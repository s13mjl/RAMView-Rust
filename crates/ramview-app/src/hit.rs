use windows::Win32::Foundation::RECT;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum HitId {
    ToggleMode,
    Settings,
    Coffee,
    Minimize,
    CloseWindow,
    Chip(&'static str),
    ToggleGroup,
    TogglePin,
    ToggleGhost,
    ToggleLock,
    Box(String),
    StripItem(u32),
    ModalClose,
    ModalCopy,
    ModalOpen,
    ModalEnd,
    Blocked,
    SettingsClose,
    SettingsCancel,
    SettingsSave,
    SettingsCoffee,
    SettingsSlider(u8),
    SettingsCheck(u8),
}

pub struct HitRegion {
    pub rect: RECT,
    pub id: HitId,
}

pub fn contains(rect: &RECT, x: i32, y: i32) -> bool {
    x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom
}

pub fn hit_test(hits: &[HitRegion], x: i32, y: i32) -> Option<HitId> {
    hits.iter()
        .rev()
        .find(|hit| contains(&hit.rect, x, y))
        .map(|hit| hit.id.clone())
}
