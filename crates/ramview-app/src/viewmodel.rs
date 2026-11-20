use ramview_core::algorithms::{SquarifiedTreemap, TreemapRectangle};
use ramview_core::memory_calculator::MemoryCalculator;
use ramview_core::models::{
    AppSettings, MemorySnapshot, ProcessMemoryInfo, SystemMemoryVisualState, APP_MODE_COMPACT_STRIP,
    APP_MODE_OVERLAY,
};

/// 1:1 port of RAMView.Core.ViewModels.MainViewModel state and layout rules.
pub struct MainViewModel {
    pub snapshot: Option<MemorySnapshot>,
    pub treemap_items: Vec<TreemapRectangle>,
    pub processes: Vec<ProcessMemoryInfo>,
    pub selected_process: Option<ProcessMemoryInfo>,
    pub search_text: String,
    pub filter_preset: String,
    pub is_grouped: bool,
    pub always_on_top: bool,
    pub click_through: bool,
    pub lock_position: bool,
    pub app_mode: String,
    pub window_opacity: f64,
    pub visual_state: SystemMemoryVisualState,
    pub canvas_width: f64,
    pub canvas_height: f64,
    pub status_message: String,
}

impl MainViewModel {
    pub fn new(settings: &AppSettings) -> Self {
        Self {
            snapshot: None,
            treemap_items: Vec::new(),
            processes: Vec::new(),
            selected_process: None,
            search_text: String::new(),
            filter_preset: settings.filter_preset.clone(),
            is_grouped: settings.is_grouped,
            always_on_top: settings.always_on_top,
            click_through: settings.click_through,
            lock_position: settings.lock_position,
            app_mode: settings.app_mode.clone(),
            window_opacity: settings.window_opacity,
            visual_state: SystemMemoryVisualState::Normal,
            canvas_width: 520.0,
            canvas_height: 280.0,
            status_message: "Ready".to_string(),
        }
    }

    /// Mirrors MainViewModel.RecalculateTreemapLayout (padding 3, §6 weights, §13 dimming).
    pub fn recalculate_treemap_layout(&mut self, settings: &AppSettings) {
        if self.processes.is_empty() || self.canvas_width <= 20.0 || self.canvas_height <= 20.0 {
            self.treemap_items = Vec::new();
            return;
        }

        let weighted = MemoryCalculator::calculate_layout_weights(
            &self.processes,
            settings.min_memory_floor_bytes,
            settings.anti_dominance_max_area_share,
        );

        let mut rects = SquarifiedTreemap::calculate_layout(
            &weighted,
            self.canvas_width,
            self.canvas_height,
            3.0,
        );

        let term = self.search_text.trim().to_string();
        if !term.is_empty() {
            for rect in &mut rects {
                rect.is_highlighted = contains_ignore_case(&rect.process.process_name, &term);
            }
        }

        self.treemap_items = rects;
    }

    /// Mirrors MainViewModel.ApplySearchFilterHighlighting: dims non-matches in place.
    pub fn apply_search_filter_highlighting(&mut self) {
        if self.treemap_items.is_empty() {
            return;
        }

        let term = self.search_text.trim().to_string();
        let has_search = !term.is_empty();

        for rect in &mut self.treemap_items {
            rect.is_highlighted =
                !has_search || contains_ignore_case(&rect.process.process_name, &term);
        }
    }

    pub fn toggle_mode(&mut self) {
        self.app_mode = if self.app_mode == APP_MODE_OVERLAY {
            APP_MODE_COMPACT_STRIP.to_string()
        } else {
            APP_MODE_OVERLAY.to_string()
        };
    }

    pub fn toggle_group(&mut self) -> bool {
        self.is_grouped = !self.is_grouped;
        self.is_grouped
    }

    pub fn toggle_always_on_top(&mut self) -> bool {
        self.always_on_top = !self.always_on_top;
        self.always_on_top
    }

    pub fn toggle_click_through(&mut self) -> bool {
        self.click_through = !self.click_through;
        self.click_through
    }

    pub fn toggle_lock_position(&mut self) -> bool {
        self.lock_position = !self.lock_position;
        self.lock_position
    }

    /// Mirrors MainViewModel.CopyProcessInfo text.
    pub fn copy_process_info_text(&self) -> Option<String> {
        let selected = self.selected_process.as_ref()?;
        let mut info = String::new();
        info.push_str(&format!("Process: {}\n", selected.process_name));
        info.push_str(&format!("PID: {}\n", selected.process_id));
        info.push_str(&format!("Working Set: {}\n", selected.formatted_memory()));
        info.push_str(&format!(
            "Tracked Share: {}\n",
            selected.formatted_percentage()
        ));
        info.push_str(&format!(
            "Path: {}\n",
            selected.executable_path.as_deref().unwrap_or("N/A")
        ));
        info.push_str(&format!("Instances: {}", selected.instance_count));
        Some(info)
    }
}

/// Appendix-Insensitive substring test approximating StringComparison.OrdinalIgnoreCase.
pub fn contains_ignore_case(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack
        .to_lowercase()
        .contains(&needle.to_lowercase())
}
