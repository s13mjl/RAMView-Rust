use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::size_of;
use std::sync::OnceLock;

use windows::core::PCWSTR;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, SelectObject,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DIB_RGB_COLORS,
    DEFAULT_CHARSET, DEFAULT_PITCH, FF_DONTCARE, HBITMAP, HDC, HFONT, HGDIOBJ, OUT_DEFAULT_PRECIS,
    SRCCOPY,
};
use windows::Win32::Graphics::GdiPlus::{
    GdipAddPathArc, GdipClosePathFigure, GdipCreateBitmapFromScan0, GdipCreateFont,
    GdipCreateFontFamilyFromName, GdipCreateFromHDC, GdipCreateImageAttributes, GdipCreatePath,
    GdipCreatePen1, GdipCreateSolidFill, GdipCreateStringFormat, GdipDeleteBrush, GdipDeleteFont,
    GdipDeleteFontFamily, GdipDeleteGraphics, GdipDeletePath, GdipDeletePen,
    GdipDeleteStringFormat, GdipDisposeImage, GdipDisposeImageAttributes, GdipDrawEllipse,
    GdipDrawImageRectRectI, GdipDrawLine, GdipDrawPath, GdipDrawString, GdipFillEllipse,
    GdipFillPath, GdipFillRectangle, GdipMeasureString, GdipRestoreGraphics, GdipSaveGraphics,
    GdipSetClipRect, GdipSetImageAttributesColorMatrix, GdipSetInterpolationMode,
    GdipSetPenLineCap197819, GdipSetSmoothingMode, GdipSetStringFormatAlign,
    GdipSetStringFormatFlags, GdipSetStringFormatHotkeyPrefix, GdipSetStringFormatLineAlign,
    GdipSetStringFormatTrimming, GdipSetTextRenderingHint, GdiplusStartup, GdiplusStartupInput,
    ColorAdjustType, ColorMatrix, ColorMatrixFlags, CombineMode, DashCap, FillMode, GpBitmap,
    GpBrush, GpFont, GpFontFamily, GpGraphics, GpImage, GpImageAttributes, GpPath, GpPen,
    GpSolidFill, GpStringFormat, InterpolationMode, LineCap, RectF, SmoothingMode, StringAlignment,
    StringTrimming, TextRenderingHint, Unit,
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

    pub fn argb(self) -> u32 {
        0xFF00_0000 | ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
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

    fn key(&self) -> u32 {
        let align = match self.halign {
            HAlign::Left => 0,
            HAlign::Center => 1,
            HAlign::Right => 2,
        };
        align
            | ((self.vcenter as u32) << 2)
            | ((self.ellipsis as u32) << 3)
            | ((self.wrap as u32) << 4)
    }
}

fn ensure_gdiplus() {
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| unsafe {
        let mut token = 0usize;
        let input = GdiplusStartupInput {
            GdiplusVersion: 1,
            ..Default::default()
        };
        let status = GdiplusStartup(&mut token, &input, std::ptr::null_mut());
        if status.0 != 0 {
            eprintln!("GDI+ startup failed: {}", status.0);
        }
    });
}

/// Opaque GDI+ font handle plus the GDI font used by the native EDIT control.
#[derive(Clone, Copy)]
pub struct Font {
    pub hfont: HFONT,
    gp: *mut GpFont,
}

/// 32bpp top-down DIB with a memory DC, painted through GDI+ (anti-aliased).
pub struct Surface {
    pub hdc: HDC,
    bitmap: HBITMAP,
    old_bitmap: HGDIOBJ,
    pub width: i32,
    pub height: i32,
    graphics: *mut GpGraphics,
    brushes: RefCell<HashMap<u32, *mut GpSolidFill>>,
    pens: RefCell<HashMap<(u32, u32, u8), *mut GpPen>>,
    formats: RefCell<HashMap<u32, *mut GpStringFormat>>,
}

impl Surface {
    pub fn new(width: i32, height: i32) -> Option<Self> {
        if width <= 0 || height <= 0 {
            return None;
        }
        ensure_gdiplus();

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

            let mut graphics: *mut GpGraphics = std::ptr::null_mut();
            if GdipCreateFromHDC(hdc, &mut graphics).0 != 0 {
                SelectObject(hdc, old_bitmap);
                let _ = DeleteObject(HGDIOBJ(bitmap.0));
                let _ = DeleteDC(hdc);
                return None;
            }

            let _ = GdipSetSmoothingMode(graphics, SmoothingMode(4)); // AntiAlias
            let _ = GdipSetTextRenderingHint(graphics, TextRenderingHint(3)); // AntiAliasGridFit
            let _ = GdipSetInterpolationMode(graphics, InterpolationMode(6)); // HighQualityBicubic
            let _ = GdipSetSmoothingMode(graphics, SmoothingMode(4));

            Some(Self {
                hdc,
                bitmap,
                old_bitmap,
                width,
                height,
                graphics,
                brushes: RefCell::new(HashMap::new()),
                pens: RefCell::new(HashMap::new()),
                formats: RefCell::new(HashMap::new()),
            })
        }
    }

    pub fn graphics(&self) -> *mut GpGraphics {
        self.graphics
    }

    /// Drops per-frame cached brushes/pens; call once before painting a frame.
    pub fn begin_frame(&self) {
        unsafe {
            for (_, brush) in self.brushes.borrow_mut().drain() {
                let _ = GdipDeleteBrush(brush as *mut GpBrush);
            }
            for (_, pen) in self.pens.borrow_mut().drain() {
                let _ = GdipDeletePen(pen);
            }
        }
    }

    fn brush(&self, color: Rgb) -> *mut GpSolidFill {
        let argb = color.argb();
        if let Some(brush) = self.brushes.borrow().get(&argb) {
            return *brush;
        }
        let mut brush: *mut GpSolidFill = std::ptr::null_mut();
        unsafe {
            let _ = GdipCreateSolidFill(argb, &mut brush);
        }
        self.brushes.borrow_mut().insert(argb, brush);
        brush
    }

    fn pen(&self, color: Rgb, width: u32, round_caps: bool) -> *mut GpPen {
        let key = (color.argb(), width, round_caps as u8);
        if let Some(pen) = self.pens.borrow().get(&key) {
            return *pen;
        }
        let mut pen: *mut GpPen = std::ptr::null_mut();
        unsafe {
            let thickness = (width as f32 / 10.0).max(1.0);
            let _ = GdipCreatePen1(color.argb(), thickness, Unit(2), &mut pen);
            if round_caps {
                let _ = GdipSetPenLineCap197819(
                    pen,
                    LineCap(2),
                    LineCap(2),
                    DashCap(2),
                );
            }
        }
        self.pens.borrow_mut().insert(key, pen);
        pen
    }

    fn format(&self, opts: TextOpts) -> *mut GpStringFormat {
        let key = opts.key();
        if let Some(format) = self.formats.borrow().get(&key) {
            return *format;
        }

        let mut format: *mut GpStringFormat = std::ptr::null_mut();
        unsafe {
            let _ = GdipCreateStringFormat(0, 0, &mut format);
            let align = match opts.halign {
                HAlign::Left => 0,
                HAlign::Center => 1,
                HAlign::Right => 2,
            };
            let _ = GdipSetStringFormatAlign(format, StringAlignment(align));
            let _ = GdipSetStringFormatLineAlign(
                format,
                StringAlignment(if opts.vcenter { 1 } else { 0 }),
            );
            if opts.ellipsis {
                let _ = GdipSetStringFormatTrimming(format, StringTrimming(3));
            }
            if !opts.wrap {
                let _ = GdipSetStringFormatFlags(format, 0x1000); // NoWrap
            }
            let _ = GdipSetStringFormatHotkeyPrefix(format, 0);
        }
        self.formats.borrow_mut().insert(key, format);
        format
    }

    pub fn fill(&self, rect: RECT, color: Rgb) {
        let brush = self.brush(color);
        unsafe {
            let _ = GdipFillRectangle(
                self.graphics,
                brush as *mut GpBrush,
                rect.left as f32,
                rect.top as f32,
                (rect.right - rect.left) as f32,
                (rect.bottom - rect.top) as f32,
            );
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
        let radius = radius.max(0) as f32;
        let mut path: *mut GpPath = std::ptr::null_mut();
        unsafe {
            if GdipCreatePath(FillMode(1), &mut path).0 != 0 {
                if let Some(color) = fill {
                    self.fill(rect, color);
                }
                return;
            }

            let x = rect.left as f32;
            let y = rect.top as f32;
            let w = (rect.right - rect.left) as f32;
            let h = (rect.bottom - rect.top) as f32;
            let d = (radius * 2.0).min(w).min(h);

            if d > 0.5 {
                let _ = GdipAddPathArc(path, x, y, d, d, 180.0, 90.0);
                let _ = GdipAddPathArc(path, x + w - d, y, d, d, 270.0, 90.0);
                let _ = GdipAddPathArc(path, x + w - d, y + h - d, d, d, 0.0, 90.0);
                let _ = GdipAddPathArc(path, x, y + h - d, d, d, 90.0, 90.0);
            } else {
                let _ = GdipAddPathArc(path, x, y, w, h, 0.0, 359.99);
            }
            let _ = GdipClosePathFigure(path);

            if let Some(color) = fill {
                let brush = self.brush(color);
                let _ = GdipFillPath(self.graphics, brush as *mut GpBrush, path);
            }
            if let Some(color) = border {
                let pen = self.pen(color, 10, false);
                let _ = GdipDrawPath(self.graphics, pen, path);
            }

            let _ = GdipDeletePath(path);
        }
    }

    pub fn ellipse(&self, rect: RECT, color: Rgb) {
        let brush = self.brush(color);
        unsafe {
            let _ = GdipFillEllipse(
                self.graphics,
                brush as *mut GpBrush,
                rect.left as f32,
                rect.top as f32,
                (rect.right - rect.left) as f32,
                (rect.bottom - rect.top) as f32,
            );
        }
    }

    pub fn ellipse_outline(&self, rect: RECT, color: Rgb, width: i32) {
        let pen = self.pen(color, (width.max(1) * 10) as u32, true);
        unsafe {
            let _ = GdipDrawEllipse(
                self.graphics,
                pen,
                rect.left as f32,
                rect.top as f32,
                (rect.right - rect.left) as f32,
                (rect.bottom - rect.top) as f32,
            );
        }
    }

    pub fn line(&self, x1: i32, y1: i32, x2: i32, y2: i32, color: Rgb, width: i32) {
        let pen = self.pen(color, (width.max(1) * 10) as u32, true);
        unsafe {
            let _ = GdipDrawLine(
                self.graphics,
                pen,
                x1 as f32,
                y1 as f32,
                x2 as f32,
                y2 as f32,
            );
        }
    }

    #[allow(dead_code)]
    pub fn line_f32(&self, x1: f32, y1: f32, x2: f32, y2: f32, color: Rgb, width: f32) {
        let pen = self.pen(color, (width.max(0.1) * 10.0) as u32, true);
        unsafe {
            let _ = GdipDrawLine(self.graphics, pen, x1, y1, x2, y2);
        }
    }

    pub fn text(&self, text: &str, rect: RECT, font: Font, color: Rgb, opts: TextOpts) {
        if text.is_empty() {
            return;
        }
        let wide: Vec<u16> = text.encode_utf16().collect();
        let layout = RectF {
            X: rect.left as f32,
            Y: rect.top as f32,
            Width: (rect.right - rect.left) as f32,
            Height: (rect.bottom - rect.top) as f32,
        };
        let format = self.format(opts);
        let brush = self.brush(color);
        unsafe {
            let _ = GdipDrawString(
                self.graphics,
                PCWSTR::from_raw(wide.as_ptr()),
                wide.len() as i32,
                font.gp,
                &layout,
                format,
                brush as *const GpBrush,
            );
        }
    }

    pub fn text_width(&self, text: &str, font: Font) -> i32 {
        if text.is_empty() {
            return 0;
        }
        let wide: Vec<u16> = text.encode_utf16().collect();
        let layout = RectF {
            X: 0.0,
            Y: 0.0,
            Width: 10000.0,
            Height: 1000.0,
        };
        let format = self.format(TextOpts {
            halign: HAlign::Left,
            vcenter: false,
            ellipsis: false,
            wrap: false,
        });
        let mut bounds = RectF::default();
        unsafe {
            let _ = GdipMeasureString(
                self.graphics,
                PCWSTR::from_raw(wide.as_ptr()),
                wide.len() as i32,
                font.gp,
                &layout,
                format,
                &mut bounds,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
        }
        bounds.Width.ceil() as i32
    }

    pub fn text_height(&self, font: Font) -> i32 {
        let wide: Vec<u16> = "Ag".encode_utf16().collect();
        let layout = RectF {
            X: 0.0,
            Y: 0.0,
            Width: 1000.0,
            Height: 1000.0,
        };
        let format = self.format(TextOpts {
            halign: HAlign::Left,
            vcenter: false,
            ellipsis: false,
            wrap: false,
        });
        let mut bounds = RectF::default();
        unsafe {
            let _ = GdipMeasureString(
                self.graphics,
                PCWSTR::from_raw(wide.as_ptr()),
                wide.len() as i32,
                font.gp,
                &layout,
                format,
                &mut bounds,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
        }
        bounds.Height.ceil() as i32
    }

    pub fn save(&self) -> u32 {
        let mut state = 0u32;
        unsafe {
            let _ = GdipSaveGraphics(self.graphics, &mut state);
        }
        state
    }

    pub fn restore(&self, state: u32) {
        unsafe {
            let _ = GdipRestoreGraphics(self.graphics, state);
        }
    }

    pub fn clip(&self, rect: RECT) {
        unsafe {
            let _ = GdipSetClipRect(
                self.graphics,
                rect.left as f32,
                rect.top as f32,
                (rect.right - rect.left) as f32,
                (rect.bottom - rect.top) as f32,
                CombineMode(1), // Intersect
            );
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
            self.begin_frame();
            for (_, format) in self.formats.borrow_mut().drain() {
                let _ = GdipDeleteStringFormat(format);
            }
            let _ = GdipDeleteGraphics(self.graphics);
            SelectObject(self.hdc, self.old_bitmap);
            let _ = DeleteObject(HGDIOBJ(self.bitmap.0));
            let _ = DeleteDC(self.hdc);
        }
    }
}

pub struct FontCache {
    scale: f32,
    fonts: HashMap<(i32, bool), Font>,
    icon_fonts: HashMap<i32, Font>,
    families: HashMap<&'static str, *mut GpFontFamily>,
}

impl FontCache {
    pub fn new(scale: f32) -> Self {
        ensure_gdiplus();
        Self {
            scale,
            fonts: HashMap::new(),
            icon_fonts: HashMap::new(),
            families: HashMap::new(),
        }
    }

    pub fn set_scale(&mut self, scale: f32) {
        if (self.scale - scale).abs() > f32::EPSILON {
            self.scale = scale;
            self.clear();
        }
    }

    fn family(&mut self, face: &'static str) -> *mut GpFontFamily {
        if let Some(family) = self.families.get(face) {
            return *family;
        }
        let name = native_wide(face);
        let mut family: *mut GpFontFamily = std::ptr::null_mut();
        unsafe {
            let _ = GdipCreateFontFamilyFromName(
                PCWSTR::from_raw(name.as_ptr()),
                std::ptr::null_mut(),
                &mut family,
            );
        }
        self.families.insert(face, family);
        family
    }

    fn clear(&mut self) {
        for (_, font) in self.fonts.drain() {
            unsafe {
                let _ = DeleteObject(HGDIOBJ(font.hfont.0));
                if !font.gp.is_null() {
                    let _ = GdipDeleteFont(font.gp);
                }
            }
        }
        for (_, font) in self.icon_fonts.drain() {
            unsafe {
                let _ = DeleteObject(HGDIOBJ(font.hfont.0));
                if !font.gp.is_null() {
                    let _ = GdipDeleteFont(font.gp);
                }
            }
        }
        for (_, family) in self.families.drain() {
            unsafe {
                if !family.is_null() {
                    let _ = GdipDeleteFontFamily(family);
                }
            }
        }
    }

    fn create(&mut self, face: &'static str, size: f32, bold: bool) -> Font {
        let px = ((size * self.scale).round() as i32).max(8);
        let family = self.family(face);

        let gdi_face = native_wide(face);
        let hfont = unsafe {
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
                PCWSTR::from_raw(gdi_face.as_ptr()),
            )
        };

        let mut gp: *mut GpFont = std::ptr::null_mut();
        unsafe {
            let _ = GdipCreateFont(
                family,
                px as f32,
                if bold { 1 } else { 0 },
                Unit(2),
                &mut gp,
            );
        }

        Font { hfont, gp }
    }

    /// `size` is a WPF-style DIP em size.
    pub fn get(&mut self, size: f32, bold: bool) -> Font {
        let px = ((size * self.scale).round() as i32).max(8);
        if let Some(font) = self.fonts.get(&(px, bold)) {
            return *font;
        }
        let font = self.create("Segoe UI", size, bold);
        self.fonts.insert((px, bold), font);
        font
    }

    /// Monochrome icon glyphs (Segoe MDL2 Assets) for pin/lock/settings.
    pub fn get_icon(&mut self, size: f32) -> Font {
        let px = ((size * self.scale).round() as i32).max(8);
        if let Some(font) = self.icon_fonts.get(&px) {
            return *font;
        }
        let font = self.create("Segoe MDL2 Assets", size, false);
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
    image: *mut GpBitmap,
    width: i32,
    height: i32,
    _pixels: Vec<u8>,
}

impl GdiIcon {
    fn from_png(bytes: &[u8]) -> Option<Self> {
        ensure_gdiplus();
        let image = image::load_from_memory(bytes).ok()?.to_rgba8();
        let (width, height) = image.dimensions();
        if width == 0 || height == 0 {
            return None;
        }

        // Premultiplied BGRA for PixelFormat32bppPARGB.
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        for (index, pixel) in image.pixels().enumerate() {
            let alpha = pixel[3] as u32;
            let base = index * 4;
            pixels[base] = ((pixel[2] as u32 * alpha) / 255) as u8;
            pixels[base + 1] = ((pixel[1] as u32 * alpha) / 255) as u8;
            pixels[base + 2] = ((pixel[0] as u32 * alpha) / 255) as u8;
            pixels[base + 3] = pixel[3];
        }

        let mut bitmap: *mut GpBitmap = std::ptr::null_mut();
        let status = unsafe {
            GdipCreateBitmapFromScan0(
                width as i32,
                height as i32,
                (width * 4) as i32,
                0x000E_200B, // PixelFormat32bppPARGB
                Some(pixels.as_ptr()),
                &mut bitmap,
            )
        };
        if status.0 != 0 {
            return None;
        }

        Some(Self {
            image: bitmap,
            width: width as i32,
            height: height as i32,
            _pixels: pixels,
        })
    }
}

impl Drop for GdiIcon {
    fn drop(&mut self) {
        unsafe {
            let _ = GdipDisposeImage(self.image as *mut GpImage);
        }
    }
}

#[derive(Default)]
pub struct IconCache {
    icons: HashMap<String, Option<GdiIcon>>,
    attributes: *mut GpImageAttributes,
}

impl IconCache {
    fn attributes(&mut self) -> *mut GpImageAttributes {
        if self.attributes.is_null() {
            unsafe {
                let _ = GdipCreateImageAttributes(&mut self.attributes);
            }
        }
        self.attributes
    }

    pub fn draw(
        &mut self,
        surface: &Surface,
        key: &str,
        png: Option<&[u8]>,
        rect: RECT,
        constant_alpha: u8,
    ) -> bool {
        let (image, source_width, source_height) = {
            let entry = self
                .icons
                .entry(key.to_string())
                .or_insert_with(|| png.and_then(GdiIcon::from_png));

            match entry {
                Some(icon) => (icon.image, icon.width, icon.height),
                None => return false,
            }
        };

        let graphics = surface.graphics();
        unsafe {
            if constant_alpha >= 250 {
                let _ = GdipDrawImageRectRectI(
                    graphics,
                    image as *mut GpImage,
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    0,
                    0,
                    source_width,
                    source_height,
                    Unit(2),
                    std::ptr::null(),
                    0,
                    std::ptr::null_mut(),
                );
            } else {
                let attributes = self.attributes();
                if attributes.is_null() {
                    return false;
                }
                let mut matrix = ColorMatrix::default();
                matrix.m[0] = 1.0;
                matrix.m[6] = 1.0;
                matrix.m[12] = 1.0;
                matrix.m[18] = constant_alpha as f32 / 255.0;
                matrix.m[24] = 1.0;
                let _ = GdipSetImageAttributesColorMatrix(
                    attributes,
                    ColorAdjustType(1),
                    true,
                    &matrix,
                    std::ptr::null(),
                    ColorMatrixFlags(0),
                );
                let _ = GdipDrawImageRectRectI(
                    graphics,
                    image as *mut GpImage,
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    0,
                    0,
                    source_width,
                    source_height,
                    Unit(2),
                    attributes,
                    0,
                    std::ptr::null_mut(),
                );
            }
        }

        true
    }
}

impl Drop for IconCache {
    fn drop(&mut self) {
        unsafe {
            if !self.attributes.is_null() {
                let _ = GdipDisposeImageAttributes(self.attributes);
            }
        }
    }
}
