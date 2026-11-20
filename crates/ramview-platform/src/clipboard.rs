use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_UNICODETEXT;

/// Sets CF_UNICODETEXT on the clipboard (WPF Clipboard.SetText equivalent).
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
                let hglobal: HANDLE = HANDLE(handle.0);
                let _ = SetClipboardData(CF_UNICODETEXT.0 as u32, hglobal);
            }
        }

        let _ = CloseClipboard();
    }
}
