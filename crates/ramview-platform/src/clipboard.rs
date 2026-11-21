use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_UNICODETEXT;

/// Reads CF_UNICODETEXT from the clipboard.
pub fn get_text(hwnd: HWND) -> Option<String> {
    unsafe {
        if OpenClipboard(hwnd).is_err() {
            return None;
        }

        let handle = GetClipboardData(CF_UNICODETEXT.0 as u32).ok()?;
        if handle.is_invalid() {
            let _ = CloseClipboard();
            return None;
        }

        let pointer = GlobalLock(HGLOBAL(handle.0)) as *const u16;
        let mut text = None;
        if !pointer.is_null() {
            let mut length = 0usize;
            while *pointer.add(length) != 0 {
                length += 1;
            }
            let units = std::slice::from_raw_parts(pointer, length);
            text = Some(String::from_utf16_lossy(units));
            let _ = GlobalUnlock(HGLOBAL(handle.0));
        }

        let _ = CloseClipboard();
        text
    }
}

pub fn set_text(hwnd: HWND, text: &str) {
    unsafe {
        if OpenClipboard(hwnd).is_err() {
            return;
        }
        let _ = EmptyClipboard();

        let units: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes = units.len() * 2;

        if let Ok(handle) = GlobalAlloc(GMEM_MOVEABLE, bytes) {
            let pointer = GlobalLock(handle) as *mut u16;
            if !pointer.is_null() {
                std::ptr::copy_nonoverlapping(units.as_ptr(), pointer, units.len());
                let _ = GlobalUnlock(handle);
                let _ = SetClipboardData(CF_UNICODETEXT.0 as u32, HANDLE(handle.0));
            }
        }

        let _ = CloseClipboard();
    }
}
