use ramview_core::hotkey::{parse_chord, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN};
use ramview_core::metrics::SystemMetricsProvider;
use ramview_core::models::{AppSettings, SystemMemoryVisualState};

/// HotkeyService.ParseChord parity.
#[test]
fn hotkey_chord_parses_default() {
    let (modifiers, vk) = parse_chord("Ctrl+Shift+R");
    assert_eq!(modifiers, MOD_CONTROL | MOD_SHIFT);
    assert_eq!(vk, b'R' as u32);
}

#[test]
fn hotkey_chord_parses_aliases_and_digits() {
    let (modifiers, vk) = parse_chord("control + alt + win + 7");
    assert_eq!(modifiers, MOD_CONTROL | MOD_ALT | MOD_WIN);
    assert_eq!(vk, b'7' as u32);
}

#[test]
fn hotkey_chord_blank_defaults_to_ctrl_shift_r() {
    let (modifiers, vk) = parse_chord("   ");
    assert_eq!(modifiers, MOD_CONTROL | MOD_SHIFT);
    assert_eq!(vk, b'R' as u32);
}

/// SystemMetricsProvider.EvaluateState parity.
#[test]
fn visual_state_matches_thresholds() {
    let settings = AppSettings::default();

    assert_eq!(
        SystemMetricsProvider::evaluate_state(0.0, &settings),
        SystemMemoryVisualState::Unknown
    );
    assert_eq!(
        SystemMetricsProvider::evaluate_state(42.0, &settings),
        SystemMemoryVisualState::Normal
    );
    assert_eq!(
        SystemMetricsProvider::evaluate_state(80.0, &settings),
        SystemMemoryVisualState::High
    );
    assert_eq!(
        SystemMetricsProvider::evaluate_state(90.0, &settings),
        SystemMemoryVisualState::Critical
    );
}
