use windows::Win32::Foundation::RECT;

pub struct Layout {
    pub header: RECT,
    pub banner: Option<RECT>,
    pub center: RECT,
    pub bottom: Option<RECT>,
}

/// Mirrors the XAML grid: 14px shell padding, 32px header, 10px gap,
/// optional 21px ghost banner, star-sized center, 22px bottom row.
pub fn compute(
    width: i32,
    height: i32,
    scale: f32,
    compact: bool,
    click_through: bool,
) -> Layout {
    let px = |dip: f32| -> i32 { (dip * scale).round() as i32 };

    let pad = px(14.0);
    let content = RECT {
        left: pad,
        top: pad,
        right: (width - pad).max(pad),
        bottom: (height - pad).max(pad),
    };

    let header_height = px(32.0);
    let header = RECT {
        left: content.left,
        top: content.top,
        right: content.right,
        bottom: content.top + header_height,
    };

    if compact {
        return Layout {
            header,
            banner: None,
            center: RECT {
                left: content.left,
                top: header.bottom + px(10.0),
                right: content.right,
                bottom: content.bottom,
            },
            bottom: None,
        };
    }

    let mut top = header.bottom + px(10.0);
    let banner = if click_through {
        let rect = RECT {
            left: content.left,
            top,
            right: content.right,
            bottom: top + px(21.0),
        };
        top = rect.bottom + px(6.0);
        Some(rect)
    } else {
        None
    };

    let bottom = RECT {
        left: content.left,
        top: content.bottom - px(22.0),
        right: content.right,
        bottom: content.bottom,
    };

    Layout {
        header,
        banner,
        center: RECT {
            left: content.left,
            top: top + px(2.0),
            right: content.right,
            bottom: bottom.top - px(8.0),
        },
        bottom: Some(bottom),
    }
}
