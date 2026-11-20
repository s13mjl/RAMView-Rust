use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;

use ramview_core::hotkey::{parse_chord, MOD_NOREPEAT};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    CloseHandle, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM, WAIT_OBJECT_0,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{CreateEventW, ResetEvent, SetEvent};
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIM_ADD, NIM_DELETE,
    NIM_MODIFY, NOTIFYICONDATAW, NOTIFYICONDATAW_0,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, GetCursorPos, LoadCursorW, LoadIconW, MF_SEPARATOR, MF_STRING,
    MSG_WAIT_FOR_MULTIPLE_OBJECTS_EX_FLAGS, MsgWaitForMultipleObjectsEx, PM_REMOVE, PeekMessageW,
    PostMessageW, PostQuitMessage, RegisterClassW, SetForegroundWindow, TrackPopupMenu,
    TranslateMessage, HICON, HMENU, IDC_ARROW, IDI_APPLICATION, QS_ALLINPUT, TPM_BOTTOMALIGN,
    TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSW,
    WM_APP, WM_DESTROY, WM_HOTKEY, WM_LBUTTONDBLCLK, WM_NULL, WM_QUIT, WM_RBUTTONUP,
};

use crate::native::{copy_wide, pcwstr, truncate_utf16, wide};

const HOTKEY_ID: i32 = 9001;
const WM_TRAY: u32 = WM_APP + 1;
const TRAY_ICON_ID: u32 = 1;
const MWMO_INPUTAVAILABLE: MSG_WAIT_FOR_MULTIPLE_OBJECTS_EX_FLAGS =
    MSG_WAIT_FOR_MULTIPLE_OBJECTS_EX_FLAGS(4);

#[derive(Clone, Copy, Debug)]
pub enum ShellEvent {
    ToggleShowHide,
    ToggleCompact,
    ToggleGhost,
    OpenSettings,
    Refresh,
    Exit,
}

enum ShellCommand {
    SetTooltip(String),
    ShowBalloon(String, String),
    Shutdown,
}

static EVENT_TX: OnceLock<Mutex<Sender<ShellEvent>>> = OnceLock::new();
static WAKE_UI: OnceLock<Arc<dyn Fn() + Send + Sync>> = OnceLock::new();
static MENU_HANDLE: AtomicIsize = AtomicIsize::new(0);

fn send_event(event: ShellEvent) {
    if let Some(lock) = EVENT_TX.get() {
        if let Ok(tx) = lock.lock() {
            let _ = tx.send(event);
        }
    }
    if let Some(wake) = WAKE_UI.get() {
        wake();
    }
}

/// System tray icon, its context menu, balloon tips, and the global hotkey (§15, §16).
/// Owns a hidden window and message loop on a dedicated thread.
pub struct ShellService {
    command_tx: Sender<ShellCommand>,
    event_rx: Receiver<ShellEvent>,
    handle: Option<JoinHandle<()>>,
    last_tooltip: Mutex<String>,
    wake_event: Arc<AtomicIsize>,
}

impl ShellService {
    pub fn start(
        initial_tooltip: &str,
        hotkey_chord: &str,
        wake_ui: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let (command_tx, command_rx) = channel::<ShellCommand>();
        let (event_tx, event_rx) = channel::<ShellEvent>();

        let _ = WAKE_UI.set(wake_ui);

        let initial = truncate_utf16(initial_tooltip, 63);
        let chord = hotkey_chord.to_string();
        let wake_event = Arc::new(AtomicIsize::new(0));

        let thread_wake_event = wake_event.clone();
        let handle = std::thread::spawn(move || {
            run_shell_thread(command_rx, event_tx, initial, &chord, thread_wake_event);
        });

        Self {
            command_tx,
            event_rx,
            handle: Some(handle),
            last_tooltip: Mutex::new(String::new()),
            wake_event,
        }
    }

    /// Signals the tray thread so queued commands are applied immediately.
    fn signal(&self) {
        let handle = self.wake_event.load(Ordering::Relaxed);
        if handle != 0 {
            unsafe {
                let _ = SetEvent(HANDLE(handle as *mut core::ffi::c_void));
            }
        }
    }

    /// Rate-limited tooltip updates: unchanged text is not pushed to the shell (§16).
    pub fn update_tooltip(&self, tooltip: &str) {
        let mut last = match self.last_tooltip.lock() {
            Ok(last) => last,
            Err(_) => return,
        };
        if *last == tooltip {
            return;
        }
        *last = tooltip.to_string();
        let sanitized = truncate_utf16(tooltip, 63);
        let _ = self.command_tx.send(ShellCommand::SetTooltip(sanitized));
        self.signal();
    }

    pub fn show_balloon(&self, title: &str, text: &str) {
        let _ = self
            .command_tx
            .send(ShellCommand::ShowBalloon(title.to_string(), text.to_string()));
        self.signal();
    }

    pub fn try_recv(&self) -> Option<ShellEvent> {
        match self.event_rx.try_recv() {
            Ok(event) => Some(event),
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => None,
        }
    }

    pub fn shutdown(&mut self) {
        let _ = self.command_tx.send(ShellCommand::Shutdown);
        self.signal();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for ShellService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run_shell_thread(
    command_rx: Receiver<ShellCommand>,
    event_tx: Sender<ShellEvent>,
    initial_tooltip: String,
    hotkey_chord: &str,
    wake_event_flag: Arc<AtomicIsize>,
) {
    let _ = EVENT_TX.set(Mutex::new(event_tx));

    unsafe {
        let class_name = wide("RAMViewShellWindowClass");
        let hinstance = GetModuleHandleW(None)
            .map(|module| HINSTANCE(module.0))
            .unwrap_or_default();

        let window_class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: hinstance,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: pcwstr(&class_name),
            ..Default::default()
        };

        if RegisterClassW(&window_class) == 0 {
            return;
        }

        let window_title = wide("RAMView Shell");
        let hwnd = match CreateWindowExW(
            WINDOW_EX_STYLE(0),
            pcwstr(&class_name),
            pcwstr(&window_title),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            None,
            None,
            hinstance,
            None,
        ) {
            Ok(hwnd) => hwnd,
            Err(_) => return,
        };

        let icon = match LoadIconW(None, IDI_APPLICATION) {
            Ok(icon) => icon,
            Err(_) => HICON(std::ptr::null_mut()),
        };

        let mut icon_data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: TRAY_ICON_ID,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: WM_TRAY,
            hIcon: icon,
            ..Default::default()
        };
        copy_wide(&mut icon_data.szTip, &initial_tooltip);

        let _ = Shell_NotifyIconW(NIM_ADD, &icon_data);

        // Context menu: Show / Hide, Compact, Ghost, Settings, Refresh, Exit
        let menu = match CreatePopupMenu() {
            Ok(menu) => menu,
            Err(_) => {
                let _ = DestroyWindow(hwnd);
                return;
            }
        };
        MENU_HANDLE.store(menu.0 as isize, Ordering::Relaxed);

        let items: [(u32, &str); 5] = [
            (1, "Show / Hide RAM View"),
            (2, "Compact Mode"),
            (3, "Toggle Ghost (Click-Through)"),
            (4, "Settings"),
            (5, "Refresh"),
        ];
        for (id, label) in items {
            let buffer = wide(label);
            let _ = AppendMenuW(menu, MF_STRING, id as usize, pcwstr(&buffer));
        }
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let exit_label = wide("Exit");
        let _ = AppendMenuW(menu, MF_STRING, 6usize, pcwstr(&exit_label));

        // Global hotkey (Ctrl+Shift+R by default)
        let (modifiers, vk) = parse_chord(hotkey_chord);
        let _ = RegisterHotKey(
            hwnd,
            HOTKEY_ID,
            HOT_KEY_MODIFIERS(modifiers | MOD_NOREPEAT),
            vk,
        );

        let wake_event = CreateEventW(None, true, false, None).unwrap_or(HANDLE(std::ptr::null_mut()));
        wake_event_flag.store(wake_event.0 as isize, Ordering::Relaxed);
        let mut running = true;

        while running {
            let wait = MsgWaitForMultipleObjectsEx(
                Some(&[wake_event]),
                u32::MAX,
                QS_ALLINPUT,
                MWMO_INPUTAVAILABLE,
            );

            let wait_code = wait.0;

            if wait_code == WAIT_OBJECT_0.0 {
                let _ = ResetEvent(wake_event);

                loop {
                    match command_rx.try_recv() {
                        Ok(ShellCommand::SetTooltip(tooltip)) => {
                            copy_wide(&mut icon_data.szTip, &tooltip);
                            icon_data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
                            let _ = Shell_NotifyIconW(NIM_MODIFY, &icon_data);
                        }
                        Ok(ShellCommand::ShowBalloon(title, text)) => {
                            copy_wide(&mut icon_data.szInfoTitle, &title);
                            copy_wide(&mut icon_data.szInfo, &text);
                            icon_data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_INFO;
                            icon_data.dwInfoFlags = NIIF_INFO;
                            icon_data.Anonymous = NOTIFYICONDATAW_0 { uTimeout: 1500 };
                            let _ = Shell_NotifyIconW(NIM_MODIFY, &icon_data);
                        }
                        Ok(ShellCommand::Shutdown) => {
                            running = false;
                            break;
                        }
                        Err(_) => break,
                    }
                }
            } else if wait_code == WAIT_OBJECT_0.0 + 1 {
                let mut message = windows::Win32::UI::WindowsAndMessaging::MSG::default();
                while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                    if message.message == WM_QUIT {
                        running = false;
                        break;
                    }
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        }

        let _ = UnregisterHotKey(hwnd, HOTKEY_ID);
        let _ = Shell_NotifyIconW(NIM_DELETE, &icon_data);
        let _ = DestroyMenu(menu);
        let _ = DestroyWindow(hwnd);

        wake_event_flag.store(0, Ordering::Relaxed);
        let _ = CloseHandle(wake_event);
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_TRAY => {
            let mouse_message = lparam.0 as u32;
            match mouse_message {
                WM_LBUTTONDBLCLK => send_event(ShellEvent::ToggleShowHide),
                WM_RBUTTONUP => {
                    let mut point = POINT::default();
                    let _ = GetCursorPos(&mut point);
                    let _ = SetForegroundWindow(hwnd);

                    let menu = HMENU(MENU_HANDLE.load(Ordering::Relaxed) as *mut core::ffi::c_void);
                    let command = TrackPopupMenu(
                        menu,
                        TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_LEFTALIGN | TPM_BOTTOMALIGN,
                        point.x,
                        point.y,
                        0,
                        hwnd,
                        None,
                    );
                    let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));

                    match command.0 {
                        1 => send_event(ShellEvent::ToggleShowHide),
                        2 => send_event(ShellEvent::ToggleCompact),
                        3 => send_event(ShellEvent::ToggleGhost),
                        4 => send_event(ShellEvent::OpenSettings),
                        5 => send_event(ShellEvent::Refresh),
                        6 => send_event(ShellEvent::Exit),
                        _ => {}
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_HOTKEY => {
            send_event(ShellEvent::ToggleShowHide);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}
