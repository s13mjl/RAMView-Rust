use std::process::Command;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowLongW, GetWindowTextW, GetWindowThreadProcessId, SetWindowLongW,
    GWL_EXSTYLE, SW_SHOWNORMAL, WS_EX_TRANSPARENT,
};

use crate::native::wide;

struct FindWindowState {
    title: String,
    pid: u32,
    hwnd: Option<HWND>,
}

unsafe extern "system" fn find_window_proc(hwnd: HWND, lparam: LPARAM) -> windows::Win32::Foundation::BOOL {
    let state = &mut *(lparam.0 as *mut FindWindowState);

    let mut owner_pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut owner_pid));
    if owner_pid != state.pid {
        return true.into();
    }

    let mut buffer = [0u16; 256];
    let len = GetWindowTextW(hwnd, &mut buffer);
    if len > 0 {
        let title = String::from_utf16_lossy(&buffer[..len as usize]);
        if title == state.title {
            state.hwnd = Some(hwnd);
            return false.into();
        }
    }

    true.into()
}

/// Finds this process' top-level window by title (winit does not expose the HWND to eframe apps).
pub fn find_main_window(title: &str) -> Option<HWND> {
    let mut state = FindWindowState {
        title: title.to_string(),
        pid: std::process::id(),
        hwnd: None,
    };

    unsafe {
        let _ = EnumWindows(
            Some(find_window_proc),
            LPARAM(&mut state as *mut FindWindowState as isize),
        );
    }

    state.hwnd
}

/// Applies the Windows dark theme framing (§15).
pub fn apply_dark_mode(hwnd: HWND) {
    let value: i32 = 1;
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &value as *const i32 as *const core::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        );
    }
}

/// Toggles WS_EX_TRANSPARENT for true click-through ghost mode (§15).
pub fn set_click_through(hwnd: HWND, enabled: bool) {
    unsafe {
        let mut ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        let flag = WS_EX_TRANSPARENT.0 as i32;
        if enabled {
            ex_style |= flag;
        } else {
            ex_style &= !flag;
        }
        SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style);
    }
}

/// Applies the dark theme framing to this process' main window.
pub fn apply_dark_mode_to_main_window() {
    if let Some(hwnd) = find_main_window("RAM VIEW") {
        apply_dark_mode(hwnd);
    }
}

/// Toggles click-through on this process' main window.
pub fn set_main_window_click_through(enabled: bool) {
    if let Some(hwnd) = find_main_window("RAM VIEW") {
        set_click_through(hwnd, enabled);
    }
}

/// Opens a URL with the shell (UseShellExecute = true).
pub fn open_url(url: &str) {
    let operation = wide("open");
    let file = wide(url);
    unsafe {
        ShellExecuteW(
            HWND::default(),
            PCWSTR::from_raw(operation.as_ptr()),
            PCWSTR::from_raw(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

/// Opens Explorer with the given file selected (`explorer.exe /select,"path"`).
pub fn open_file_location(path: &str) {
    let _ = Command::new("explorer.exe")
        .arg(format!("/select,\"{path}\""))
        .spawn();
}
