use std::collections::HashMap;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

use crate::native::{pcwstr, wide};

/// Extracts and caches application icons with dual code paths (§24):
/// 1) Win32 shell icon extraction for classic executables.
/// 2) Package asset resolution for packaged store apps.
pub struct IconExtractor {
    cache: Mutex<HashMap<String, Option<Vec<u8>>>>,
}

impl Default for IconExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl IconExtractor {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn get_icon_png(&self, executable_path: Option<&str>, process_name: &str) -> Option<Vec<u8>> {
        let cache_key = match executable_path {
            Some(path) if !path.is_empty() => path.to_lowercase(),
            _ => process_name.to_lowercase(),
        };

        if let Ok(cache) = self.cache.lock() {
            if let Some(cached) = cache.get(&cache_key) {
                return cached.clone();
            }
        }

        let extracted = self.extract_icon(executable_path, process_name);

        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(cache_key, extracted.clone());
        }

        extracted
    }

    fn extract_icon(&self, path: Option<&str>, _process_name: &str) -> Option<Vec<u8>> {
        let path = path?;

        // Path 1: Classic Win32 EXE extraction via the shell
        if !path.is_empty() && Path::new(path).exists() {
            if let Some(bytes) = shell_icon_to_png(path) {
                return Some(bytes);
            }
        }

        // Path 2: Packaged / Modern app fallback resolution
        if !path.is_empty() && path.to_lowercase().contains("windowsapps") {
            if let Some(bytes) = try_extract_from_appx(path) {
                return Some(bytes);
            }
        }

        None
    }
}

fn shell_icon_to_png(path: &str) -> Option<Vec<u8>> {
    let wide_path = wide(path);
    let mut file_info = SHFILEINFOW::default();

    let result = unsafe {
        SHGetFileInfoW(
            pcwstr(&wide_path),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut file_info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        )
    };

    if result == 0 || file_info.hIcon.is_invalid() {
        return None;
    }

    let png = hicon_to_png(file_info.hIcon);
    unsafe {
        let _ = DestroyIcon(file_info.hIcon);
    }
    png
}

fn try_extract_from_appx(exe_path: &str) -> Option<Vec<u8>> {
    let dir = Path::new(exe_path).parent()?;
    let assets_dir = dir.join("Assets");
    if !assets_dir.is_dir() {
        return None;
    }

    let mut candidates = find_files(&assets_dir, "*targetsize-32*");
    if candidates.is_empty() {
        candidates = find_files(&assets_dir, "*Square44x44Logo*");
    }

    let first = candidates.first()?;
    std::fs::read(first).ok()
}

/// Minimal wildcard directory scan (* and ?), sorted for deterministic selection.
fn find_files(dir: &Path, pattern: &str) -> Vec<PathBuf> {
    let mut matches: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if wildcard_match(pattern, name) {
                matches.push(path);
            }
        }
    }

    matches.sort();
    matches
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let text: Vec<char> = text.to_lowercase().chars().collect();

    let mut pattern_index = 0usize;
    let mut text_index = 0usize;
    let mut star_index: Option<usize> = None;
    let mut star_text_index = 0usize;

    while text_index < text.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == '?' || pattern[pattern_index] == text[text_index])
        {
            pattern_index += 1;
            text_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == '*' {
            star_index = Some(pattern_index);
            pattern_index += 1;
            star_text_index = text_index;
        } else if let Some(star) = star_index {
            pattern_index = star + 1;
            star_text_index += 1;
            text_index = star_text_index;
        } else {
            return false;
        }
    }

    while pattern_index < pattern.len() && pattern[pattern_index] == '*' {
        pattern_index += 1;
    }

    pattern_index == pattern.len()
}

fn hicon_to_png(hicon: HICON) -> Option<Vec<u8>> {
    unsafe {
        let mut info = ICONINFO::default();
        if GetIconInfo(hicon, &mut info).is_err() {
            return None;
        }

        let result = if info.hbmColor.is_invalid() {
            None
        } else {
            bitmap_to_png(info.hbmColor, info.hbmMask)
        };

        if !info.hbmColor.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
        }
        if !info.hbmMask.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
        }

        result
    }
}

unsafe fn bitmap_to_png(color: HBITMAP, mask: HBITMAP) -> Option<Vec<u8>> {
    let mut bitmap = BITMAP::default();
    let got = GetObjectW(
        HGDIOBJ(color.0),
        std::mem::size_of::<BITMAP>() as i32,
        Some(&mut bitmap as *mut BITMAP as *mut c_void),
    );
    if got == 0 || bitmap.bmWidth <= 0 || bitmap.bmHeight <= 0 {
        return None;
    }

    let width = bitmap.bmWidth as u32;
    let height = bitmap.bmHeight as u32;
    let pixel_count = (width * height) as usize;

    let mut dib = read_dib(color, width, height, 32)?;
    if dib.len() < pixel_count * 4 {
        return None;
    }

    // Convert BGRA -> RGBA
    let mut rgba = vec![0u8; pixel_count * 4];
    let mut all_alpha_zero = true;
    for i in 0..pixel_count {
        let b = dib[i * 4];
        let g = dib[i * 4 + 1];
        let r = dib[i * 4 + 2];
        let a = dib[i * 4 + 3];
        rgba[i * 4] = r;
        rgba[i * 4 + 1] = g;
        rgba[i * 4 + 2] = b;
        rgba[i * 4 + 3] = a;
        if a != 0 {
            all_alpha_zero = false;
        }
    }

    // Icons without an alpha channel carry transparency in the AND mask.
    if all_alpha_zero && !mask.is_invalid() {
        if let Some(mask_bits) = read_dib(mask, width, height, 1) {
            let stride = ((width + 31) / 32) * 4;
            for y in 0..height as usize {
                for x in 0..width as usize {
                    let byte_index = y * stride as usize + x / 8;
                    let bit = (mask_bits[byte_index] >> (7 - (x % 8))) & 1;
                    let alpha = if bit == 0 { 255 } else { 0 };
                    rgba[(y * width as usize + x) * 4 + 3] = alpha;
                }
            }
        }
    }

    dib.clear();

    let image = image::RgbaImage::from_raw(width, height, rgba)?;
    let mut png_bytes: Vec<u8> = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut png_bytes);
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut cursor, image::ImageFormat::Png)
        .ok()?;

    Some(png_bytes)
}

unsafe fn read_dib(bitmap: HBITMAP, width: u32, height: u32, bit_count: u16) -> Option<Vec<u8>> {
    // GetDIBits requires the bitmap not to be selected into a DC.
    let hdc = CreateCompatibleDC(HDC(std::ptr::null_mut()));
    if hdc.is_invalid() {
        return None;
    }

    let mut bitmap_info = BITMAPINFO::default();
    bitmap_info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bitmap_info.bmiHeader.biWidth = width as i32;
    bitmap_info.bmiHeader.biHeight = -(height as i32);
    bitmap_info.bmiHeader.biPlanes = 1;
    bitmap_info.bmiHeader.biBitCount = bit_count;
    bitmap_info.bmiHeader.biCompression = BI_RGB.0;

    let byte_count = if bit_count == 32 {
        (width * height * 4) as usize
    } else {
        let stride = ((width + 31) / 32) * 4;
        (stride * height) as usize
    };

    let mut buffer = vec![0u8; byte_count];
    let lines = GetDIBits(
        hdc,
        bitmap,
        0,
        height,
        Some(buffer.as_mut_ptr() as *mut c_void),
        &mut bitmap_info,
        DIB_RGB_COLORS,
    );

    let _ = DeleteDC(hdc);

    if lines == 0 {
        return None;
    }

    Some(buffer)
}
