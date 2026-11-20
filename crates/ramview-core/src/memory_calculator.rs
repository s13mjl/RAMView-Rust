use crate::models::ProcessMemoryInfo;

/// Handles memory calculations, floor clamping, anti-dominance adjustment,
/// and percentage computations per specification §6.
pub struct MemoryCalculator;

impl MemoryCalculator {
    /// Computes adjusted layout weights for treemap rendering.
    /// Enforces the minimum visual floor and anti-dominance capping (e.g. max 60% area).
    pub fn calculate_layout_weights(
        processes: &[ProcessMemoryInfo],
        min_floor_bytes: i64,
        max_share_ratio: f64,
    ) -> Vec<(ProcessMemoryInfo, f64)> {
        if processes.is_empty() {
            return Vec::new();
        }

        // 1. Calculate raw weights applying the minimum floor
        let mut raw_items: Vec<(ProcessMemoryInfo, f64)> = Vec::with_capacity(processes.len());
        for p in processes {
            // Ensure minimum visual floor so small processes do not disappear (§6)
            let weight = (p.working_set64 as f64).max(min_floor_bytes as f64);
            raw_items.push((p.clone(), weight));
        }

        let total_raw_weight: f64 = raw_items.iter().map(|(_, w)| *w).sum();
        if total_raw_weight <= 0.0 {
            return raw_items
                .into_iter()
                .map(|(process, _)| (process, 1.0))
                .collect();
        }

        // 2. Enforce Anti-Dominance Rule (§6):
        // No single process may occupy more than maxShareRatio (e.g. 60%) of the treemap.
        let max_allowed_weight = total_raw_weight * max_share_ratio;

        let mut dominance_adjusted = false;
        let mut non_dominant_raw_sum = 0.0;
        let mut dominant_total_allocated = 0.0;
        let mut dominant_indices: Vec<usize> = Vec::new();

        for (i, (_, raw_weight)) in raw_items.iter().enumerate() {
            if *raw_weight > max_allowed_weight && raw_items.len() > 1 {
                dominant_indices.push(i);
                dominant_total_allocated += max_allowed_weight;
                dominance_adjusted = true;
            } else {
                non_dominant_raw_sum += *raw_weight;
            }
        }

        let mut adjusted: Vec<(ProcessMemoryInfo, f64)> = Vec::with_capacity(raw_items.len());

        if dominance_adjusted && non_dominant_raw_sum > 0.0 {
            let mut remaining_budget = total_raw_weight - dominant_total_allocated;
            if remaining_budget <= 0.0 {
                remaining_budget = total_raw_weight * (1.0 - max_share_ratio);
            }

            for (i, (process, raw_weight)) in raw_items.iter().enumerate() {
                if dominant_indices.contains(&i) {
                    adjusted.push((process.clone(), max_allowed_weight));
                } else {
                    let scaled_weight = (raw_weight / non_dominant_raw_sum) * remaining_budget;
                    adjusted.push((process.clone(), scaled_weight.max(1.0)));
                }
            }
        } else {
            adjusted = raw_items;
        }

        // Stable ordering: sort descending by Weight, tie-break by ProcessName then ProcessId (§7)
        adjusted.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| ordinal_ignore_case_cmp(&a.0.process_name, &b.0.process_name))
                .then_with(|| a.0.process_id.cmp(&b.0.process_id))
        });

        adjusted
    }
}

/// Approximation of .NET's StringComparer.OrdinalIgnoreCase ordering.
fn ordinal_ignore_case_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let a_upper = a.to_uppercase();
    let b_upper = b.to_uppercase();
    a_upper.cmp(&b_upper)
}
