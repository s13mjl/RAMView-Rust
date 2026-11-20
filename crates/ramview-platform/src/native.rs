use windows::core::PCWSTR;

/// Encodes a Rust string as a NUL-terminated UTF-16 buffer.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Borrows a NUL-terminated UTF-16 buffer as a PCWSTR.
pub fn pcwstr(buf: &[u16]) -> PCWSTR {
    PCWSTR::from_raw(buf.as_ptr())
}

/// Decodes a fixed-size UTF-16 buffer up to its first NUL.
pub fn from_wide(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

/// Truncates like C# `value.Substring(0, max)` in UTF-16 code units.
pub fn truncate_utf16(value: &str, max: usize) -> String {
    value.encode_utf16().take(max).map(|c| {
        char::from_u32(c as u32).unwrap_or('\u{fffd}')
    }).collect()
}

/// Copies a string into a fixed-size NUL-terminated UTF-16 destination buffer.
pub fn copy_wide(dst: &mut [u16], value: &str) {
    for slot in dst.iter_mut() {
        *slot = 0;
    }
    for (i, unit) in value.encode_utf16().take(dst.len().saturating_sub(1)).enumerate() {
        dst[i] = unit;
    }
}
