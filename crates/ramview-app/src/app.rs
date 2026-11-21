use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Instant;

use ramview_core::models::{ProcessMemoryInfo, APP_MODE_COMPACT_STRIP};
use ramview_platform::api_server::LocalApiServer;
use ramview_platform::icon_extractor::IconExtractor;
use ramview_platform::process_monitor::ProcessMonitor;
use ramview_platform::ram_monitor::RAMMonitor;
use ramview_platform::settings_service::SettingsService;
use ramview_platform::shell::{ShellEvent, ShellService};
use ramview_platform::startup::StartupManager;
use ramview_platform::terminator::ProcessTerminator;
use ramview_platform::window;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateRoundRectRgn, CreateSolidBrush, DrawTextW, EndPaint, GetDC, InvalidateRect,
    ReleaseDC, SelectObject, SetBkColor, SetBkMode, SetTextColor, SetWindowRgn, UpdateWindow,
    DRAW_TEXT_FORMAT, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, HBRUSH, HDC, HFONT,
    HGDIOBJ, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetFocus, ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::hit::{hit_test, HitId, HitRegion};
use crate::layout;
use crate::native_wide;
use crate::render::{FontCache, IconCache, Surface};
use crate::treemap::{key_for, AnimatedTreemap};
use crate::ui::main_window;
use crate::ui::settings_window::{self, SettingsForm};
use crate::viewmodel::MainViewModel;
use crate::worker::{PollConfig, PollWorker, Polled};

static APP_PTR: AtomicPtr<RamViewApp> = AtomicPtr::new(std::ptr::null_mut());

const MSG_WAKE: u32 = WM_APP + 1;
const WM_MOUSELEAVE: u32 = 0x02A3;
const EM_SETMARGINS: u32 = 0x00D3;
const TIMER_TICK: usize = 1;
const EDIT_ID: usize = 1001;
const MAIN_CLASS: &str = "RAMViewMainWindow";
const SETTINGS_CLASS: &str = "RAMViewSettingsWindow";

pub struct TerminateResult {
    pub process_name: String,
    pub success: bool,
}

pub struct RamViewApp {
    pub hwnd: HWND,
    pub settings: SettingsService,
    pub vm: MainViewModel,
    pub terminator: Arc<ProcessTerminator>,
    pub api_server: Option<LocalApiServer>,
    pub shell: Option<ShellService>,
    pub worker: Option<PollWorker>,

    pub scale: f32,
    pub surface: Option<Surface>,
    pub fonts: FontCache,
    pub icons: IconCache,
    pub treemap: AnimatedTreemap,
    pub strip_scroll: i32,

    pub hits: Vec<HitRegion>,
    pub hover: Option<HitId>,
    pub tooltip_visible: bool,
    hover_since: Instant,
    pub pressed: Option<HitId>,
    pub cursor: POINT,
    pub edit_focus: bool,
    pub edit: HWND,
    pub edit_brush: HBRUSH,
    pub edit_font: HFONT,
    edit_rect: RECT,

    pub settings_hwnd: HWND,
    pub settings_surface: Option<Surface>,
    pub settings_hits: Vec<HitRegion>,
    pub settings_hover: Option<HitId>,
    pub settings_pressed: Option<HitId>,
    pub settings_drag: Option<u8>,
    pub settings_scroll: i32,
    pub settings_max_scroll: i32,
    pub settings_form: SettingsForm,

    terminate_tx: mpsc::Sender<TerminateResult>,
    terminate_rx: mpsc::Receiver<TerminateResult>,

    started: Instant,
    window_hidden: bool,
    exiting: bool,
    dwm_rounded: bool,
}

impl RamViewApp {
    fn new() -> Self {
        let settings = SettingsService::new(None);
        let vm = MainViewModel::new(settings.current_settings());
        let (terminate_tx, terminate_rx) = mpsc::channel();

        Self {
            hwnd: HWND::default(),
            settings_form: SettingsForm::from_settings(
                settings.current_settings(),
                StartupManager::is_startup_enabled(),
            ),
            settings,
            vm,
            terminator: Arc::new(ProcessTerminator::new()),
            api_server: None,
            shell: None,
            worker: None,
            scale: 1.0,
            surface: None,
            fonts: FontCache::new(1.0),
            icons: IconCache::default(),
            treemap: AnimatedTreemap::new(),
            strip_scroll: 0,
            hits: Vec::new(),
            hover: None,
            tooltip_visible: false,
            hover_since: Instant::now(),
            pressed: None,
            cursor: POINT::default(),
            edit_focus: false,
            edit: HWND::default(),
            edit_brush: HBRUSH::default(),
            edit_font: HFONT::default(),
            edit_rect: RECT::default(),
            settings_hwnd: HWND::default(),
            settings_surface: None,
            settings_hits: Vec::new(),
            settings_hover: None,
            settings_pressed: None,
            settings_drag: None,
            settings_scroll: 0,
            settings_max_scroll: 0,
            terminate_tx,
            terminate_rx,
            started: Instant::now(),
            window_hidden: false,
            exiting: false,
            dwm_rounded: false,
        }
    }

    fn now(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    // ---------------------------------------------------------------- run loop

    pub fn run() -> windows::core::Result<()> {
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }

        let instance = unsafe { GetModuleHandleW(None) }
            .map(|module| windows::Win32::Foundation::HINSTANCE(module.0))
            .unwrap_or_default();

        let mut app = RamViewApp::new();
        APP_PTR.store(&mut app as *mut RamViewApp, Ordering::Relaxed);

        // The "Run at Windows startup" entry launches us with --minimized.
        let start_minimized = std::env::args()
            .skip(1)
            .any(|argument| argument.eq_ignore_ascii_case("--minimized"));

        let hwnd = unsafe { app.create_main_window(instance) }?;
        app.hwnd = hwnd;

        app.refresh_scale();
        app.create_edit(instance);
        app.center_window(720.0, 480.0);
        app.init_services();

        if start_minimized {
            app.window_hidden = true;
        } else {
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = UpdateWindow(hwnd);
            }
        }

        unsafe {
            SetTimer(hwnd, TIMER_TICK, 16, None);
        }

        let mut message = MSG::default();
        while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }

        app.shutdown();
        APP_PTR.store(std::ptr::null_mut(), Ordering::Relaxed);
        Ok(())
    }

    unsafe fn create_main_window(
        &mut self,
        instance: windows::Win32::Foundation::HINSTANCE,
    ) -> windows::core::Result<HWND> {
        let class = native_wide(MAIN_CLASS);
        let class_name = PCWSTR::from_raw(class.as_ptr());

        let window_class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(main_wndproc),
            hInstance: instance,
            hIcon: app_icon(instance),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassW(&window_class);

        let title = native_wide("RAM VIEW");
        let style = WS_POPUP
            | WS_THICKFRAME
            | WS_CAPTION
            | WS_SYSMENU
            | WS_MINIMIZEBOX
            | WS_MAXIMIZEBOX
            | WS_CLIPCHILDREN;
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED,
            class_name,
            PCWSTR::from_raw(title.as_ptr()),
            style,
            100,
            100,
            720,
            480,
            None,
            None,
            instance,
            None,
        )?;

        let dark: i32 = 1;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const i32 as *const core::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        );

        self.update_layered_alpha(hwnd);
        self.dwm_rounded = try_round_window(hwnd);
        if self.vm.always_on_top {
            let _ = SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }

        Ok(hwnd)
    }

    fn refresh_scale(&mut self) {
        let dpi = unsafe { GetDpiForWindow(self.hwnd) };
        self.set_scale(if dpi > 0 { dpi as f32 / 96.0 } else { 1.0 });
    }

    fn set_scale(&mut self, scale: f32) {
        if (self.scale - scale).abs() < 0.001 {
            return;
        }
        self.scale = scale;
        self.fonts.set_scale(scale);
        self.surface = None;
        self.settings_surface = None;
        self.update_edit_font();
        self.recalc_layout();
    }

    fn center_window(&mut self, width_dip: f32, height_dip: f32) {
        let width = (width_dip * self.scale).round() as i32;
        let height = (height_dip * self.scale).round() as i32;
        let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let screen_height = unsafe { GetSystemMetrics(SM_CYSCREEN) };

        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                None,
                (screen_width - width) / 2,
                (screen_height - height) / 2,
                width,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }

    fn init_services(&mut self) {
        let ram_monitor = Arc::new(RAMMonitor::new());
        let process_monitor = Arc::new(ProcessMonitor::new(IconExtractor::new()));
        let terminator = self.terminator.clone();

        self.api_server = Some(LocalApiServer::start(
            ram_monitor.clone(),
            process_monitor.clone(),
            terminator.clone(),
        ));

        let waker: Arc<dyn Fn() + Send + Sync> = {
            let handle = self.hwnd.0 as isize;
            Arc::new(move || unsafe {
                let _ = PostMessageW(
                    HWND(handle as *mut core::ffi::c_void),
                    MSG_WAKE,
                    WPARAM(0),
                    LPARAM(0),
                );
            })
        };

        let config = self.poll_config();
        self.worker = Some(PollWorker::start(
            ram_monitor,
            process_monitor,
            self.settings.current_settings().refresh_interval_ms,
            config,
            waker.clone(),
        ));

        self.shell = Some(ShellService::start(
            "RAM View — Live System Memory",
            &self.settings.current_settings().global_hotkey,
            waker,
        ));
    }

    pub fn poll_config(&self) -> PollConfig {
        PollConfig {
            is_grouped: self.vm.is_grouped,
            show_system_processes: self.settings.current_settings().show_system_processes,
            filter_preset: self.vm.filter_preset.clone(),
            settings: self.settings.current_settings().clone(),
        }
    }

    pub fn trigger_poll(&self) {
        if let Some(worker) = &self.worker {
            worker.trigger(self.poll_config());
        }
    }

    fn shutdown(&mut self) {
        if let Some(mut shell) = self.shell.take() {
            shell.shutdown();
        }
        if let Some(mut worker) = self.worker.take() {
            worker.shutdown();
        }
        self.api_server.take();

        unsafe {
            if !self.edit.is_invalid() {
                let _ = DestroyWindow(self.edit);
                self.edit = HWND::default();
            }
            if !self.edit_brush.is_invalid() {
                let _ = windows::Win32::Graphics::Gdi::DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(
                    self.edit_brush.0,
                ));
                self.edit_brush = HBRUSH::default();
            }
        }
    }

    // ---------------------------------------------------------------- search box

    fn create_edit(&mut self, instance: windows::Win32::Foundation::HINSTANCE) {
        let class = native_wide("EDIT");
        let empty = native_wide("");
        let style = windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(
            WS_CHILD.0 | WS_VISIBLE.0 | ES_AUTOHSCROLL as u32,
        );

        let edit = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                PCWSTR::from_raw(class.as_ptr()),
                PCWSTR::from_raw(empty.as_ptr()),
                style,
                0,
                0,
                10,
                10,
                self.hwnd,
                HMENU(EDIT_ID as *mut core::ffi::c_void),
                instance,
                None,
            )
        };

        if let Ok(edit) = edit {
            self.edit = edit;
            self.update_edit_font();

            // Native margins inside the rounded search field.
            unsafe {
                SendMessageW(
                    edit,
                    EM_SETMARGINS,
                    WPARAM(3), // EC_LEFTMARGIN | EC_RIGHTMARGIN
                    LPARAM(((6 << 16) | 2) as isize),
                );
            }

            // Subclass to paint our own placeholder (keeps the native IME/caret).
            unsafe {
                let edit_proc: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT =
                    edit_wndproc;
                let old = SetWindowLongPtrW(edit, GWLP_WNDPROC, edit_proc as usize as isize);
                let _ = SetWindowLongPtrW(edit, GWLP_USERDATA, old);
            }

            self.edit_brush = unsafe { CreateSolidBrush(Self::edit_background(false)) };
        }
    }

    fn edit_background(focused: bool) -> COLORREF {
        // over(#090D14, white, 0x20 / 0x28), precomposited for the opaque EDIT brush.
        if focused {
            crate::render::over(
                crate::render::HUD,
                crate::render::WHITE,
                0x28,
            )
            .colorref()
        } else {
            crate::render::over(
                crate::render::HUD,
                crate::render::WHITE,
                0x20,
            )
            .colorref()
        }
    }

    fn update_edit_font(&mut self) {
        if self.edit.is_invalid() {
            return;
        }
        self.edit_font = self.fonts.get(12.0, false).hfont;
        unsafe {
            SendMessageW(
                self.edit,
                WM_SETFONT,
                WPARAM(self.edit_font.0 as usize),
                LPARAM(1),
            );
        }
    }

    fn move_edit(&mut self, rect: RECT) {
        if self.edit.is_invalid() {
            return;
        }
        if rect.left == self.edit_rect.left
            && rect.top == self.edit_rect.top
            && rect.right == self.edit_rect.right
            && rect.bottom == self.edit_rect.bottom
        {
            return;
        }
        self.edit_rect = rect;
        unsafe {
            let _ = SetWindowPos(
                self.edit,
                None,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            let _ = InvalidateRect(self.hwnd, None, false);
        }
    }

    fn edit_text(&self) -> String {
        if self.edit.is_invalid() {
            return String::new();
        }
        unsafe {
            let length = GetWindowTextLengthW(self.edit);
            let mut buffer = vec![0u16; (length + 1) as usize];
            let copied = GetWindowTextW(self.edit, &mut buffer);
            String::from_utf16_lossy(&buffer[..copied.max(0) as usize])
        }
    }

    fn update_search(&mut self) {
        let text = self.edit_text();
        if text == self.vm.search_text {
            return;
        }
        self.vm.search_text = text;
        self.vm.apply_search_filter_highlighting();
        self.treemap.sync(&self.vm.treemap_items, self.now());
        self.invalidate();
    }

    // ---------------------------------------------------------------- drawing

    pub fn invalidate(&self) {
        if !self.hwnd.is_invalid() {
            unsafe {
                let _ = InvalidateRect(self.hwnd, None, false);
            }
        }
    }

    fn paint_main(&mut self, hwnd: HWND) {
        unsafe {
            let mut paint_struct = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut paint_struct);

            let mut client = RECT::default();
            let _ = GetClientRect(hwnd, &mut client);
            let width = client.right - client.left;
            let height = client.bottom - client.top;

            if width > 0 && height > 0 {
                let needs_surface = match &self.surface {
                    Some(surface) => surface.width != width || surface.height != height,
                    None => true,
                };
                if needs_surface {
                    self.surface = Surface::new(width, height);
                }

                if let Some(surface) = self.surface.take() {
                    let edit_rect = main_window::paint(self, &surface);
                    surface.blit_to(hdc);
                    self.surface = Some(surface);
                    match edit_rect {
                        Some(rect) => {
                            self.move_edit(rect);
                            if !self.edit.is_invalid() {
                                let _ = ShowWindow(self.edit, SW_SHOW);
                            }
                        }
                        None => {
                            if !self.edit.is_invalid() {
                                let _ = ShowWindow(self.edit, SW_HIDE);
                            }
                        }
                    }
                }
            }

            let _ = EndPaint(hwnd, &paint_struct);
        }
    }

    fn paint_settings(&mut self, hwnd: HWND) {
        unsafe {
            let mut paint_struct = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut paint_struct);

            let mut client = RECT::default();
            let _ = GetClientRect(hwnd, &mut client);
            let width = client.right - client.left;
            let height = client.bottom - client.top;

            if width > 0 && height > 0 {
                let needs_surface = match &self.settings_surface {
                    Some(surface) => surface.width != width || surface.height != height,
                    None => true,
                };
                if needs_surface {
                    self.settings_surface = Surface::new(width, height);
                }

                if let Some(surface) = self.settings_surface.take() {
                    settings_window::paint(self, &surface);
                    surface.blit_to(hdc);
                    self.settings_surface = Some(surface);
                }
            }

            let _ = EndPaint(hwnd, &paint_struct);
        }
    }

    // ---------------------------------------------------------------- layout / state

    fn client_size(&self) -> (i32, i32) {
        unsafe {
            let mut client = RECT::default();
            let _ = GetClientRect(self.hwnd, &mut client);
            (client.right, client.bottom)
        }
    }

    fn recalc_layout(&mut self) {
        let (width, height) = self.client_size();
        if width <= 0 || height <= 0 {
            return;
        }
        let compact = self.vm.app_mode == APP_MODE_COMPACT_STRIP;
        let layout = layout::compute(width, height, self.scale, compact, self.vm.click_through);
        let center_width = (layout.center.right - layout.center.left) as f64 / self.scale as f64;
        let center_height = (layout.center.bottom - layout.center.top) as f64 / self.scale as f64;

        if center_width > 20.0 && center_height > 20.0 {
            self.vm.canvas_width = center_width;
            self.vm.canvas_height = center_height;
        }

        self.vm
            .recalculate_treemap_layout(self.settings.current_settings());
        self.treemap.sync(&self.vm.treemap_items, self.now());
        self.invalidate();
    }

    fn apply_polled(&mut self, polled: Polled) {
        self.vm.snapshot = Some(polled.snapshot);
        self.vm.processes = polled.processes;
        self.vm.visual_state = polled.visual_state;
        self.recalc_layout();
        self.update_tray_tooltip();
    }

    fn update_tray_tooltip(&self) {
        let Some(snapshot) = &self.vm.snapshot else {
            return;
        };
        let tooltip = format!(
            "RAM View: {} / {} ({:.0}%)",
            snapshot.formatted_used(),
            snapshot.formatted_total(),
            snapshot.system_memory_usage_percentage()
        );
        if let Some(shell) = &self.shell {
            shell.update_tooltip(&tooltip);
        }
    }

    fn drain_events(&mut self) {
        let mut polled = Vec::new();
        if let Some(worker) = &self.worker {
            while let Some(item) = worker.try_recv() {
                polled.push(item);
            }
        }
        for item in polled {
            self.apply_polled(item);
        }

        let mut events = Vec::new();
        if let Some(shell) = &self.shell {
            while let Some(event) = shell.try_recv() {
                events.push(event);
            }
        }
        for event in events {
            self.handle_shell_event(event);
        }

        let mut results = Vec::new();
        while let Ok(result) = self.terminate_rx.try_recv() {
            results.push(result);
        }
        for result in results {
            if result.success {
                self.vm.status_message = format!("Terminated {}", result.process_name);
                self.vm.selected_process = None;
                self.trigger_poll();
            } else {
                self.alert(
                    "Termination Failed",
                    &format!(
                        "Could not terminate {}. You may lack sufficient privileges.",
                        result.process_name
                    ),
                );
            }
            self.invalidate();
        }
    }

    fn handle_shell_event(&mut self, event: ShellEvent) {
        match event {
            ShellEvent::ToggleShowHide => {
                if self.window_hidden || !unsafe { IsWindowVisible(self.hwnd).as_bool() } {
                    unsafe {
                        let _ = ShowWindow(self.hwnd, SW_SHOW);
                        let _ = SetForegroundWindow(self.hwnd);
                    }
                    self.window_hidden = false;
                } else {
                    self.hide_window(false);
                }
            }
            ShellEvent::ToggleCompact => {
                self.vm.toggle_mode();
                self.resize_for_mode();
            }
            ShellEvent::ToggleGhost => {
                self.vm.toggle_click_through();
                self.apply_click_through();
                self.recalc_layout();
            }
            ShellEvent::OpenSettings => self.open_settings(),
            ShellEvent::Refresh => self.trigger_poll(),
            ShellEvent::Exit => {
                self.exiting = true;
                self.shutdown();
                unsafe {
                    let _ = DestroyWindow(self.hwnd);
                }
            }
        }
        self.invalidate();
    }

    fn hide_window(&mut self, balloon: bool) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
        self.window_hidden = true;
        self.hover = None;
        self.tooltip_visible = false;
        self.close_settings();
        if balloon {
            if let Some(shell) = &self.shell {
                shell.show_balloon(
                    "RAM View",
                    "RAM View is running in the background. Access anytime from the system tray.",
                );
            }
        }
    }

    fn resize_for_mode(&mut self) {
        let compact = self.vm.app_mode == APP_MODE_COMPACT_STRIP;
        let height = ((if compact { 120.0 } else { 460.0 }) * self.scale).round() as i32;
        let (width, _) = self.client_size();
        if width <= 0 {
            return;
        }
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                None,
                0,
                0,
                width,
                height,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }

    fn update_layered_alpha(&self, hwnd: HWND) {
        let alpha = (self.vm.window_opacity.clamp(0.05, 1.0) * 255.0).round() as u8;
        unsafe {
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
        }
    }

    fn apply_topmost(&self) {
        unsafe {
            let target = if self.vm.always_on_top {
                HWND_TOPMOST
            } else {
                HWND_NOTOPMOST
            };
            let _ = SetWindowPos(
                self.hwnd,
                target,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }

    fn apply_click_through(&self) {
        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::{
                GetWindowLongW, SetWindowLongW, GWL_EXSTYLE, WS_EX_TRANSPARENT,
            };
            let mut style = GetWindowLongW(self.hwnd, GWL_EXSTYLE);
            let flag = WS_EX_TRANSPARENT.0 as i32;
            if self.vm.click_through {
                style |= flag;
            } else {
                style &= !flag;
            }
            SetWindowLongW(self.hwnd, GWL_EXSTYLE, style);
        }
    }

    // ---------------------------------------------------------------- settings window

    fn open_settings(&mut self) {
        self.settings_form = SettingsForm::from_settings(
            self.settings.current_settings(),
            StartupManager::is_startup_enabled(),
        );
        self.settings_scroll = 0;
        self.settings_surface = None;

        if self.settings_hwnd.is_invalid() {
            unsafe {
                let instance = GetModuleHandleW(None)
                    .map(|module| windows::Win32::Foundation::HINSTANCE(module.0))
                    .unwrap_or_default();
                if let Some(hwnd) = self.create_settings_window(instance) {
                    self.settings_hwnd = hwnd;
                }
            }
        }

        if !self.settings_hwnd.is_invalid() {
            unsafe {
                let _ = ShowWindow(self.settings_hwnd, SW_SHOW);
                let _ = SetForegroundWindow(self.settings_hwnd);
                let _ = InvalidateRect(self.settings_hwnd, None, false);
            }
        }
    }

    unsafe fn create_settings_window(
        &self,
        instance: windows::Win32::Foundation::HINSTANCE,
    ) -> Option<HWND> {
        let class = native_wide(SETTINGS_CLASS);
        let class_name = PCWSTR::from_raw(class.as_ptr());

        let window_class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(settings_wndproc),
            hInstance: instance,
            hIcon: app_icon(instance),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassW(&window_class);

        let width = (440.0 * self.scale).round() as i32;
        let height = (460.0 * self.scale).round() as i32;
        let screen_width = GetSystemMetrics(SM_CXSCREEN);
        let screen_height = GetSystemMetrics(SM_CYSCREEN);

        let title = native_wide("RAM View — Settings");
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class_name,
            PCWSTR::from_raw(title.as_ptr()),
            WS_POPUP,
            (screen_width - width) / 2,
            (screen_height - height) / 2,
            width,
            height,
            self.hwnd,
            None,
            instance,
            None,
        )
        .ok()?;
        if !try_round_window(hwnd) {
            let _ = SetWindowRgn(
                hwnd,
                CreateRoundRectRgn(
                    0,
                    0,
                    width + 1,
                    height + 1,
                    (24.0 * self.scale) as i32,
                    (24.0 * self.scale) as i32,
                ),
                true,
            );
        }
        let dark: i32 = 1;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const i32 as *const core::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        );

        Some(hwnd)
    }

    fn close_settings(&mut self) {
        if !self.settings_hwnd.is_invalid() {
            unsafe {
                let _ = ShowWindow(self.settings_hwnd, SW_HIDE);
            }
        }
        self.settings_drag = None;
        self.settings_pressed = None;
    }

    fn save_settings(&mut self) {
        let form = self.settings_form.clone();
        let mut settings = self.settings.current_settings().clone();

        settings.refresh_interval_ms = form.refresh_interval_ms.round() as i32;
        settings.window_opacity = form.window_opacity;
        settings.anti_dominance_max_area_share = form.anti_dominance;
        settings.always_on_top = form.always_on_top;
        settings.click_through = form.click_through;
        settings.lock_position = form.lock_position;
        settings.is_grouped = form.is_grouped;
        settings.show_system_processes = form.show_system;
        settings.start_with_windows = form.start_with_windows;

        StartupManager::set_startup(settings.start_with_windows);
        self.settings.save_settings(settings.clone());

        self.vm.window_opacity = settings.window_opacity;
        self.vm.always_on_top = settings.always_on_top;
        self.vm.click_through = settings.click_through;
        self.vm.lock_position = settings.lock_position;
        self.vm.is_grouped = settings.is_grouped;

        self.update_layered_alpha(self.hwnd);
        self.apply_topmost();
        self.apply_click_through();
        self.trigger_poll();
        self.recalc_layout();
        self.close_settings();
        self.invalidate();
    }

    // ---------------------------------------------------------------- interaction

    fn update_hover(&mut self, x: i32, y: i32) {
        let hit = hit_test(&self.hits, x, y);
        if hit != self.hover {
            self.hover = hit;
            self.hover_since = Instant::now();
            self.tooltip_visible = false;
            unsafe {
                if !self.hwnd.is_invalid() {
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: self.hwnd,
                        dwHoverTime: 0,
                    };
                    let _ = TrackMouseEvent(&mut track);
                }
            }
            self.invalidate();
        }
    }

    fn activate(&mut self, id: HitId) {
        match id {
            HitId::ToggleMode => {
                self.vm.toggle_mode();
                self.resize_for_mode();
            }
            HitId::Settings => self.open_settings(),
            HitId::Coffee => window::open_url("https://buymeacoffee.com/ahmadhassan806"),
            HitId::Minimize => self.hide_window(true),
            HitId::CloseWindow => self.hide_window(false),
            HitId::Chip(preset) => {
                if self.vm.filter_preset != preset {
                    self.vm.filter_preset = preset.to_string();
                    self.trigger_poll();
                }
            }
            HitId::ToggleGroup => {
                self.vm.toggle_group();
                self.trigger_poll();
            }
            HitId::TogglePin => {
                self.vm.toggle_always_on_top();
                self.apply_topmost();
            }
            HitId::ToggleGhost => {
                self.vm.toggle_click_through();
                self.apply_click_through();
                self.recalc_layout();
            }
            HitId::ToggleLock => {
                self.vm.toggle_lock_position();
            }
            HitId::Box(key) => {
                if let Some(process) = self
                    .vm
                    .processes
                    .iter()
                    .find(|process| key_for(process) == key)
                {
                    self.vm.selected_process = Some(process.clone());
                }
            }
            HitId::StripItem(pid) => {
                if let Some(process) = self
                    .vm
                    .processes
                    .iter()
                    .find(|process| process.process_id == pid)
                {
                    self.vm.selected_process = Some(process.clone());
                }
            }
            HitId::ModalClose => {
                self.vm.selected_process = None;
            }
            HitId::ModalCopy => {
                if let Some(text) = self.vm.copy_process_info_text() {
                    ramview_platform::clipboard::set_text(self.hwnd, &text);
                    if let Some(process) = &self.vm.selected_process {
                        self.vm.status_message =
                            format!("Copied {} info to clipboard", process.process_name);
                    }
                }
            }
            HitId::ModalOpen => {
                if let Some(process) = &self.vm.selected_process {
                    if let Some(path) = &process.executable_path {
                        if std::path::Path::new(path).is_file() {
                            window::open_file_location(path);
                        }
                    }
                }
            }
            HitId::ModalEnd => {
                if let Some(process) = self.vm.selected_process.clone() {
                    self.request_terminate(process);
                }
            }
            _ => {}
        }

        self.invalidate();
    }

    fn request_terminate(&mut self, process: ProcessMemoryInfo) {
        if let Err(reason) =
            self.terminator
                .can_terminate(process.process_id, &process.process_name)
        {
            self.alert("Protected System Process", &reason);
            return;
        }

        let message = native_wide(&format!(
            "Are you sure you want to end process '{}' (PID: {})?",
            process.process_name, process.process_id
        ));
        let title = native_wide("End Process Confirmation");

        let answer = unsafe {
            MessageBoxW(
                self.hwnd,
                PCWSTR::from_raw(message.as_ptr()),
                PCWSTR::from_raw(title.as_ptr()),
                MB_YESNO | MB_ICONWARNING,
            )
        };
        if answer != IDYES {
            return;
        }

        let terminator = self.terminator.clone();
        let tx = self.terminate_tx.clone();
        let handle = self.hwnd.0 as isize;
        let pid = process.process_id;
        let name = process.process_name.clone();

        std::thread::spawn(move || {
            let success = terminator.terminate_process(pid);
            let _ = tx.send(TerminateResult {
                process_name: name,
                success,
            });
            unsafe {
                let _ = PostMessageW(
                    HWND(handle as *mut core::ffi::c_void),
                    MSG_WAKE,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        });
    }

    fn alert(&self, title: &str, message: &str) {
        let message = native_wide(message);
        let title = native_wide(title);
        unsafe {
            MessageBoxW(
                self.hwnd,
                PCWSTR::from_raw(message.as_ptr()),
                PCWSTR::from_raw(title.as_ptr()),
                MB_OK | MB_ICONINFORMATION,
            );
        }
    }

    // ---------------------------------------------------------------- message pump

    unsafe fn handle_main(
        &mut self,
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Option<LRESULT> {
        match message {
            WM_PAINT => {
                self.paint_main(hwnd);
                Some(LRESULT(0))
            }
            WM_ERASEBKGND => Some(LRESULT(1)),
            // WS_THICKFRAME gives native resize borders; drop the non-client frame
            // so the HUD still paints edge to edge.
            WM_NCCALCSIZE => {
                if wparam.0 != 0 {
                    Some(LRESULT(0))
                } else {
                    None
                }
            }
            WM_NCPAINT => Some(LRESULT(0)),
            WM_NCHITTEST => {
                let x = (lparam.0 as u32 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 as u32 >> 16) & 0xFFFF) as i16 as i32;
                let mut window = RECT::default();
                let _ = GetWindowRect(hwnd, &mut window);
                let edge = (6.0 * self.scale).round().max(4.0) as i32;

                let on_left = x - window.left < edge;
                let on_right = window.right - x < edge;
                let on_top = y - window.top < edge;
                let on_bottom = window.bottom - y < edge;

                let hit = match (on_left, on_right, on_top, on_bottom) {
                    (true, _, true, _) => HTTOPLEFT,
                    (_, true, true, _) => HTTOPRIGHT,
                    (true, _, _, true) => HTBOTTOMLEFT,
                    (_, true, _, true) => HTBOTTOMRIGHT,
                    (true, _, _, _) => HTLEFT,
                    (_, true, _, _) => HTRIGHT,
                    (_, _, true, _) => HTTOP,
                    (_, _, _, true) => HTBOTTOM,
                    _ => HTCLIENT,
                };
                Some(LRESULT(hit as isize))
            }
            WM_SIZE => {
                let width = loword(lparam.0) as i32;
                let height = hiword(lparam.0) as i32;
                if !self.dwm_rounded {
                    let _ = SetWindowRgn(
                        hwnd,
                        CreateRoundRectRgn(
                            0,
                            0,
                            width + 1,
                            height + 1,
                            (28.0 * self.scale) as i32,
                            (28.0 * self.scale) as i32,
                        ),
                        true,
                    );
                }
                self.recalc_layout();
                Some(LRESULT(0))
            }
            WM_DPICHANGED => {
                let dpi = (wparam.0 & 0xFFFF) as u32;
                self.set_scale(dpi as f32 / 96.0);
                let suggested = &*(lparam.0 as *const RECT);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    suggested.left,
                    suggested.top,
                    suggested.right - suggested.left,
                    suggested.bottom - suggested.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                self.recalc_layout();
                Some(LRESULT(0))
            }
            WM_MOUSEMOVE => {
                let x = loword(lparam.0) as i16 as i32;
                let y = hiword(lparam.0) as i16 as i32;
                self.cursor = POINT { x, y };
                self.update_hover(x, y);
                Some(LRESULT(0))
            }
            WM_MOUSELEAVE => {
                self.hover = None;
                self.tooltip_visible = false;
                self.invalidate();
                Some(LRESULT(0))
            }
            WM_LBUTTONDOWN => {
                let x = loword(lparam.0) as i16 as i32;
                let y = hiword(lparam.0) as i16 as i32;
                self.pressed = hit_test(&self.hits, x, y);
                if self.pressed.is_some() {
                    let _ = SetCapture(hwnd);
                } else if !self.vm.lock_position {
                    let _ = ReleaseCapture();
                    let _ = SendMessageW(hwnd, WM_NCLBUTTONDOWN, WPARAM(HTCAPTION as usize), LPARAM(0));
                }
                Some(LRESULT(0))
            }
            WM_LBUTTONUP => {
                let _ = ReleaseCapture();
                let x = loword(lparam.0) as i16 as i32;
                let y = hiword(lparam.0) as i16 as i32;
                let released = hit_test(&self.hits, x, y);
                let pressed = self.pressed.take();
                if let (Some(pressed), Some(released)) = (pressed, released) {
                    if pressed == released {
                        self.activate(pressed);
                    }
                }
                Some(LRESULT(0))
            }
            WM_MOUSEWHEEL => {
                let delta = (wparam.0 >> 16) as i16 as i32;
                if self.vm.app_mode == APP_MODE_COMPACT_STRIP {
                    self.strip_scroll = (self.strip_scroll - delta / 120 * 48).max(0);
                    self.invalidate();
                }
                Some(LRESULT(0))
            }
            WM_SETCURSOR => {
                let hit_code = loword(lparam.0);
                if hit_code == HTCLIENT as u16 {
                    let interactive = self.hover.is_some();
                    let cursor = if interactive {
                        LoadCursorW(None, IDC_HAND).unwrap_or_default()
                    } else {
                        LoadCursorW(None, IDC_ARROW).unwrap_or_default()
                    };
                    unsafe {
                        SetCursor(cursor);
                    }
                    return Some(LRESULT(1));
                }
                None
            }
            WM_TIMER => {
                if wparam.0 == TIMER_TICK {
                    if IsWindowVisible(hwnd).as_bool() {
                        let mut repaint = self.treemap.update(self.now());
                        if !self.tooltip_visible {
                            if let Some(hit) = &self.hover {
                                if has_tooltip(hit)
                                    && self.hover_since.elapsed().as_millis() >= 400
                                {
                                    self.tooltip_visible = true;
                                    repaint = true;
                                }
                            }
                        }
                        if repaint {
                            let _ = InvalidateRect(hwnd, None, false);
                        }
                    }
                    return Some(LRESULT(0));
                }
                None
            }
            MSG_WAKE => {
                self.drain_events();
                let _ = InvalidateRect(hwnd, None, false);
                Some(LRESULT(0))
            }
            WM_COMMAND => {
                let id = loword(wparam.0 as isize);
                let code = hiword(wparam.0 as isize);
                if id == EDIT_ID as u16 {
                    match code {
                        0x0100 => {
                            self.edit_focus = true;
                            let _ = windows::Win32::Graphics::Gdi::DeleteObject(
                                windows::Win32::Graphics::Gdi::HGDIOBJ(self.edit_brush.0),
                            );
                            self.edit_brush =
                                CreateSolidBrush(Self::edit_background(true));
                            let _ =
                                windows::Win32::Graphics::Gdi::InvalidateRect(hwnd, None, false);
                            let _ = InvalidateRect(self.edit, None, true);
                        }
                        0x0200 => {
                            self.edit_focus = false;
                            let _ = windows::Win32::Graphics::Gdi::DeleteObject(
                                windows::Win32::Graphics::Gdi::HGDIOBJ(self.edit_brush.0),
                            );
                            self.edit_brush =
                                CreateSolidBrush(Self::edit_background(false));
                            let _ =
                                windows::Win32::Graphics::Gdi::InvalidateRect(hwnd, None, false);
                            let _ = InvalidateRect(self.edit, None, true);
                        }
                        0x0300 => {
                            self.update_search();
                            let _ = InvalidateRect(self.edit, None, true);
                        }
                        _ => {}
                    }
                }
                Some(LRESULT(0))
            }
            WM_CTLCOLOREDIT => {
                let hdc = HDC(lparam.0 as *mut core::ffi::c_void);
                unsafe {
                    SetTextColor(hdc, COLORREF(0x00FF_FFFF));
                    SetBkColor(hdc, Self::edit_background(self.edit_focus));
                }
                Some(LRESULT(self.edit_brush.0 as isize))
            }
            WM_GETMINMAXINFO => {
                let info = &mut *(lparam.0 as *mut MINMAXINFO);
                let compact = self.vm.app_mode == APP_MODE_COMPACT_STRIP;
                info.ptMinTrackSize.x = (340.0 * self.scale) as i32;
                info.ptMinTrackSize.y =
                    ((if compact { 60.0 } else { 180.0 }) * self.scale) as i32;
                Some(LRESULT(0))
            }
            WM_CLOSE => {
                if !self.exiting {
                    self.hide_window(false);
                }
                Some(LRESULT(0))
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                Some(LRESULT(0))
            }
            WM_SHOWWINDOW => {
                if wparam.0 != 0 {
                    self.window_hidden = false;
                }
                let _ = SetTimer(hwnd, TIMER_TICK, 16, None);
                Some(LRESULT(0))
            }
            _ => None,
        }
    }

    unsafe fn handle_settings(
        &mut self,
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Option<LRESULT> {
        match message {
            WM_PAINT => {
                self.paint_settings(hwnd);
                Some(LRESULT(0))
            }
            WM_ERASEBKGND => Some(LRESULT(1)),
            WM_MOUSEMOVE => {
                let x = loword(lparam.0) as i16 as i32;
                let y = hiword(lparam.0) as i16 as i32;
                if let Some(index) = self.settings_drag {
                    self.settings_set_slider(index, x);
                    let _ = InvalidateRect(hwnd, None, false);
                } else {
                    let hit = hit_test(&self.settings_hits, x, y);
                    if hit != self.settings_hover {
                        self.settings_hover = hit;
                        let _ = InvalidateRect(hwnd, None, false);
                    }
                }
                Some(LRESULT(0))
            }
            WM_LBUTTONDOWN => {
                let x = loword(lparam.0) as i16 as i32;
                let y = hiword(lparam.0) as i16 as i32;
                let hit = hit_test(&self.settings_hits, x, y);
                if let Some(HitId::SettingsSlider(index)) = hit {
                    self.settings_drag = Some(index);
                    let _ = SetCapture(hwnd);
                    self.settings_set_slider(index, x);
                }
                self.settings_pressed = hit;
                let _ = InvalidateRect(hwnd, None, false);
                Some(LRESULT(0))
            }
            WM_LBUTTONUP => {
                let x = loword(lparam.0) as i16 as i32;
                let y = hiword(lparam.0) as i16 as i32;
                let _ = ReleaseCapture();
                self.settings_drag = None;
                let released = hit_test(&self.settings_hits, x, y);
                let pressed = self.settings_pressed.take();
                if let (Some(pressed), Some(released)) = (pressed, released) {
                    if pressed == released {
                        self.activate_settings(pressed);
                    }
                }
                let _ = InvalidateRect(hwnd, None, false);
                Some(LRESULT(0))
            }
            WM_MOUSEWHEEL => {
                let delta = (wparam.0 >> 16) as i16 as i32;
                self.settings_scroll =
                    (self.settings_scroll - delta / 120 * 40).clamp(0, self.settings_max_scroll);
                let _ = InvalidateRect(hwnd, None, false);
                Some(LRESULT(0))
            }
            WM_SETCURSOR => {
                let hit_code = loword(lparam.0);
                if hit_code == HTCLIENT as u16 {
                    let interactive = self.settings_hover.is_some();
                    let cursor = if interactive {
                        LoadCursorW(None, IDC_HAND).unwrap_or_default()
                    } else {
                        LoadCursorW(None, IDC_ARROW).unwrap_or_default()
                    };
                    SetCursor(cursor);
                    return Some(LRESULT(1));
                }
                None
            }
            WM_CLOSE => {
                self.close_settings();
                Some(LRESULT(0))
            }
            _ => None,
        }
    }

    fn settings_set_slider(&mut self, index: u8, x: i32) {
        settings_window::set_slider_from_x(self, index, x);
    }

    fn activate_settings(&mut self, id: HitId) {
        match id {
            HitId::SettingsClose | HitId::SettingsCancel => self.close_settings(),
            HitId::SettingsSave => self.save_settings(),
            HitId::SettingsCoffee => window::open_url("https://buymeacoffee.com/ahmadhassan806"),
            HitId::SettingsCheck(index) => settings_window::toggle_check(self, index),
            _ => {}
        }
        if !self.settings_hwnd.is_invalid() {
            unsafe {
                let _ = InvalidateRect(self.settings_hwnd, None, false);
            }
        }
    }
}

fn has_tooltip(id: &HitId) -> bool {
    matches!(
        id,
        HitId::Box(_)
            | HitId::StripItem(_)
            | HitId::ToggleMode
            | HitId::Settings
            | HitId::Coffee
            | HitId::Minimize
            | HitId::CloseWindow
            | HitId::ToggleGroup
            | HitId::TogglePin
            | HitId::ToggleGhost
            | HitId::ToggleLock
    )
}

fn app_icon(instance: windows::Win32::Foundation::HINSTANCE) -> HICON {
    // Icon resource id 1 is emitted by ramview.rc.
    unsafe { LoadIconW(instance, PCWSTR::from_raw(1 as *const u16)).unwrap_or_default() }
}

/// Windows 11 lets DWM round the window with anti-aliased corners. Falls back to
/// a window region on older builds.
fn try_round_window(hwnd: HWND) -> bool {
    unsafe {
        let preference = DWMWCP_ROUND;
        let rounded = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &preference as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        )
        .is_ok();

        if rounded {
            let border = crate::render::Rgb::new(0x2E, 0x36, 0x42).colorref();
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_BORDER_COLOR,
                &border as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
            );
        }

        rounded
    }
}

unsafe extern "system" fn edit_wndproc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let old_raw = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
    let old_proc: Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT> =
        std::mem::transmute(old_raw);

    let result = match old_proc {
        Some(proc) => CallWindowProcW(Some(proc), hwnd, message, wparam, lparam),
        None => DefWindowProcW(hwnd, message, wparam, lparam),
    };

    // Draw the placeholder ourselves so it matches the HUD typography.
    if message == WM_PAINT && GetWindowTextLengthW(hwnd) == 0 && GetFocus() != hwnd {
        let hdc = GetDC(hwnd);
        if !hdc.is_invalid() {
            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);
            rect.left += 2;
            rect.right -= 4;

            let font_handle = SendMessageW(hwnd, WM_GETFONT, WPARAM(0), LPARAM(0)).0;
            if font_handle != 0 {
                SelectObject(hdc, HGDIOBJ(font_handle as *mut core::ffi::c_void));
            }

            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, COLORREF(0x008B_7464));
            let mut text = native_wide("Filter processes...");
            DrawTextW(
                hdc,
                &mut text,
                &mut rect,
                DRAW_TEXT_FORMAT(
                    DT_SINGLELINE.0 | DT_VCENTER.0 | DT_LEFT.0 | DT_NOPREFIX.0,
                ),
            );
            let _ = ReleaseDC(hwnd, hdc);
        }
    }

    if message == WM_SETFOCUS || message == WM_KILLFOCUS {
        let _ = InvalidateRect(hwnd, None, true);
    }

    result
}

fn loword(value: isize) -> u16 {
    (value as u32 & 0xFFFF) as u16
}

fn hiword(value: isize) -> u16 {
    ((value as u32 >> 16) & 0xFFFF) as u16
}

unsafe extern "system" fn main_wndproc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let pointer = APP_PTR.load(Ordering::Relaxed);
    if pointer.is_null() {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    let app = &mut *pointer;
    match app.handle_main(hwnd, message, wparam, lparam) {
        Some(result) => result,
        None => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe extern "system" fn settings_wndproc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let pointer = APP_PTR.load(Ordering::Relaxed);
    if pointer.is_null() {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    let app = &mut *pointer;
    match app.handle_settings(hwnd, message, wparam, lparam) {
        Some(result) => result,
        None => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}
