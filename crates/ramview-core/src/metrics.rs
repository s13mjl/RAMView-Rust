use crate::models::{AppSettings, SystemMemoryVisualState};

pub struct SystemMetricsProvider;

impl SystemMetricsProvider {
    pub fn evaluate_state(
        memory_usage_percentage: f64,
        settings: &AppSettings,
    ) -> SystemMemoryVisualState {
        if memory_usage_percentage <= 0.0 {
            return SystemMemoryVisualState::Unknown;
        }
        if memory_usage_percentage >= settings.critical_memory_threshold_percent {
            return SystemMemoryVisualState::Critical;
        }
        if memory_usage_percentage >= settings.high_memory_threshold_percent {
            return SystemMemoryVisualState::High;
        }
        SystemMemoryVisualState::Normal
    }
}
