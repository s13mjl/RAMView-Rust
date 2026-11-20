use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};

/// User-friendly byte formatting matching ProcessMemoryInfo.FormatBytes (§6).
pub fn format_bytes(bytes: i64) -> String {
    if bytes <= 0 {
        return "0 MB".to_string();
    }

    let kb = bytes as f64 / 1024.0;
    let mb = kb / 1024.0;
    let gb = mb / 1024.0;

    if gb >= 1.0 {
        format!("{gb:.2} GB")
    } else if mb >= 1.0 {
        format!("{mb:.1} MB")
    } else {
        format!("{kb:.0} KB")
    }
}

/// Immutable snapshot representing a running process's memory footprint and metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct ProcessMemoryInfo {
    pub process_id: u32,
    pub process_name: String,
    pub working_set64: i64,
    pub percentage_of_tracked: f64,
    pub executable_path: Option<String>,
    pub icon_png_bytes: Option<Vec<u8>>,
    pub is_responding: bool,
    pub start_time: Option<DateTime<Local>>,
    pub timestamp: DateTime<Utc>,
    pub instance_count: i32,
}

impl ProcessMemoryInfo {
    pub fn new(process_id: u32, process_name: impl Into<String>, working_set64: i64) -> Self {
        Self {
            process_id,
            process_name: process_name.into(),
            working_set64,
            percentage_of_tracked: 0.0,
            executable_path: None,
            icon_png_bytes: None,
            is_responding: true,
            start_time: None,
            timestamp: Utc::now(),
            instance_count: 1,
        }
    }

    /// User-friendly memory formatting (e.g. "2.45 GB", "512.3 MB").
    pub fn formatted_memory(&self) -> String {
        format_bytes(self.working_set64)
    }

    /// Formatted percentage string (e.g. "15.4%").
    pub fn formatted_percentage(&self) -> String {
        format!("{:.1}%", self.percentage_of_tracked)
    }
}

/// Immutable snapshot representing overall system physical memory and tracked processes.
#[derive(Clone, Debug, PartialEq)]
pub struct MemorySnapshot {
    pub total_physical_bytes: u64,
    pub used_physical_bytes: u64,
    pub available_physical_bytes: u64,
    pub total_tracked_bytes: i64,
    pub processes: Vec<ProcessMemoryInfo>,
    pub timestamp: DateTime<Utc>,
}

impl MemorySnapshot {
    pub fn new(
        total_physical_bytes: u64,
        used_physical_bytes: u64,
        available_physical_bytes: u64,
    ) -> Self {
        Self {
            total_physical_bytes,
            used_physical_bytes,
            available_physical_bytes,
            total_tracked_bytes: 0,
            processes: Vec::new(),
            timestamp: Utc::now(),
        }
    }

    pub fn system_memory_usage_percentage(&self) -> f64 {
        if self.total_physical_bytes > 0 {
            (self.used_physical_bytes as f64 / self.total_physical_bytes as f64) * 100.0
        } else {
            0.0
        }
    }

    pub fn formatted_total(&self) -> String {
        format_bytes(self.total_physical_bytes as i64)
    }

    pub fn formatted_used(&self) -> String {
        format_bytes(self.used_physical_bytes as i64)
    }

    pub fn formatted_available(&self) -> String {
        format_bytes(self.available_physical_bytes as i64)
    }

    pub fn formatted_tracked(&self) -> String {
        format_bytes(self.total_tracked_bytes)
    }

    pub fn summary_text(&self) -> String {
        format!("{} / {}", self.formatted_used(), self.formatted_total())
    }
}

/// Persisted configuration options for RAM View (§19, §20).
/// Serialized with the same PascalCase member names System.Text.Json uses,
/// so existing `%LOCALAPPDATA%\RAMView\settings.json` files load unchanged.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct AppSettings {
    pub refresh_interval_ms: i32,
    pub is_grouped: bool,
    pub always_on_top: bool,
    pub click_through: bool,
    pub lock_position: bool,
    pub start_with_windows: bool,
    pub window_opacity: f64,
    pub min_memory_floor_bytes: i64,
    pub anti_dominance_max_area_share: f64,
    pub show_system_processes: bool,
    pub app_mode: String,
    pub overlay_left: f64,
    pub overlay_top: f64,
    pub overlay_width: f64,
    pub overlay_height: f64,
    pub high_memory_threshold_percent: f64,
    pub critical_memory_threshold_percent: f64,
    pub theme: String,
    pub filter_preset: String,
    pub global_hotkey: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            refresh_interval_ms: 1000,
            is_grouped: true,
            always_on_top: true,
            click_through: false,
            lock_position: false,
            start_with_windows: false,
            window_opacity: 0.96,
            min_memory_floor_bytes: 20 * 1024 * 1024,
            anti_dominance_max_area_share: 0.60,
            show_system_processes: true,
            app_mode: "Overlay".to_string(),
            overlay_left: 100.0,
            overlay_top: 100.0,
            overlay_width: 540.0,
            overlay_height: 380.0,
            high_memory_threshold_percent: 80.0,
            critical_memory_threshold_percent: 90.0,
            theme: "Dark".to_string(),
            filter_preset: "All".to_string(),
            global_hotkey: "Ctrl+Shift+R".to_string(),
        }
    }
}

/// System memory load states for dynamic visual indicators.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SystemMemoryVisualState {
    High,
    Critical,
    #[default]
    Normal,
    Unknown,
}

/// App modes supported by MainViewModel.AppMode.
pub const APP_MODE_OVERLAY: &str = "Overlay";
pub const APP_MODE_COMPACT_STRIP: &str = "CompactStrip";
