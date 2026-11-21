use ramview_core::algorithms::TreemapRectangle;
use ramview_core::models::{
    ProcessMemoryInfo, SystemMemoryVisualState, APP_MODE_COMPACT_STRIP,
};
use windows::Win32::Foundation::{POINT, RECT};

use crate::app::RamViewApp;
use crate::hit::{HitId, HitRegion};
use crate::layout;
use crate::render::{
    over, FontCache, IconCache, Rgb, Surface, TextOpts, ACCENT, ACCENT_CYAN, ACCENT_LIGHT,
    AMBER_BORDER, CHIP_BLUE, DANGER, GREEN, HUD, SKY, TEXT_MUTED, TEXT_MUTED_2, TEXT_PRIMARY,
    TEXT_SECONDARY, WHITE,
};
use crate::treemap::{key_for, AnimatedTreemap};
use crate::viewmodel::MainViewModel;

pub struct PaintCtx<'a> {
    pub surface: &'a Surface,
    pub scale: f32,
    pub fonts: &'a mut FontCache,
    pub icons: &'a mut IconCache,
    pub hits: &'a mut Vec<HitRegion>,
    pub vm: &'a MainViewModel,
    pub treemap: &'a AnimatedTreemap,
    pub hover: Option<HitId>,
    pub pressed: Option<HitId>,
    pub strip_scroll: i32,
    pub tooltip: Option<HitId>,
    pub cursor: POINT,
    pub edit_focus: bool,
}

pub fn scale_px(scale: f32, dip: f32) -> i32 {
    (dip * scale).round() as i32
}

pub fn paint(app: &mut RamViewApp, surface: &Surface) -> Option<RECT> {
    let tooltip = if app.tooltip_visible {
        app.hover.clone()
    } else {
        None
    };

    let mut hits: Vec<HitRegion> = Vec::new();
    let mut ctx = PaintCtx {
        surface,
        scale: app.scale,
        fonts: &mut app.fonts,
        icons: &mut app.icons,
        hits: &mut hits,
        vm: &app.vm,
        treemap: &app.treemap,
        hover: app.hover.clone(),
        pressed: app.pressed.clone(),
        strip_scroll: app.strip_scroll,
        tooltip,
        cursor: app.cursor,
        edit_focus: app.edit_focus,
    };

    let edit_rect = paint_impl(&mut ctx);
    app.hits = hits;
    edit_rect
}

fn paint_impl(ctx: &mut PaintCtx) -> Option<RECT> {
    ctx.surface.begin_frame();
    ctx.surface.clear(HUD);

    let compact = ctx.vm.app_mode == APP_MODE_COMPACT_STRIP;
    let layout = layout::compute(
        ctx.surface.width,
        ctx.surface.height,
        ctx.scale,
        compact,
        ctx.vm.click_through,
    );

    let edit_rect = paint_header(ctx, &layout);
    if let Some(banner) = layout.banner {
        paint_banner(ctx, banner);
    }
    paint_center(ctx, layout.center, compact);
    if let Some(bottom) = layout.bottom {
        paint_bottom(ctx, bottom);
    }
    let bounds = RECT {
        left: 0,
        top: 0,
        right: ctx.surface.width,
        bottom: ctx.surface.height,
    };
    paint_tooltip(ctx, bounds);

    edit_rect
}

fn paint_header(ctx: &mut PaintCtx, layout: &layout::Layout) -> Option<RECT> {
    let PaintCtx {
        surface,
        scale,
        fonts,
        hits,
        hover,
        pressed,
        vm,
        edit_focus,
        ..
    } = ctx;
    let scale = *scale;

    let rect = layout.header;
    let center_y = (rect.top + rect.bottom) / 2;
    let s = |dip: f32| scale_px(scale, dip);

    // Pulse dot + glow
    let dot = s(9.0);
    let state_color = match vm.visual_state {
        SystemMemoryVisualState::Critical => DANGER,
        SystemMemoryVisualState::High => AMBER_BORDER,
        SystemMemoryVisualState::Normal => GREEN,
        SystemMemoryVisualState::Unknown => Rgb::new(0x9C, 0xA3, 0xAF),
    };
    surface.ellipse(
        RECT {
            left: rect.left - s(3.0),
            top: center_y - dot / 2 - s(3.0),
            right: rect.left + dot + s(3.0),
            bottom: center_y + dot / 2 + s(3.0),
        },
        over(HUD, GREEN, 0x22),
    );
    surface.ellipse(
        RECT {
            left: rect.left,
            top: center_y - dot / 2,
            right: rect.left + dot,
            bottom: center_y + dot / 2,
        },
        state_color,
    );

    // Title
    let title_font = fonts.get(14.0, true);
    let title = "RAM VIEW";
    surface.text(
        title,
        RECT {
            left: rect.left + s(17.0),
            top: rect.top,
            right: rect.left + s(200.0),
            bottom: rect.bottom,
        },
        title_font,
        TEXT_PRIMARY,
        TextOpts::left(false),
    );
    let mut x = rect.left + s(17.0) + surface.text_width(title, title_font) + s(12.0);

    // RAM counter card
    let summary = vm
        .snapshot
        .as_ref()
        .map(|snapshot| snapshot.summary_text())
        .unwrap_or_else(|| "Scanning RAM...".to_string());
    let usage = vm
        .snapshot
        .as_ref()
        .map(|snapshot| format!("• {:.1}% Used", snapshot.system_memory_usage_percentage()))
        .unwrap_or_default();

    let summary_font = fonts.get(13.0, true);
    let usage_font = fonts.get(12.0, false);
    let summary_width = surface.text_width(&summary, summary_font);
    let usage_width = if usage.is_empty() {
        0
    } else {
        surface.text_width(&usage, usage_font) + s(6.0)
    };
    let card_height = s(32.0);
    let card = RECT {
        left: x,
        top: center_y - card_height / 2,
        right: x + summary_width + usage_width + s(18.0),
        bottom: center_y + card_height / 2,
    };
    surface.round_rect(
        card,
        s(8.0),
        Some(over(HUD, WHITE, 0x1C)),
        Some(over(HUD, WHITE, 0x30)),
    );

    let line_top = card.top + s(4.0);
    let line_bottom = line_top + s(18.0);
    surface.text(
        &summary,
        RECT {
            left: card.left + s(9.0),
            top: line_top,
            right: card.right - s(9.0),
            bottom: line_bottom,
        },
        summary_font,
        ACCENT_LIGHT,
        TextOpts::left(true),
    );
    if !usage.is_empty() {
        surface.text(
            &usage,
            RECT {
                left: card.left + s(9.0) + summary_width + s(6.0),
                top: line_top,
                right: card.right - s(9.0),
                bottom: line_bottom,
            },
            usage_font,
            TEXT_MUTED,
            TextOpts::left(false),
        );
    }

    let bar = RECT {
        left: card.left + s(9.0),
        top: card.bottom - s(7.0),
        right: card.right - s(9.0),
        bottom: card.bottom - s(4.0),
    };
    surface.fill(bar, over(HUD, WHITE, 0x25));
    let percentage = vm
        .snapshot
        .as_ref()
        .map(|snapshot| snapshot.system_memory_usage_percentage())
        .unwrap_or(0.0)
        .clamp(0.0, 100.0);
    if percentage > 0.0 {
        let width = ((bar.right - bar.left) as f64 * percentage / 100.0) as i32;
        if width > 0 {
            surface.fill(
                RECT {
                    right: bar.left + width,
                    ..bar
                },
                ACCENT,
            );
        }
    }

    x = card.right;

    // Window actions: compact/overlay, settings, coffee, minimize, close
    let actions = [
        (HitId::ToggleMode, "Toggle Compact Strip / Overlay Mode"),
        (HitId::Settings, "Settings & Preferences"),
        (HitId::Coffee, "Buy Me a Coffee / Support Developer"),
        (HitId::Minimize, "Minimize to System Tray"),
        (HitId::CloseWindow, "Close (Minimizes to Tray)"),
    ];
    let total_width = s(5.0 * 28.0 + 4.0 * 4.0);
    let buttons_left = rect.right - total_width;
    let button_top = center_y - s(14.0);

    for (index, (id, _)) in actions.iter().enumerate() {
        let button = RECT {
            left: buttons_left + s(index as f32 * 32.0),
            top: button_top,
            right: buttons_left + s(index as f32 * 32.0) + s(28.0),
            bottom: button_top + s(28.0),
        };
        let hovered = hover.as_ref() == Some(id);
        let is_pressed = pressed.as_ref() == Some(id);
        paint_glass_button(surface, scale, button, hovered, is_pressed);

        match index {
            0 => paint_compact_icon(surface, scale, button),
            1 => paint_glyph(
                surface,
                fonts,
                scale,
                button,
                "\u{E713}",
                12.0,
                TEXT_SECONDARY,
            ),
            2 => paint_coffee_icon(surface, scale, button),
            3 => paint_minimize_icon(surface, scale, button),
            _ => paint_close_icon(surface, scale, button),
        }

        hits.push(HitRegion {
            rect: button,
            id: id.clone(),
        });
    }

    // Live search box (§13): painted here, the native EDIT sits inside it.
    let search_right = buttons_left - s(14.0);
    let search_left = (search_right - s(220.0)).max(x + s(14.0));
    let mut edit_rect = None;

    if search_right - search_left > s(40.0) {
        let box_rect = RECT {
            left: search_left,
            top: center_y - s(14.0),
            right: search_right,
            bottom: center_y + s(14.0),
        };
        let background = over(HUD, WHITE, if *edit_focus { 0x28 } else { 0x20 });
        let border = if *edit_focus {
            ACCENT
        } else {
            over(HUD, WHITE, 0x38)
        };
        surface.round_rect(box_rect, s(6.0), Some(background), Some(border));

        paint_search_icon(surface, scale, box_rect.left + s(10.0), center_y);

        edit_rect = Some(RECT {
            left: box_rect.left + s(28.0),
            top: box_rect.top + s(1.0),
            right: box_rect.right - s(8.0),
            bottom: box_rect.bottom - s(1.0),
        });
    }

    edit_rect
}

fn paint_glass_button(surface: &Surface, scale: f32, rect: RECT, hovered: bool, pressed: bool) {
    let fill = if pressed {
        over(HUD, WHITE, 0x50)
    } else if hovered {
        over(HUD, WHITE, 0x38)
    } else {
        over(HUD, WHITE, 0x1F)
    };
    let border = if hovered {
        over(HUD, WHITE, 0x70)
    } else {
        over(HUD, WHITE, 0x30)
    };
    surface.round_rect(rect, scale_px(scale, 6.0), Some(fill), Some(border));
}

fn button_center(rect: RECT) -> (i32, i32) {
    ((rect.left + rect.right) / 2, (rect.top + rect.bottom) / 2)
}

fn paint_compact_icon(surface: &Surface, scale: f32, rect: RECT) {
    let (cx, cy) = button_center(rect);
    let w = scale_px(scale, 11.0);
    let h = scale_px(scale, 10.0);
    let icon = RECT {
        left: cx - w / 2,
        top: cy - h / 2,
        right: cx - w / 2 + w,
        bottom: cy - h / 2 + h,
    };
    surface.round_rect(icon, 1, None, Some(TEXT_SECONDARY));
    surface.fill(
        RECT {
            left: icon.left + 1,
            top: icon.top + 1,
            right: icon.right - 1,
            bottom: cy,
        },
        TEXT_SECONDARY,
    );
}

fn paint_coffee_icon(surface: &Surface, scale: f32, rect: RECT) {
    let (cx, cy) = button_center(rect);
    let w = scale_px(scale, 9.0);
    let h = scale_px(scale, 8.0);
    let cup = RECT {
        left: cx - w / 2 - scale_px(scale, 1.0),
        top: cy - h / 2 + scale_px(scale, 1.0),
        right: cx - w / 2 + w,
        bottom: cy + h / 2 + scale_px(scale, 1.0),
    };
    surface.round_rect(cup, scale_px(scale, 2.0), Some(TEXT_SECONDARY), None);
    let handle = RECT {
        left: cup.right - scale_px(scale, 1.0),
        top: cup.top + scale_px(scale, 1.0),
        right: cup.right + scale_px(scale, 4.0),
        bottom: cup.bottom - scale_px(scale, 2.0),
    };
    surface.ellipse_outline(handle, TEXT_SECONDARY, scale_px(scale, 1.0).max(1));
    surface.line(
        cx,
        cup.top - scale_px(scale, 3.0),
        cx,
        cup.top - scale_px(scale, 1.0),
        TEXT_SECONDARY,
        scale_px(scale, 1.0).max(1),
    );
}

fn paint_minimize_icon(surface: &Surface, scale: f32, rect: RECT) {
    let (cx, cy) = button_center(rect);
    let half = scale_px(scale, 5.0);
    surface.line(cx - half, cy, cx + half, cy, TEXT_SECONDARY, 1);
}

fn paint_close_icon(surface: &Surface, scale: f32, rect: RECT) {
    let (cx, cy) = button_center(rect);
    let half = scale_px(scale, 4.0);
    surface.line(cx - half, cy - half, cx + half, cy + half, TEXT_SECONDARY, 1);
    surface.line(cx - half, cy + half, cx + half, cy - half, TEXT_SECONDARY, 1);
}

fn paint_search_icon(surface: &Surface, scale: f32, left: i32, center_y: i32) {
    let radius = scale_px(scale, 4.0);
    surface.ellipse_outline(
        RECT {
            left,
            top: center_y - radius,
            right: left + radius * 2,
            bottom: center_y + radius,
        },
        Rgb::new(0x64, 0x74, 0x8B),
        scale_px(scale, 1.0).max(1),
    );
    surface.line(
        left + radius * 2 - scale_px(scale, 1.0),
        center_y + radius - scale_px(scale, 1.0),
        left + radius * 2 + scale_px(scale, 2.0),
        center_y + radius + scale_px(scale, 2.0),
        Rgb::new(0x64, 0x74, 0x8B),
        scale_px(scale, 1.0).max(1),
    );
}

fn paint_glyph(
    surface: &Surface,
    fonts: &mut FontCache,
    _scale: f32,
    rect: RECT,
    glyph: &str,
    size: f32,
    color: Rgb,
) {
    let font = fonts.get_icon(size);
    surface.text(glyph, rect, font, color, TextOpts::centered(false));
}

fn paint_banner(ctx: &mut PaintCtx, rect: RECT) {
    let scale = ctx.scale;
    let s = |dip: f32| scale_px(scale, dip);
    ctx.surface.round_rect(
        rect,
        s(6.0),
        Some(over(HUD, ACCENT, 0x30)),
        Some(over(HUD, ACCENT, 0x60)),
    );

    let font = ctx.fonts.get(11.0, false);
    let text = if ctx.vm.click_through {
        "Ghost (Click-Through) Mode Active: mouse clicks pass directly through to background apps. Press Ctrl+Shift+R or use tray icon to toggle."
    } else {
        ""
    };

    let text_rect = RECT {
        left: rect.left + s(8.0),
        top: rect.top,
        right: rect.right - s(8.0),
        bottom: rect.bottom,
    };
    ctx.surface
        .text(text, text_rect, font, ACCENT_CYAN, TextOpts::centered(true));
}

fn paint_center(ctx: &mut PaintCtx, canvas: RECT, compact: bool) {
    if canvas.right - canvas.left < 2 || canvas.bottom - canvas.top < 2 {
        return;
    }

    if compact {
        paint_strip(ctx, canvas);
        return;
    }

    paint_treemap(ctx, canvas);

    if ctx.vm.selected_process.is_some() {
        paint_detail_modal(ctx, canvas);
    }
}

fn paint_treemap(ctx: &mut PaintCtx, canvas: RECT) {
    let PaintCtx {
        surface,
        scale,
        fonts,
        icons,
        hits,
        hover,
        treemap,
        ..
    } = ctx;
    let scale = *scale;

    let saved = surface.save();
    surface.clip(canvas);

    for entry in &treemap.boxes {
        let rect = RECT {
            left: canvas.left + scale_px(scale, entry.current.left),
            top: canvas.top + scale_px(scale, entry.current.top),
            right: canvas.left + scale_px(scale, entry.current.right),
            bottom: canvas.top + scale_px(scale, entry.current.bottom),
        };
        if rect.right - rect.left < 2 || rect.bottom - rect.top < 2 {
            continue;
        }

        let id = HitId::Box(entry.key.clone());
        let hovered = hover.as_ref() == Some(&id);

        let saved_box = surface.save();
        surface.clip(rect);
        paint_treemap_box(
            surface,
            fonts,
            icons,
            scale,
            rect,
            &entry.item,
            entry.opacity,
            hovered,
        );
        surface.restore(saved_box);

        hits.push(HitRegion { rect, id });
    }

    surface.restore(saved);
}

fn paint_treemap_box(
    surface: &Surface,
    fonts: &mut FontCache,
    icons: &mut IconCache,
    scale: f32,
    rect: RECT,
    item: &TreemapRectangle,
    opacity: f32,
    hovered: bool,
) {
    let s = |dip: f32| scale_px(scale, dip);
    let factor = opacity.clamp(0.0, 1.0);
    let text_alpha = (255.0 * factor).round().clamp(0.0, 255.0) as u8;

    let fill_alpha = if hovered { 0x38u8 as f32 } else { 0x1F as f32 } * factor;
    let border_alpha = if hovered { 0x80u8 as f32 } else { 0x35 as f32 } * factor;

    let fill = over(HUD, WHITE, fill_alpha.round() as u8);
    let border = over(fill, WHITE, border_alpha.round() as u8);

    surface.round_rect(rect, s(6.0), Some(fill), Some(border));

    let inner = RECT {
        left: rect.left + s(4.0),
        top: rect.top + s(3.0),
        right: rect.right - s(4.0),
        bottom: rect.bottom - s(3.0),
    };
    if inner.right - inner.left < s(4.0) || inner.bottom - inner.top < s(6.0) {
        return;
    }

    let process = &item.process;
    let compact = item.height < 36.0;
    let name_font = fonts.get(if compact { 10.0 } else { 11.0 }, true);

    let icon_size = s(14.0);
    let icon_rect = RECT {
        left: inner.left,
        top: inner.top,
        right: inner.left + icon_size,
        bottom: inner.top + icon_size,
    };
    let has_icon = icons.draw(
        surface,
        &icon_key(process),
        process.icon_png_bytes.as_deref(),
        icon_rect,
        text_alpha,
    );

    let text_left = inner.left + if has_icon { icon_size + s(4.0) } else { 0 };

    let badge = if process.instance_count > 1 {
        Some(format!("×{}", process.instance_count))
    } else {
        None
    };
    let badge_font = fonts.get(9.0, true);
    let badge_width = badge
        .as_ref()
        .map(|text| surface.text_width(text, badge_font) + s(6.0))
        .unwrap_or(0);

    let name_rect = RECT {
        left: text_left,
        top: inner.top,
        right: inner.right - if badge.is_some() { badge_width + s(2.0) } else { 0 },
        bottom: inner.top + icon_size,
    };
    surface.text(
        &process.process_name,
        name_rect,
        name_font,
        over(fill, TEXT_PRIMARY, text_alpha),
        TextOpts::left(true),
    );

    if let Some(badge) = badge {
        let badge_rect = RECT {
            left: inner.right - badge_width,
            top: inner.top,
            right: inner.right,
            bottom: inner.top + s(13.0),
        };
        surface.round_rect(
            badge_rect,
            s(3.0),
            Some(over(fill, WHITE, (0x35u8 as f32 * factor).round() as u8)),
            None,
        );
        surface.text(
            &badge,
            badge_rect,
            badge_font,
            over(fill, Rgb::new(0xE5, 0xE7, 0xEB), text_alpha),
            TextOpts::centered(false),
        );
    }

    if compact {
        return;
    }

    let ram_font = fonts.get(13.0, true);
    let ram_row = RECT {
        left: inner.left,
        top: inner.top + icon_size + s(2.0),
        right: inner.right,
        bottom: inner.top + icon_size + s(2.0) + s(16.0),
    };
    surface.text(
        &process.formatted_memory(),
        ram_row,
        ram_font,
        over(fill, TEXT_PRIMARY, text_alpha),
        TextOpts::left(true),
    );

    let percentage_font = fonts.get(10.0, false);
    surface.text(
        &process.formatted_percentage(),
        RECT {
            left: inner.left,
            top: ram_row.bottom,
            right: inner.right,
            bottom: ram_row.bottom + s(15.0),
        },
        percentage_font,
        over(fill, TEXT_MUTED, text_alpha),
        TextOpts::left(true),
    );
}

fn icon_key(process: &ProcessMemoryInfo) -> String {
    match process.executable_path.as_deref() {
        Some(path) if !path.is_empty() => path.to_lowercase(),
        _ => process.process_name.to_lowercase(),
    }
}

fn paint_strip(ctx: &mut PaintCtx, canvas: RECT) {
    let PaintCtx {
        surface,
        scale,
        fonts,
        icons,
        hits,
        hover,
        vm,
        strip_scroll,
        ..
    } = ctx;
    let scale = *scale;

    surface.round_rect(
        canvas,
        scale_px(scale, 8.0),
        Some(over(HUD, Rgb::new(0x0B, 0x0E, 0x14), 0x20)),
        None,
    );

    let inner = RECT {
        left: canvas.left + scale_px(scale, 3.0),
        top: canvas.top + scale_px(scale, 3.0),
        right: canvas.right - scale_px(scale, 3.0),
        bottom: canvas.bottom - scale_px(scale, 3.0),
    };
    if inner.right - inner.left < 2 || inner.bottom - inner.top < 2 {
        return;
    }

    let item_height = scale_px(scale, 32.0).min(inner.bottom - inner.top);
    let item_top = inner.top + ((inner.bottom - inner.top) - item_height) / 2;

    let name_font = fonts.get(11.0, true);
    let memory_font = fonts.get(10.0, false);

    let saved = surface.save();
    surface.clip(inner);

    let mut x = inner.left - *strip_scroll;

    for process in &vm.processes {
        let name_width = surface.text_width(&process.process_name, name_font);
        let memory_width = surface.text_width(&process.formatted_memory(), memory_font);
        let item_width = scale_px(scale, 6.0)
            + scale_px(scale, 14.0)
            + scale_px(scale, 4.0)
            + name_width
            + scale_px(scale, 5.0)
            + memory_width
            + scale_px(scale, 6.0);

        let rect = RECT {
            left: x,
            top: item_top,
            right: x + item_width,
            bottom: item_top + item_height,
        };

        if rect.right > inner.left && rect.left < inner.right {
            let id = HitId::StripItem(process.process_id);
            let hovered = hover.as_ref() == Some(&id);

            let fill = if hovered {
                over(HUD, WHITE, 0x38)
            } else {
                over(HUD, WHITE, 0x1E)
            };
            let border = if hovered {
                over(HUD, WHITE, 0x80)
            } else {
                over(HUD, WHITE, 0x35)
            };

            surface.round_rect(rect, scale_px(scale, 5.0), Some(fill), Some(border));

            let icon_rect = RECT {
                left: rect.left + scale_px(scale, 6.0),
                top: rect.top + (item_height - scale_px(scale, 14.0)) / 2,
                right: rect.left + scale_px(scale, 20.0),
                bottom: rect.top + (item_height - scale_px(scale, 14.0)) / 2 + scale_px(scale, 14.0),
            };
            icons.draw(
                surface,
                &icon_key(process),
                process.icon_png_bytes.as_deref(),
                icon_rect,
                255,
            );

            let text_top = rect.top;
            surface.text(
                &process.process_name,
                RECT {
                    left: icon_rect.right + scale_px(scale, 4.0),
                    top: text_top,
                    right: rect.right,
                    bottom: rect.bottom,
                },
                name_font,
                TEXT_PRIMARY,
                TextOpts::left(false),
            );
            surface.text(
                &process.formatted_memory(),
                RECT {
                    left: icon_rect.right + scale_px(scale, 4.0) + name_width + scale_px(scale, 5.0),
                    top: text_top,
                    right: rect.right,
                    bottom: rect.bottom,
                },
                memory_font,
                ACCENT_CYAN,
                TextOpts::left(false),
            );

            hits.push(HitRegion { rect, id });
        }

        x += item_width + scale_px(scale, 4.0);
    }

    surface.restore(saved);
}

fn paint_bottom(ctx: &mut PaintCtx, rect: RECT) {
    let PaintCtx {
        surface,
        scale,
        fonts,
        hits,
        hover,
        vm,
        ..
    } = ctx;
    let scale = *scale;
    let s = |dip: f32| scale_px(scale, dip);
    let center_y = (rect.top + rect.bottom) / 2;

    let chip_font = fonts.get(11.0, false);
    let chip_font_bold = fonts.get(11.0, true);

    let presets: [(&str, &'static str); 4] = [
        ("All", "All"),
        ("Apps", "Applications"),
        ("System", "System"),
        ("Highest", "Highest"),
    ];

    let mut x = rect.left;

    for (label, preset) in presets {
        let width = surface
            .text_width(label, chip_font)
            .max(surface.text_width(label, chip_font_bold))
            + s(20.0);
        let height = s(21.0);
        let chip = RECT {
            left: x,
            top: center_y - height / 2,
            right: x + width,
            bottom: center_y + height / 2,
        };

        let selected = vm.filter_preset == preset;
        let id = HitId::Chip(preset);
        let hovered = hover.as_ref() == Some(&id);

        let fill = if selected {
            CHIP_BLUE
        } else {
            over(HUD, WHITE, 0x14)
        };
        let border = if selected {
            ACCENT_LIGHT
        } else if hovered {
            over(HUD, WHITE, 0x70)
        } else {
            over(HUD, WHITE, 0x25)
        };
        let color = if selected { WHITE } else { TEXT_MUTED };

        surface.round_rect(chip, s(12.0), Some(fill), Some(border));
        surface.text(
            label,
            chip,
            if selected { chip_font_bold } else { chip_font },
            color,
            TextOpts::centered(false),
        );

        hits.push(HitRegion { rect: chip, id });
        x += width + s(4.0);
    }

    let middle_left = x;

    // Right toggles: Group, Pin, Ghost, Lock
    let toggle_font = fonts.get(11.0, false);
    let group_width = surface.text_width("Group", toggle_font) + s(16.0);
    let pin_text_width = surface.text_width("Pin", toggle_font);
    let pin_width = s(11.0) + s(3.0) + pin_text_width + s(16.0);
    let ghost_width = surface.text_width("Ghost", toggle_font) + s(16.0);
    let lock_width = s(11.0) + s(16.0);
    let gap = s(5.0);

    let total = group_width + pin_width + ghost_width + lock_width + gap * 3;
    let mut bx = rect.right - total;

    let toggles = [
        (HitId::ToggleGroup, "Group", vm.is_grouped, 0u8),
        (HitId::TogglePin, "Pin", vm.always_on_top, 1),
        (HitId::ToggleGhost, "Ghost", vm.click_through, 2),
        (HitId::ToggleLock, "", vm.lock_position, 3),
    ];

    for (id, label, checked, kind) in toggles {
        let width = match kind {
            0 => group_width,
            1 => pin_width,
            2 => ghost_width,
            _ => lock_width,
        };
        let height = s(22.0);
        let toggle = RECT {
            left: bx,
            top: center_y - height / 2,
            right: bx + width,
            bottom: center_y + height / 2,
        };

        let hovered = hover.as_ref() == Some(&id);
        let fill = if checked {
            ACCENT
        } else {
            over(HUD, WHITE, 0x15)
        };
        let border = if checked {
            ACCENT_LIGHT
        } else if hovered {
            over(HUD, WHITE, 0x80)
        } else {
            over(HUD, WHITE, 0x25)
        };
        let color = if checked { WHITE } else { TEXT_SECONDARY };

        surface.round_rect(toggle, s(5.0), Some(fill), Some(border));

        match kind {
            0 => surface.text(
                label,
                toggle,
                toggle_font,
                color,
                TextOpts::centered(false),
            ),
            1 => {
                let icon_font = fonts.get_icon(11.0);
                surface.text(
                    "\u{E718}",
                    RECT {
                        left: toggle.left + s(8.0),
                        top: toggle.top,
                        right: toggle.left + s(19.0),
                        bottom: toggle.bottom,
                    },
                    icon_font,
                    color,
                    TextOpts::centered(false),
                );
                surface.text(
                    label,
                    RECT {
                        left: toggle.left + s(22.0),
                        top: toggle.top,
                        right: toggle.right - s(8.0),
                        bottom: toggle.bottom,
                    },
                    toggle_font,
                    color,
                    TextOpts::left(false),
                );
            }
            2 => surface.text(
                label,
                toggle,
                toggle_font,
                color,
                TextOpts::centered(false),
            ),
            _ => {
                let icon_font = fonts.get_icon(11.0);
                surface.text(
                    "\u{E72E}",
                    toggle,
                    icon_font,
                    color,
                    TextOpts::centered(false),
                );
            }
        }

        hits.push(HitRegion { rect: toggle, id });
        bx += width + gap;
    }

    let status_left = middle_left + s(4.0);
    let status_right = (rect.right - total) - s(4.0);
    if status_right > status_left {
        let status_font = fonts.get(11.0, false);
        surface.text(
            &vm.status_message,
            RECT {
                left: status_left,
                top: rect.top,
                right: status_right,
                bottom: rect.bottom,
            },
            status_font,
            TEXT_MUTED,
            TextOpts::centered(true),
        );
    }
}

fn paint_detail_modal(ctx: &mut PaintCtx, canvas: RECT) {
    let scale = ctx.scale;
    let s = |dip: f32| scale_px(scale, dip);

    // Modal backdrop blocks the treemap beneath it.
    ctx.surface
        .fill(canvas, over(HUD, Rgb::new(0, 0, 0), 0x60));
    ctx.hits.push(HitRegion {
        rect: canvas,
        id: HitId::Blocked,
    });

    let Some(process) = ctx.vm.selected_process.clone() else {
        return;
    };

    let card_width = s(380.0).min(canvas.right - canvas.left - s(8.0));
    let card_height = s(250.0).min(canvas.bottom - canvas.top - s(8.0));
    if card_width < s(80.0) || card_height < s(80.0) {
        return;
    }
    let card = RECT {
        left: (canvas.left + canvas.right) / 2 - card_width / 2,
        top: (canvas.top + canvas.bottom) / 2 - card_height / 2,
        right: (canvas.left + canvas.right) / 2 + card_width / 2,
        bottom: (canvas.top + canvas.bottom) / 2 + card_height / 2,
    };
    let card_bg = Rgb::new(0x0B, 0x0F, 0x19);
    ctx.surface.round_rect(
        card,
        s(12.0),
        Some(card_bg),
        Some(over(card_bg, WHITE, 0x40)),
    );

    let inner = RECT {
        left: card.left + s(18.0),
        top: card.top + s(18.0),
        right: card.right - s(18.0),
        bottom: card.bottom - s(18.0),
    };

    let icon_size = s(28.0);
    let icon_rect = RECT {
        left: inner.left,
        top: inner.top,
        right: inner.left + icon_size,
        bottom: inner.top + icon_size,
    };
    ctx.icons.draw(
        ctx.surface,
        &icon_key(&process),
        process.icon_png_bytes.as_deref(),
        icon_rect,
        255,
    );

    let name_font = ctx.fonts.get(16.0, true);
    let name_left = icon_rect.right + s(10.0);
    ctx.surface.text(
        &process.process_name,
        RECT {
            left: name_left,
            top: inner.top,
            right: inner.right - s(30.0),
            bottom: inner.top + s(22.0),
        },
        name_font,
        WHITE,
        TextOpts::left(true),
    );

    let pid_font = ctx.fonts.get(11.0, false);
    let pid_text = format!("PID: {}", process.process_id);
    let pid_width = ctx.surface.text_width(&pid_text, pid_font);
    let pid_row = RECT {
        left: name_left,
        top: inner.top + s(22.0),
        right: inner.right,
        bottom: inner.top + s(38.0),
    };
    ctx.surface.text(
        &pid_text,
        RECT {
            right: name_left + pid_width + s(4.0),
            ..pid_row
        },
        pid_font,
        TEXT_MUTED,
        TextOpts::left(false),
    );
    ctx.surface.text(
        &format!("• {} instances", process.instance_count),
        RECT {
            left: name_left + pid_width + s(6.0),
            ..pid_row
        },
        pid_font,
        ACCENT_LIGHT,
        TextOpts::left(false),
    );

    let close_button = RECT {
        left: inner.right - s(26.0),
        top: inner.top,
        right: inner.right,
        bottom: inner.top + s(26.0),
    };
    let close_hovered = ctx.hover.as_ref() == Some(&HitId::ModalClose);
    paint_glass_on(ctx.surface, ctx.scale, close_button, card_bg, close_hovered, false);
    paint_close_icon(ctx.surface, ctx.scale, close_button);
    ctx.hits.push(HitRegion {
        rect: close_button,
        id: HitId::ModalClose,
    });

    let details_top = inner.top + icon_size + s(14.0);
    let details = RECT {
        left: inner.left,
        top: details_top,
        right: inner.right,
        bottom: (inner.bottom - s(26.0) - s(14.0)).max(details_top + s(64.0)),
    };
    ctx.surface.round_rect(
        details,
        s(8.0),
        Some(over(card_bg, WHITE, 0x14)),
        Some(over(card_bg, WHITE, 0x20)),
    );

    let row_left = details.left + s(12.0);
    let row_right = details.right - s(12.0);
    let label_font = ctx.fonts.get(12.0, false);
    let value_font = ctx.fonts.get(13.0, true);
    let mut row_top = details.top + s(9.0);

    ctx.surface.text(
        "Working Set (RAM):",
        RECT {
            left: row_left,
            top: row_top,
            right: row_right,
            bottom: row_top + s(18.0),
        },
        label_font,
        TEXT_MUTED,
        TextOpts::left(false),
    );
    ctx.surface.text(
        &process.formatted_memory(),
        RECT {
            left: row_left,
            top: row_top,
            right: row_right,
            bottom: row_top + s(18.0),
        },
        value_font,
        SKY,
        TextOpts::right(false),
    );

    row_top += s(20.0);
    ctx.surface.text(
        "Tracked Share:",
        RECT {
            left: row_left,
            top: row_top,
            right: row_right,
            bottom: row_top + s(18.0),
        },
        label_font,
        TEXT_MUTED,
        TextOpts::left(false),
    );
    ctx.surface.text(
        &process.formatted_percentage(),
        RECT {
            left: row_left,
            top: row_top,
            right: row_right,
            bottom: row_top + s(18.0),
        },
        ctx.fonts.get(12.0, true),
        Rgb::new(0xF1, 0xF5, 0xF9),
        TextOpts::right(false),
    );

    if let Some(path) = &process.executable_path {
        row_top += s(18.0);
        ctx.surface.text(
            "Path:",
            RECT {
                left: row_left,
                top: row_top,
                right: row_left + s(34.0),
                bottom: row_top + s(16.0),
            },
            ctx.fonts.get(11.0, false),
            TEXT_MUTED,
            TextOpts::left(false),
        );
        ctx.surface.text(
            path,
            RECT {
                left: row_left + s(36.0),
                top: row_top,
                right: row_right,
                bottom: (row_top + s(40.0)).min(details.bottom - s(4.0)),
            },
            ctx.fonts.get(11.0, false),
            Rgb::new(0xCB, 0xD5, 0xE1),
            TextOpts::wrapped_left(),
        );
    }

    // Actions: Open Folder, Copy Info, End Process
    let actions_top = inner.bottom - s(26.0);
    let action_font = ctx.fonts.get(11.0, false);

    let end_label = "End Process";
    let end_width = ctx.surface.text_width(end_label, action_font) + s(20.0);
    let end_rect = RECT {
        left: inner.right - end_width,
        top: actions_top,
        right: inner.right,
        bottom: actions_top + s(26.0),
    };
    let end_hovered = ctx.hover.as_ref() == Some(&HitId::ModalEnd);
    ctx.surface.round_rect(
        end_rect,
        s(6.0),
        Some(over(card_bg, DANGER, if end_hovered { 0x70 } else { 0x50 })),
        Some(DANGER),
    );
    ctx.surface
        .text(end_label, end_rect, action_font, WHITE, TextOpts::centered(false));
    ctx.hits.push(HitRegion {
        rect: end_rect,
        id: HitId::ModalEnd,
    });

    let copy_label = "Copy Info";
    let copy_width = ctx.surface.text_width(copy_label, action_font) + s(20.0);
    let copy_rect = RECT {
        left: end_rect.left - s(6.0) - copy_width,
        top: actions_top,
        right: end_rect.left - s(6.0),
        bottom: actions_top + s(26.0),
    };
    let copy_hovered = ctx.hover.as_ref() == Some(&HitId::ModalCopy);
    paint_glass_on(ctx.surface, ctx.scale, copy_rect, card_bg, copy_hovered, false);
    ctx.surface
        .text(copy_label, copy_rect, action_font, TEXT_PRIMARY, TextOpts::centered(false));
    ctx.hits.push(HitRegion {
        rect: copy_rect,
        id: HitId::ModalCopy,
    });

    if process.executable_path.is_some() {
        let open_label = "Open Folder";
        let open_width = ctx.surface.text_width(open_label, action_font) + s(20.0);
        let open_rect = RECT {
            left: inner.left,
            top: actions_top,
            right: inner.left + open_width,
            bottom: actions_top + s(26.0),
        };
        let open_hovered = ctx.hover.as_ref() == Some(&HitId::ModalOpen);
        paint_glass_on(ctx.surface, ctx.scale, open_rect, card_bg, open_hovered, false);
        ctx.surface.text(
            open_label,
            open_rect,
            action_font,
            TEXT_PRIMARY,
            TextOpts::centered(false),
        );
        ctx.hits.push(HitRegion {
            rect: open_rect,
            id: HitId::ModalOpen,
        });
    }
}

fn paint_glass_on(
    surface: &Surface,
    scale: f32,
    rect: RECT,
    base: Rgb,
    hovered: bool,
    pressed: bool,
) {
    let fill = if pressed {
        over(base, WHITE, 0x50)
    } else if hovered {
        over(base, WHITE, 0x38)
    } else {
        over(base, WHITE, 0x1F)
    };
    let border = if hovered {
        over(base, WHITE, 0x70)
    } else {
        over(base, WHITE, 0x30)
    };
    surface.round_rect(rect, scale_px(scale, 6.0), Some(fill), Some(border));
}

struct TooltipLine {
    text: String,
    size: f32,
    bold: bool,
    color: Rgb,
}

fn tooltip_lines(vm: &MainViewModel, id: &HitId) -> Vec<TooltipLine> {
    let process = match id {
        HitId::Box(key) => vm.processes.iter().find(|p| &key_for(p) == key),
        HitId::StripItem(pid) => vm.processes.iter().find(|p| p.process_id == *pid),
        _ => None,
    };

    if let Some(process) = process {
        return vec![
            TooltipLine {
                text: process.process_name.clone(),
                size: 13.0,
                bold: true,
                color: WHITE,
            },
            TooltipLine {
                text: format!("RAM: {}", process.formatted_memory()),
                size: 12.0,
                bold: false,
                color: ACCENT_CYAN,
            },
            TooltipLine {
                text: format!("Tracked Share: {}", process.formatted_percentage()),
                size: 11.0,
                bold: false,
                color: TEXT_MUTED_2,
            },
            TooltipLine {
                text: format!("PID: {}", process.process_id),
                size: 11.0,
                bold: false,
                color: TEXT_MUTED_2,
            },
            TooltipLine {
                text: format!("Instances: {}", process.instance_count),
                size: 11.0,
                bold: false,
                color: TEXT_MUTED_2,
            },
        ];
    }

    let text = match id {
        HitId::ToggleMode => "Toggle Compact Strip / Overlay Mode",
        HitId::Settings => "Settings & Preferences",
        HitId::Coffee => "Buy Me a Coffee / Support Developer",
        HitId::Minimize => "Minimize to System Tray",
        HitId::CloseWindow => "Close (Minimizes to Tray)",
        HitId::ToggleGroup => "Toggle process grouping (aggregates chrome instances, etc.)",
        HitId::TogglePin => "Pin as Always on Top HUD",
        HitId::ToggleGhost => "Toggle click-through mode (transparent to mouse)",
        HitId::ToggleLock => "Lock overlay position (prevents moving/dragging)",
        _ => return Vec::new(),
    };

    vec![TooltipLine {
        text: text.to_string(),
        size: 11.0,
        bold: false,
        color: TEXT_PRIMARY,
    }]
}

fn paint_tooltip(ctx: &mut PaintCtx, canvas: RECT) {
    let Some(id) = ctx.tooltip.clone() else {
        return;
    };
    let lines = tooltip_lines(ctx.vm, &id);
    if lines.is_empty() {
        return;
    }

    let scale = ctx.scale;
    let s = |dip: f32| scale_px(scale, dip);
    let padding = s(8.0);
    let gap = s(2.0);

    let mut width = 0;
    let mut height = padding * 2;
    let mut measured: Vec<(String, crate::render::Font, Rgb, i32)> = Vec::new();

    for (index, line) in lines.iter().enumerate() {
        let font = ctx.fonts.get(line.size, line.bold);
        let line_width = ctx.surface.text_width(&line.text, font);
        let line_height = ctx.surface.text_height(font);
        width = width.max(line_width);
        height += line_height;
        if index + 1 < lines.len() {
            height += gap;
        }
        measured.push((line.text.clone(), font, line.color, line_height));
    }

    width += padding * 2;
    let max_width = canvas.right - canvas.left;
    let max_height = canvas.bottom - canvas.top;
    if width > max_width {
        width = max_width;
    }
    if height > max_height {
        height = max_height;
    }

    let x = (ctx.cursor.x + s(14.0))
        .min(canvas.right - width - s(4.0))
        .max(canvas.left + s(4.0));
    let y = (ctx.cursor.y + s(18.0))
        .min(canvas.bottom - height - s(4.0))
        .max(canvas.top + s(4.0));

    let tooltip = RECT {
        left: x,
        top: y,
        right: x + width,
        bottom: y + height,
    };
    let background = Rgb::new(0x12, 0x16, 0x22);
    ctx.surface.round_rect(
        tooltip,
        s(6.0),
        Some(background),
        Some(over(background, WHITE, 0x50)),
    );

    let saved = ctx.surface.save();
    ctx.surface.clip(tooltip);

    let mut top = tooltip.top + padding;
    for (text, font, color, line_height) in measured {
        ctx.surface.text(
            &text,
            RECT {
                left: tooltip.left + padding,
                top,
                right: tooltip.right - padding,
                bottom: top + line_height,
            },
            font,
            color,
            TextOpts::left(false),
        );
        top += line_height + gap;
    }

    ctx.surface.restore(saved);
}
