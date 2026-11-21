use ramview_core::models::AppSettings;
use windows::Win32::Foundation::RECT;

use crate::app::RamViewApp;
use crate::hit::{HitId, HitRegion};
use crate::render::{
    over, Rgb, Surface, TextOpts, ACCENT, ACCENT_LIGHT, AMBER, AMBER_BORDER, HUD, TEXT_MUTED,
    TEXT_PRIMARY, WHITE,
};

const CARD: Rgb = Rgb::new(0x0F, 0x17, 0x2A);
const LABEL: Rgb = Rgb::new(0xE2, 0xE8, 0xF0);

#[derive(Clone)]
pub struct SettingsForm {
    pub refresh_interval_ms: f64,
    pub window_opacity: f64,
    pub anti_dominance: f64,
    pub always_on_top: bool,
    pub click_through: bool,
    pub lock_position: bool,
    pub is_grouped: bool,
    pub show_system: bool,
    pub start_with_windows: bool,
}

impl SettingsForm {
    pub fn from_settings(settings: &AppSettings, startup_enabled: bool) -> Self {
        Self {
            refresh_interval_ms: settings.refresh_interval_ms as f64,
            window_opacity: settings.window_opacity,
            anti_dominance: settings.anti_dominance_max_area_share,
            always_on_top: settings.always_on_top,
            click_through: settings.click_through,
            lock_position: settings.lock_position,
            is_grouped: settings.is_grouped,
            show_system: settings.show_system_processes,
            start_with_windows: startup_enabled,
        }
    }
}

struct Geometry {
    body: RECT,
    content_top: i32,
    footer: RECT,
}

fn s(scale: f32, dip: f32) -> i32 {
    (dip * scale).round() as i32
}

fn geometry(scale: f32, width: i32, height: i32, scroll: i32) -> Geometry {
    let pad = s(scale, 20.0);
    let header_bottom = pad + s(scale, 26.0);
    let body = RECT {
        left: pad,
        top: header_bottom + s(scale, 16.0),
        right: width - pad,
        bottom: height - pad - s(scale, 26.0) - s(scale, 16.0),
    };
    let footer = RECT {
        left: pad,
        top: height - pad - s(scale, 26.0),
        right: width - pad,
        bottom: height - pad,
    };

    Geometry {
        body,
        content_top: body.top - scroll,
        footer,
    }
}

fn slider_track(scale: f32, body: &RECT, content_top: i32, index: u8) -> RECT {
    let row_top = content_top + s(scale, 54.0) * index as i32;
    RECT {
        left: body.left,
        top: row_top + s(scale, 20.0),
        right: body.right - s(scale, 60.0),
        bottom: row_top + s(scale, 23.0),
    }
}

fn slider_label_rect(scale: f32, body: &RECT, content_top: i32, index: u8) -> RECT {
    let row_top = content_top + s(scale, 54.0) * index as i32;
    RECT {
        left: body.left,
        top: row_top,
        right: body.right,
        bottom: row_top + s(scale, 18.0),
    }
}

fn checkbox_rect(scale: f32, body: &RECT, content_top: i32, index: u8) -> RECT {
    let base = content_top + s(scale, 54.0) * 3 + s(scale, 4.0);
    let row_top = base + s(scale, 26.0) * index as i32;
    RECT {
        left: body.left,
        top: row_top,
        right: body.right,
        bottom: row_top + s(scale, 22.0),
    }
}

fn coffee_rect(scale: f32, body: &RECT, content_top: i32) -> RECT {
    let base = content_top + s(scale, 54.0) * 3 + s(scale, 4.0) + s(scale, 26.0) * 6;
    RECT {
        left: body.left,
        top: base + s(scale, 12.0),
        right: body.right,
        bottom: base + s(scale, 12.0) + s(scale, 62.0),
    }
}

fn clip_to(rect: RECT, bounds: RECT) -> Option<RECT> {
    let clipped = RECT {
        left: rect.left.max(bounds.left),
        top: rect.top.max(bounds.top),
        right: rect.right.min(bounds.right),
        bottom: rect.bottom.min(bounds.bottom),
    };
    if clipped.right > clipped.left && clipped.bottom > clipped.top {
        Some(clipped)
    } else {
        None
    }
}

fn slider_values(index: u8, form: &SettingsForm) -> (f64, f64, f64, f64) {
    match index {
        0 => (
            form.refresh_interval_ms,
            250.0,
            3000.0,
            250.0,
        ),
        1 => (form.window_opacity, 0.30, 1.0, 0.05),
        _ => (form.anti_dominance, 0.30, 0.85, 0.05),
    }
}

pub fn set_slider_from_x(app: &mut RamViewApp, index: u8, x: i32) {
    let (width, height) = settings_client_size(app);
    if width <= 0 || height <= 0 {
        return;
    }
    let geo = geometry(app.scale, width, height, app.settings_scroll);
    let track = slider_track(app.scale, &geo.body, geo.content_top, index);
    let span = (track.right - track.left).max(1) as f64;
    let fraction = (((x - track.left) as f64) / span).clamp(0.0, 1.0);

    let (_, minimum, maximum, step) = slider_values(index, &app.settings_form);
    let raw = minimum + fraction * (maximum - minimum);
    let snapped = (raw / step).round() * step;
    let value = snapped.clamp(minimum, maximum);

    match index {
        0 => app.settings_form.refresh_interval_ms = value,
        1 => app.settings_form.window_opacity = value,
        _ => app.settings_form.anti_dominance = value,
    }
}

pub fn toggle_check(app: &mut RamViewApp, index: u8) {
    let form = &mut app.settings_form;
    match index {
        0 => form.always_on_top = !form.always_on_top,
        1 => form.click_through = !form.click_through,
        2 => form.lock_position = !form.lock_position,
        3 => form.is_grouped = !form.is_grouped,
        4 => form.show_system = !form.show_system,
        _ => form.start_with_windows = !form.start_with_windows,
    }
}

fn settings_client_size(app: &RamViewApp) -> (i32, i32) {
    if app.settings_hwnd.is_invalid() {
        return (0, 0);
    }
    unsafe {
        let mut client = RECT::default();
        let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(app.settings_hwnd, &mut client);
        (client.right, client.bottom)
    }
}

pub fn paint(app: &mut RamViewApp, surface: &Surface) {
    let scale = app.scale;
    let width = surface.width;
    let height = surface.height;
    let pad = s(scale, 20.0);
    let geo = geometry(scale, width, height, app.settings_scroll);

    surface.begin_frame();
    surface.clear(CARD);
    surface.round_rect(
        RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        },
        s(scale, 12.0),
        None,
        Some(over(CARD, WHITE, 0x40)),
    );

    app.settings_hits.clear();

    // Header
    let title_font = app.fonts.get(16.0, true);
    surface.text(
        "Settings & Preferences",
        RECT {
            left: pad,
            top: pad,
            right: width - pad - s(scale, 30.0),
            bottom: pad + s(scale, 26.0),
        },
        title_font,
        WHITE,
        TextOpts::left(false),
    );

    let close = RECT {
        left: width - pad - s(scale, 26.0),
        top: pad,
        right: width - pad,
        bottom: pad + s(scale, 26.0),
    };
    let close_hovered = app.settings_hover.as_ref() == Some(&HitId::SettingsClose);
    surface.round_rect(
        close,
        s(scale, 6.0),
        Some(if close_hovered {
            over(CARD, WHITE, 0x38)
        } else {
            over(CARD, WHITE, 0x1F)
        }),
        Some(over(CARD, WHITE, 0x30)),
    );
    let cross = s(scale, 4.0) as f32;
    let cx = (close.left as f32 + close.right as f32) * 0.5;
    let cy = (close.top as f32 + close.bottom as f32) * 0.5;
    let cross_width = (1.2 * scale).max(1.0);
    surface.line_f32(cx - cross, cy - cross, cx + cross, cy + cross, TEXT_PRIMARY, cross_width);
    surface.line_f32(cx - cross, cy + cross, cx + cross, cy - cross, TEXT_PRIMARY, cross_width);
    app.settings_hits.push(HitRegion {
        rect: close,
        id: HitId::SettingsClose,
    });

    // Body (scrolled)
    let saved = surface.save();
    surface.clip(geo.body);

    let label_font = app.fonts.get(12.0, true);
    let value_font = app.fonts.get(11.0, false);
    let slider_labels = [
        "Update Cadence (Polling Interval):",
        "Window Glass Opacity:",
        "Anti-Dominance Max Box Share (§6):",
    ];

    for index in 0..3u8 {
        let label_rect = slider_label_rect(scale, &geo.body, geo.content_top, index);
        surface.text(
            slider_labels[index as usize],
            label_rect,
            label_font,
            LABEL,
            TextOpts::left(false),
        );

        let (value, minimum, maximum, _) = slider_values(index, &app.settings_form);
        let fraction = ((value - minimum) / (maximum - minimum)).clamp(0.0, 1.0) as f32;

        let track = slider_track(scale, &geo.body, geo.content_top, index);
        surface.round_rect(
            track,
            s(scale, 2.0),
            Some(over(CARD, WHITE, 0x25)),
            None,
        );

        let filled_width = ((track.right - track.left) as f32 * fraction) as i32;
        if filled_width > 0 {
            surface.round_rect(
                RECT {
                    right: track.left + filled_width,
                    ..track
                },
                s(scale, 2.0),
                Some(ACCENT),
                None,
            );
        }

        let knob_radius = s(scale, 6.0);
        let knob_center = track.left + filled_width;
        let knob_center_y = (track.top + track.bottom) / 2;
        surface.ellipse(
            RECT {
                left: knob_center - knob_radius,
                top: knob_center_y - knob_radius,
                right: knob_center + knob_radius,
                bottom: knob_center_y + knob_radius,
            },
            ACCENT_LIGHT,
        );

        let display = match index {
            0 => format!("{:.0} ms", value),
            _ => format!("{:.0}%", value * 100.0),
        };
        surface.text(
            &display,
            RECT {
                left: track.right + s(scale, 8.0),
                top: track.top - s(scale, 6.0),
                right: geo.body.right,
                bottom: track.bottom + s(scale, 10.0),
            },
            value_font,
            TEXT_MUTED,
            TextOpts::right(false),
        );

        let hit = RECT {
            left: track.left,
            top: track.top - s(scale, 8.0),
            right: track.right,
            bottom: track.bottom + s(scale, 8.0),
        };
        if let Some(clamped) = clip_to(hit, geo.body) {
            app.settings_hits.push(HitRegion {
                rect: clamped,
                id: HitId::SettingsSlider(index),
            });
        }
    }

    let check_labels = [
        "Always on Top (HUD Overlay)",
        "Click-Through Mode (Pass clicks through overlay)",
        "Lock Overlay Position (Prevent dragging)",
        "Group Same-Name Process Instances",
        "Show Windows System Processes",
        "Start Automatically with Windows",
    ];
    let check_values = [
        app.settings_form.always_on_top,
        app.settings_form.click_through,
        app.settings_form.lock_position,
        app.settings_form.is_grouped,
        app.settings_form.show_system,
        app.settings_form.start_with_windows,
    ];

    for index in 0..6u8 {
        let row = checkbox_rect(scale, &geo.body, geo.content_top, index);
        let box_size = s(scale, 14.0);
        let checker = RECT {
            left: row.left,
            top: (row.top + row.bottom) / 2 - box_size / 2,
            right: row.left + box_size,
            bottom: (row.top + row.bottom) / 2 + box_size / 2,
        };
        let checked = check_values[index as usize];

        surface.round_rect(
            checker,
            s(scale, 3.0),
            Some(if checked {
                ACCENT
            } else {
                over(CARD, WHITE, 0x18)
            }),
            Some(if checked {
                ACCENT_LIGHT
            } else {
                over(CARD, WHITE, 0x30)
            }),
        );

        if checked {
            surface.line(
                checker.left + s(scale, 3.0),
                checker.top + s(scale, 7.0),
                checker.left + s(scale, 6.0),
                checker.top + s(scale, 10.0),
                WHITE,
                s(scale, 1.0).max(1),
            );
            surface.line(
                checker.left + s(scale, 6.0),
                checker.top + s(scale, 10.0),
                checker.left + s(scale, 11.0),
                checker.top + s(scale, 4.0),
                WHITE,
                s(scale, 1.0).max(1),
            );
        }

        surface.text(
            check_labels[index as usize],
            RECT {
                left: checker.right + s(scale, 8.0),
                top: row.top,
                right: row.right,
                bottom: row.bottom,
            },
            app.fonts.get(12.0, false),
            LABEL,
            TextOpts::left(false),
        );

        if let Some(clamped) = clip_to(row, geo.body) {
            app.settings_hits.push(HitRegion {
                rect: clamped,
                id: HitId::SettingsCheck(index),
            });
        }
    }

    // Support developer card
    let coffee = coffee_rect(scale, &geo.body, geo.content_top);
    surface.round_rect(
        coffee,
        s(scale, 8.0),
        Some(over(CARD, WHITE, 0x1A)),
        Some(over(CARD, WHITE, 0x35)),
    );
    surface.text(
        "Enjoying RAM View?",
        RECT {
            left: coffee.left + s(scale, 12.0),
            top: coffee.top + s(scale, 6.0),
            right: coffee.right - s(scale, 12.0),
            bottom: coffee.top + s(scale, 28.0),
        },
        app.fonts.get(12.0, true),
        WHITE,
        TextOpts::left(false),
    );
    surface.text(
        "Support development with a coffee",
        RECT {
            left: coffee.left + s(scale, 12.0),
            top: coffee.top + s(scale, 28.0),
            right: coffee.right - s(scale, 12.0),
            bottom: coffee.top + s(scale, 52.0),
        },
        app.fonts.get(11.0, false),
        TEXT_MUTED,
        TextOpts::left(false),
    );

    let coffee_label = "Buy Me a Coffee";
    let coffee_font = app.fonts.get(12.0, false);
    let button_width = surface.text_width(coffee_label, coffee_font) + s(scale, 20.0);
    let coffee_button = RECT {
        left: coffee.right - s(scale, 12.0) - button_width,
        top: (coffee.top + coffee.bottom) / 2 - s(scale, 13.0),
        right: coffee.right - s(scale, 12.0),
        bottom: (coffee.top + coffee.bottom) / 2 + s(scale, 13.0),
    };
    let coffee_hovered = app.settings_hover.as_ref() == Some(&HitId::SettingsCoffee);
    surface.round_rect(
        coffee_button,
        s(scale, 6.0),
        Some(over(AMBER, WHITE, if coffee_hovered { 51 } else { 0 })),
        Some(AMBER_BORDER),
    );
    surface.text(
        coffee_label,
        coffee_button,
        coffee_font,
        WHITE,
        TextOpts::centered(false),
    );

    if let Some(clamped) = clip_to(coffee_button, geo.body) {
        app.settings_hits.push(HitRegion {
            rect: clamped,
            id: HitId::SettingsCoffee,
        });
    }

    surface.restore(saved);

    // Footer
    let footer_font = app.fonts.get(12.0, false);
    let cancel = RECT {
        left: geo.footer.right - s(scale, 110.0) - s(scale, 10.0) - s(scale, 80.0),
        top: geo.footer.top,
        right: geo.footer.right - s(scale, 110.0) - s(scale, 10.0),
        bottom: geo.footer.bottom,
    };
    let save = RECT {
        left: geo.footer.right - s(scale, 110.0),
        top: geo.footer.top,
        right: geo.footer.right,
        bottom: geo.footer.bottom,
    };

    let cancel_hovered = app.settings_hover.as_ref() == Some(&HitId::SettingsCancel);
    surface.round_rect(
        cancel,
        s(scale, 6.0),
        Some(over(CARD, WHITE, if cancel_hovered { 0x38 } else { 0x1F })),
        Some(over(CARD, WHITE, 0x30)),
    );
    surface.text(
        "Cancel",
        cancel,
        footer_font,
        TEXT_PRIMARY,
        TextOpts::centered(false),
    );

    let save_hovered = app.settings_hover.as_ref() == Some(&HitId::SettingsSave);
    surface.round_rect(
        save,
        s(scale, 6.0),
        Some(over(ACCENT, WHITE, if save_hovered { 46 } else { 0 })),
        Some(ACCENT_LIGHT),
    );
    surface.text(
        "Save Settings",
        save,
        footer_font,
        WHITE,
        TextOpts::centered(false),
    );

    app.settings_hits.push(HitRegion {
        rect: cancel,
        id: HitId::SettingsCancel,
    });
    app.settings_hits.push(HitRegion {
        rect: save,
        id: HitId::SettingsSave,
    });

    // Keep HUD referenced as the compositing base for the shared palette.
    let _ = HUD;

    app.settings_max_scroll =
        (coffee.bottom + s(scale, 4.0) - geo.body.bottom).max(0);
}
