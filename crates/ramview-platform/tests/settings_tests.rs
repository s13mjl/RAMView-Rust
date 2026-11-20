use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use ramview_core::models::AppSettings;
use ramview_platform::settings_service::SettingsService;

/// Port of tests/RAMView.Tests/SettingsServiceTests.cs
fn temp_path(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("RAMView_Test_{label}_{nanos}.json"))
}

#[test]
fn settings_service_saves_and_loads_round_trip() {
    let path = temp_path("roundtrip");

    let mut service = SettingsService::new(Some(path.clone()));
    let mut settings = AppSettings::default();
    settings.refresh_interval_ms = 750;
    settings.is_grouped = false;
    settings.always_on_top = true;
    settings.click_through = true;
    settings.window_opacity = 0.85;
    settings.anti_dominance_max_area_share = 0.50;

    service.save_settings(settings);

    let mut loaded_service = SettingsService::new(Some(path.clone()));
    let loaded = loaded_service.load_settings();

    assert_eq!(loaded.refresh_interval_ms, 750);
    assert!(!loaded.is_grouped);
    assert!(loaded.always_on_top);
    assert!(loaded.click_through);
    assert!((loaded.window_opacity - 0.85).abs() < 0.005);
    assert!((loaded.anti_dominance_max_area_share - 0.50).abs() < 0.005);

    let _ = std::fs::remove_file(path);
}

#[test]
fn settings_service_non_existent_file_returns_defaults() {
    let path = temp_path("missing");
    let mut service = SettingsService::new(Some(path));

    let settings = service.load_settings();

    assert_eq!(settings.refresh_interval_ms, 1000);
    assert!(settings.is_grouped);
    assert!(settings.always_on_top);
    assert!(!settings.click_through);
    assert_eq!(settings.app_mode, "Overlay");
    assert_eq!(settings.filter_preset, "All");
    assert_eq!(settings.global_hotkey, "Ctrl+Shift+R");
    assert_eq!(settings.min_memory_floor_bytes, 20 * 1024 * 1024);
    assert!((settings.anti_dominance_max_area_share - 0.60).abs() < f64::EPSILON);
}

/// The rewrite must read and write the same PascalCase JSON System.Text.Json uses.
#[test]
fn settings_json_uses_pascal_case_and_round_trips() {
    let path = temp_path("pascalcase");

    let mut service = SettingsService::new(Some(path.clone()));
    service.save_settings(AppSettings::default());

    let json = std::fs::read_to_string(&path).unwrap();
    for key in [
        "\"RefreshIntervalMs\"",
        "\"IsGrouped\"",
        "\"AlwaysOnTop\"",
        "\"ClickThrough\"",
        "\"WindowOpacity\"",
        "\"MinMemoryFloorBytes\"",
        "\"AntiDominanceMaxAreaShare\"",
        "\"ShowSystemProcesses\"",
        "\"AppMode\"",
        "\"FilterPreset\"",
        "\"GlobalHotkey\"",
    ] {
        assert!(json.contains(key), "settings.json must contain {key}");
    }

    // A settings.json written by the C# original loads unchanged.
    let original = r#"{
  "RefreshIntervalMs": 500,
  "IsGrouped": false,
  "AlwaysOnTop": false,
  "ClickThrough": true,
  "LockPosition": true,
  "StartWithWindows": true,
  "WindowOpacity": 0.7,
  "MinMemoryFloorBytes": 10485760,
  "AntiDominanceMaxAreaShare": 0.45,
  "ShowSystemProcesses": false,
  "AppMode": "CompactStrip",
  "OverlayLeft": 12,
  "OverlayTop": 34,
  "OverlayWidth": 600,
  "OverlayHeight": 400,
  "HighMemoryThresholdPercent": 75,
  "CriticalMemoryThresholdPercent": 95,
  "Theme": "Dark",
  "FilterPreset": "System",
  "GlobalHotkey": "Ctrl+Alt+M"
}"#;

    std::fs::write(&path, original).unwrap();
    let loaded = service.load_settings();

    assert_eq!(loaded.refresh_interval_ms, 500);
    assert!(!loaded.is_grouped);
    assert!(loaded.click_through);
    assert_eq!(loaded.app_mode, "CompactStrip");
    assert_eq!(loaded.filter_preset, "System");
    assert_eq!(loaded.global_hotkey, "Ctrl+Alt+M");
    assert_eq!(loaded.overlay_width, 600.0);

    let _ = std::fs::remove_file(path);
}

/// Defaults for every persisted member (AppSettings property defaults).
#[test]
fn default_settings_match_the_original_members() {
    let defaults = AppSettings::default();

    assert_eq!(defaults.refresh_interval_ms, 1000);
    assert!(defaults.is_grouped);
    assert!(defaults.always_on_top);
    assert!(!defaults.click_through);
    assert!(!defaults.lock_position);
    assert!(!defaults.start_with_windows);
    assert!((defaults.window_opacity - 0.96).abs() < f64::EPSILON);
    assert_eq!(defaults.min_memory_floor_bytes, 20 * 1024 * 1024);
    assert!((defaults.anti_dominance_max_area_share - 0.60).abs() < f64::EPSILON);
    assert!(defaults.show_system_processes);
    assert_eq!(defaults.app_mode, "Overlay");
    assert_eq!(defaults.overlay_left, 100.0);
    assert_eq!(defaults.overlay_top, 100.0);
    assert_eq!(defaults.overlay_width, 540.0);
    assert_eq!(defaults.overlay_height, 380.0);
    assert_eq!(defaults.high_memory_threshold_percent, 80.0);
    assert_eq!(defaults.critical_memory_threshold_percent, 90.0);
    assert_eq!(defaults.theme, "Dark");
    assert_eq!(defaults.filter_preset, "All");
    assert_eq!(defaults.global_hotkey, "Ctrl+Shift+R");
}
