use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindow, GetWindowThreadProcessId, IsWindowVisible, SendMessageTimeoutW,
    GW_OWNER, SMTO_ABORTIFHUNG, WM_NULL,
};

struct WindowSearch {
    pid: u32,
    main_window: Option<HWND>,
}

unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> windows::Win32::Foundation::BOOL {
    let search = &mut *(lparam.0 as *mut WindowSearch);

    let mut owner_pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut owner_pid));
    if owner_pid != search.pid {
        return true.into();
    }

    // .NET's MainWindowHandle: a visible top-level window with no owner.
    if !IsWindowVisible(hwnd).as_bool() {
        return true.into();
    }
    if let Ok(owner) = GetWindow(hwnd, GW_OWNER) {
        if !owner.is_invalid() {
            return true.into();
        }
    }

    search.main_window = Some(hwnd);
    false.into()
}

/// Mirrors `Process.Responding`: true when the process has no main window,
/// or when its main window answers WM_NULL within 5 seconds.
pub fn is_process_responding(pid: u32) -> bool {
    let mut search = WindowSearch {
        pid,
        main_window: None,
    };

    unsafe {
        let _ = EnumWindows(
            Some(enum_windows_proc),
            LPARAM(&mut search as *mut WindowSearch as isize),
        );
    }

    let hwnd = match search.main_window {
        Some(hwnd) => hwnd,
        None => return true,
    };

    let result = unsafe {
        SendMessageTimeoutW(
            hwnd,
            WM_NULL,
            WPARAM(0),
            LPARAM(0),
            SMTO_ABORTIFHUNG,
            5000,
            None,
        )
    };

    result != LRESULT(0)
}
