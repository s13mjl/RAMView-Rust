use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::size_of;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, RECT, SIZE};
use windows::Win32::Graphics::Gdi::{
    AlphaBlend, BitBlt, CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject,
    DrawTextW, Ellipse, FillRect, GetStockObject, GetTextExtentPoint32W, IntersectClipRect, LineTo,
    MoveToEx, RestoreDC, RoundRect, SaveDC, SelectObject, SetBkMode, SetDCBrushColor,
    SetDCPenColor, SetTextColor, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION,
    CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DC_BRUSH, DC_PEN, DEFAULT_CHARSET, DEFAULT_PITCH,
    DIB_RGB_COLORS, DRAW_TEXT_FORMAT, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_RIGHT,
    DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, FF_DONTCARE, HBRUSH, HBITMAP, HDC, HFONT, HGDIOBJ,
    NULL_BRUSH, NULL_PEN, OUT_DEFAULT_PRECIS, SRCCOPY, TRANSPARENT,
};

use crate::native_wide;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn colorref(self) -> COLORREF {
        COLORREF(self.r as u32 | ((self.g as u32) << 8) | ((self.b as u32) << 16))
    }

    pub fn mix(self, fg: Rgb, alpha: f32) -> Rgb {
        let alpha = alpha.clamp(0.0, 1.0);
        Rgb {
            r: (self.r as f32 + (fg.r as f32 - self.r as f32) * alpha).round() as u8,
            g: (self.g as f32 + (fg.g as f32 - self.g as f32) * alpha).round() as u8,
            b: (self.b as f32 + (fg.b as f32 - self.b as f32) * alpha).round() as u8,
        }
    }
}

/// Alpha-composites `fg` (with `a` 0..255) over an opaque `bg`.
pub fn over(bg: Rgb, fg: Rgb, a: u8) -> Rgb {
    bg.mix(fg, a as f32 / 255.0)
}

pub const HUD: Rgb = Rgb::new(0x09, 0x0D, 0x14);
pub const WHITE: Rgb = Rgb::new(0xFF, 0xFF, 0xFF);
pub const TEXT_PRIMARY: Rgb = Rgb::new(0xF9, 0xFA, 0xFB);
pub const TEXT_SECONDARY: Rgb = Rgb::new(0xD1, 0xD5, 0xDB);
pub const TEXT_MUTED: Rgb = Rgb::new(0x94, 0xA3, 0xB8);
pub const TEXT_MUTED_2: Rgb = Rgb::new(0x9C, 0xA3, 0xAF);
pub const ACCENT: Rgb = Rgb::new(0x3B, 0x82, 0xF6);
pub const ACCENT_LIGHT: Rgb = Rgb::new(0x60, 0xA5, 0xFA);
pub const ACCENT_CYAN: Rgb = Rgb::new(0x93, 0xC5, 0xFD);
pub const SKY: Rgb = Rgb::new(0x38, 0xBD, 0xF8);
pub const CHIP_BLUE: Rgb = Rgb::new(0x25, 0x63, 0xEB);
pub const DANGER: Rgb = Rgb::new(0xEF, 0x44, 0x44);
pub const AMBER: Rgb = Rgb::new(0xD9, 0x77, 0x06);
pub const AMBER_BORDER: Rgb = Rgb::new(0xF5, 0x9E, 0x0B);
pub const GREEN: Rgb = Rgb::new(0x10, 0xB9, 0x81);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy)]
pub struct TextOpts {
    pub halign: HAlign,
    pub vcenter: bool,
    pub ellipsis: bool,
    pub wrap: bool,
}

impl Default for TextOpts {
    fn default() -> Self {
        Self {
            halign: HAlign::Left,
            vcenter: true,
            ellipsis: false,
            wrap: false,
        }
    }
}

impl TextOpts {
    pub fn left(ellipsis: bool) -> Self {
        Self {
            ellipsis,
            ..Default::default()
        }
    }

    pub fn centered(ellipsis: bool) -> Self {
        Self {
            halign: HAlign::Center,
            ellipsis,
            ..Default::default()
        }
    }

    pub fn right(ellipsis: bool) -> Self {
        Self {
            halign: HAlign::Right,
            ellipsis,
            ..Default::default()
        }
    }

    pub fn wrapped_left() -> Self {
        Self {
            halign: HAlign::Left,
            vcenter: false,
            ellipsis: false,
            wrap: true,
        }
    }
}

/// 32bpp top-down DIB with a memory DC; everything is drawn opaquely into it.
pub struct Surface {
    pub hdc: HDC,
    bitmap: HBITMAP,
    old_bitmap: HGDIOBJ,
    pub width: i32,
    pub height: i32,
}

impl Surface {
    pub fn new(width: i32, height: i32) -> Option<Self> {
        if width <= 0 || height <= 0 {
            return None;
        }

        unsafe {
            let hdc = CreateCompatibleDC(HDC(std::ptr::null_mut()));
            if hdc.is_invalid() {
                return None;
            }

            let mut info = BITMAPINFO::default();
            info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = width;
            info.bmiHeader.biHeight = -height;
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            info.bmiHeader.biCompression = BI_RGB.0;

            let mut bits: *mut c_void = std::ptr::null_mut();
            let bitmap = CreateDIBSection(
                HDC(std::ptr::null_mut()),
                &info,
                DIB_RGB_COLORS,
                &mut bits,
                None,
                0,
            )
            .ok()?;

            let old_bitmap = SelectObject(hdc, HGDIOBJ(bitmap.0));

            Some(Self {
                hdc,
                bitmap,
                old_bitmap,
                width,
                height,
            })
        }
    }

    pub fn fill(&self, rect: RECT, color: Rgb) {
        unsafe {
            let brush = HBRUSH(GetStockObject(DC_BRUSH).0);
            SelectObject(self.hdc, HGDIOBJ(brush.0));
            SetDCBrushColor(self.hdc, color.colorref());
            let _ = FillRect(self.hdc, &rect, brush);
        }
    }

    pub fn clear(&self, color: Rgb) {
        self.fill(
            RECT {
                left: 0,
                top: 0,
                right: self.width,
                bottom: self.height,
            },
            color,
        );
    }

    pub fn round_rect(&self, rect: RECT, radius: i32, fill: Option<Rgb>, border: Option<Rgb>) {
        unsafe {
            match fill {
                Some(color) => {
                    SelectObject(self.hdc, GetStockObject(DC_BRUSH));
                    SetDCBrushColor(self.hdc, color.colorref());
                }
                None => {
                    SelectObject(self.hdc, GetStockObject(NULL_BRUSH));
                }
            }

            match border {
                Some(color) => {
                    SelectObject(self.hdc, GetStockObject(DC_PEN));
                    SetDCPenColor(self.hdc, color.colorref());
                }
                None => {
                    SelectObject(self.hdc, GetStockObject(NULL_PEN));
                }
            }

            let _ = RoundRect(
                self.hdc,
                rect.left,
                rect.top,
                rect.right,
                rect.bottom,
                radius * 2,
                radius * 2,
            );
        }
    }

    pub fn ellipse(&self, rect: RECT, color: Rgb) {
        unsafe {
            SelectObject(self.hdc, GetStockObject(DC_BRUSH));
            SetDCBrushColor(self.hdc, color.colorref());
            SelectObject(self.hdc, GetStockObject(NULL_PEN));
            let _ = Ellipse(self.hdc, rect.left, rect.top, rect.right, rect.bottom);
        }
    }

    pub fn ellipse_outline(&self, rect: RECT, color: Rgb, width: i32) {
        unsafe {
            SelectObject(self.hdc, GetStockObject(NULL_BRUSH));
            let pen = windows::Win32::Graphics::Gdi::CreatePen(
                windows::Win32::Graphics::Gdi::PS_SOLID,
                width,
                color.colorref(),
            );
            let old = SelectObject(self.hdc, HGDIOBJ(pen.0));
            let _ = Ellipse(self.hdc, rect.left, rect.top, rect.right, rect.bottom);
            SelectObject(self.hdc, old);
            let _ = DeleteObject(HGDIOBJ(pen.0));
        }
    }

    pub fn line(&self, x1: i32, y1: i32, x2: i32, y2: i32, color: Rgb, width: i32) {
        unsafe {
            let pen = windows::Win32::Graphics::Gdi::CreatePen(
                windows::Win32::Graphics::Gdi::PS_SOLID,
                width,
                color.colorref(),
            );
            let old = SelectObject(self.hdc, HGDIOBJ(pen.0));
            let _ = MoveToEx(self.hdc, x1, y1, None);
            let _ = LineTo(self.hdc, x2, y2);
            SelectObject(self.hdc, old);
            let _ = DeleteObject(HGDIOBJ(pen.0));
        }
    }

    pub fn text(&self, text: &str, rect: RECT, font: HFONT, color: Rgb, opts: TextOpts) {
        let mut buffer: Vec<u16> = text.encode_utf16().collect();
        if buffer.is_empty() {
            return;
        }

        let mut flags = DT_NOPREFIX.0;
        match opts.halign {
            HAlign::Left => flags |= DT_LEFT.0,
            HAlign::Center => flags |= DT_CENTER.0,
            HAlign::Right => flags |= DT_RIGHT.0,
        }

        if opts.wrap {
            flags |= DT_WORDBREAK.0;
        } else {
            flags |= DT_SINGLELINE.0;
            if opts.vcenter {
                flags |= DT_VCENTER.0;
            }
            if opts.ellipsis {
                flags |= DT_END_ELLIPSIS.0;
            }
        }

        let mut rect = rect;
        unsafe {
            SelectObject(self.hdc, HGDIOBJ(font.0));
            SetBkMode(self.hdc, TRANSPARENT);
            SetTextColor(self.hdc, color.colorref());
            DrawTextW(self.hdc, &mut buffer, &mut rect, DRAW_TEXT_FORMAT(flags));
        }
    }

    pub fn text_width(&self, text: &str, font: HFONT) -> i32 {
        if text.is_empty() {
            return 0;
        }
        let buffer: Vec<u16> = text.encode_utf16().collect();
        let mut size = SIZE::default();
        unsafe {
            SelectObject(self.hdc, HGDIOBJ(font.0));
            let _ = GetTextExtentPoint32W(self.hdc, &buffer, &mut size);
        }
        size.cx
    }

    pub fn text_height(&self, font: HFONT) -> i32 {
        let buffer: Vec<u16> = "Ag".encode_utf16().collect();
        let mut size = SIZE::default();
        unsafe {
            SelectObject(self.hdc, HGDIOBJ(font.0));
            let _ = GetTextExtentPoint32W(self.hdc, &buffer, &mut size);
        }
        size.cy
    }

    pub fn save(&self) -> i32 {
        unsafe { SaveDC(self.hdc) }
    }

    pub fn restore(&self, saved: i32) {
        unsafe {
            let _ = RestoreDC(self.hdc, saved);
        }
    }

    pub fn clip(&self, rect: RECT) {
        unsafe {
            IntersectClipRect(self.hdc, rect.left, rect.top, rect.right, rect.bottom);
        }
    }

    pub fn blit_to(&self, destination: HDC) {
        unsafe {
            let _ = BitBlt(
                destination,
                0,
                0,
                self.width,
                self.height,
                self.hdc,
                0,
                0,
                SRCCOPY,
            );
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.hdc, self.old_bitmap);
            let _ = DeleteObject(HGDIOBJ(self.bitmap.0));
            let _ = DeleteDC(self.hdc);
        }
    }
}

pub struct FontCache {
    scale: f32,
    fonts: HashMap<(i32, bool), HFONT>,
    icon_fonts: HashMap<i32, HFONT>,
}

impl FontCache {
    pub fn new(scale: f32) -> Self {
        Self {
            scale,
            fonts: HashMap::new(),
            icon_fonts: HashMap::new(),
        }
    }

    pub fn set_scale(&mut self, scale: f32) {
        if (self.scale - scale).abs() > f32::EPSILON {
            self.scale = scale;
            self.clear();
        }
    }

    fn clear(&mut self) {
        for (_, font) in self.fonts.drain() {
            unsafe {
                let _ = DeleteObject(HGDIOBJ(font.0));
            }
        }
        for (_, font) in self.icon_fonts.drain() {
            unsafe {
                let _ = DeleteObject(HGDIOBJ(font.0));
            }
        }
    }

    /// `size` is a WPF-style DIP em size.
    pub fn get(&mut self, size: f32, bold: bool) -> HFONT {
        let px = ((size * self.scale).round() as i32).max(8);
        if let Some(font) = self.fonts.get(&(px, bold)) {
            return *font;
        }

        let face = native_wide("Segoe UI");
        let font = unsafe {
            CreateFontW(
                -px,
                0,
                0,
                0,
                if bold { 700 } else { 400 },
                0,
                0,
                0,
                DEFAULT_CHARSET.0 as u32,
                OUT_DEFAULT_PRECIS.0 as u32,
                CLIP_DEFAULT_PRECIS.0 as u32,
                CLEARTYPE_QUALITY.0 as u32,
                (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
                PCWSTR::from_raw(face.as_ptr()),
            )
        };

        self.fonts.insert((px, bold), font);
        font
    }

    /// Monochrome icon glyphs (Segoe MDL2 Assets) for pin/lock/settings.
    pub fn get_icon(&mut self, size: f32) -> HFONT {
        let px = ((size * self.scale).round() as i32).max(8);
        if let Some(font) = self.icon_fonts.get(&px) {
            return *font;
        }

        let face = native_wide("Segoe MDL2 Assets");
        let font = unsafe {
            CreateFontW(
                -px,
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                DEFAULT_CHARSET.0 as u32,
                OUT_DEFAULT_PRECIS.0 as u32,
                CLIP_DEFAULT_PRECIS.0 as u32,
                CLEARTYPE_QUALITY.0 as u32,
                (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
                PCWSTR::from_raw(face.as_ptr()),
            )
        };

        self.icon_fonts.insert(px, font);
        font
    }
}

impl Drop for FontCache {
    fn drop(&mut self) {
        self.clear();
    }
}

struct GdiIcon {
    dc: HDC,
    bitmap: HBITMAP,
    old_bitmap: HGDIOBJ,
    width: i32,
    height: i32,
}

impl GdiIcon {
    fn from_png(bytes: &[u8]) -> Option<Self> {
        let image = image::load_from_memory(bytes).ok()?.to_rgba8();
        let (width, height) = image.dimensions();
        if width == 0 || height == 0 {
            return None;
        }

        unsafe {
            let dc = CreateCompatibleDC(HDC(std::ptr::null_mut()));
            if dc.is_invalid() {
                return None;
            }

            let mut info = BITMAPINFO::default();
            info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = width as i32;
            info.bmiHeader.biHeight = -(height as i32);
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            info.bmiHeader.biCompression = BI_RGB.0;

            let mut bits: *mut c_void = std::ptr::null_mut();
            let bitmap = CreateDIBSection(
                HDC(std::ptr::null_mut()),
                &info,
                DIB_RGB_COLORS,
                &mut bits,
                None,
                0,
            )
            .ok()?;
            if bits.is_null() {
                return None;
            }

            // Premultiplied BGRA for AlphaBlend(AC_SRC_ALPHA).
            let pixels = bits as *mut u8;
            for (index, pixel) in image.pixels().enumerate() {
                let alpha = pixel[3] as u32;
                let base = index * 4;
                *pixels.add(base) = ((pixel[2] as u32 * alpha) / 255) as u8;
                *pixels.add(base + 1) = ((pixel[1] as u32 * alpha) / 255) as u8;
                *pixels.add(base + 2) = ((pixel[0] as u32 * alpha) / 255) as u8;
                *pixels.add(base + 3) = pixel[3];
            }

            let old_bitmap = SelectObject(dc, HGDIOBJ(bitmap.0));

            Some(Self {
                dc,
                bitmap,
                old_bitmap,
                width: width as i32,
                height: height as i32,
            })
        }
    }

    fn draw(&self, destination: HDC, rect: RECT, constant_alpha: u8) {
        let blend = BLENDFUNCTION {
            BlendOp: 0,
            BlendFlags: 0,
            SourceConstantAlpha: constant_alpha,
            AlphaFormat: 1,
        };
        unsafe {
            let _ = AlphaBlend(
                destination,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                self.dc,
                0,
                0,
                self.width,
                self.height,
                blend,
            );
        }
    }
}

impl Drop for GdiIcon {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old_bitmap);
            let _ = DeleteObject(HGDIOBJ(self.bitmap.0));
            let _ = DeleteDC(self.dc);
        }
    }
}

#[derive(Default)]
pub struct IconCache {
    icons: HashMap<String, Option<GdiIcon>>,
}

impl IconCache {
    pub fn draw(
        &mut self,
        destination: HDC,
        key: &str,
        png: Option<&[u8]>,
        rect: RECT,
        constant_alpha: u8,
    ) -> bool {
        let entry = self
            .icons
            .entry(key.to_string())
            .or_insert_with(|| png.and_then(GdiIcon::from_png));

        match entry {
            Some(icon) => {
                icon.draw(destination, rect, constant_alpha);
                true
            }
            None => false,
        }
    }
}
