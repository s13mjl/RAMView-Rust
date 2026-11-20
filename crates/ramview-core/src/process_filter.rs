use crate::models::ProcessMemoryInfo;

/// System process names used by ProcessMonitor.IsSystemProcess (§13 filters).
const SYSTEM_PROCESS_NAMES: [&str; 14] = [
    "System",
    "Idle",
    "Registry",
    "smss",
    "csrss",
    "wininit",
    "services",
    "lsass",
    "svchost",
    "fontdrvhost",
    "winlogon",
    "dwm",
    "sihost",
    "Memory Compression",
];

pub fn is_system_process(pid: u32, name: &str) -> bool {
    if pid == 0 || pid == 4 {
        return true;
    }
    SYSTEM_PROCESS_NAMES
        .iter()
        .any(|s| s.eq_ignore_ascii_case(name))
}

/// Applies Process Grouping (§11): aggregate same-name instances into one entry.
/// Group order follows first appearance; the primary instance is the one with the
/// highest working set (first on ties), matching LINQ GroupBy + OrderByDescending.
pub fn group_processes(processes: Vec<ProcessMemoryInfo>) -> Vec<ProcessMemoryInfo> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: std::collections::HashMap<String, Vec<ProcessMemoryInfo>> =
        std::collections::HashMap::new();

    for process in processes {
        let key = process.process_name.to_lowercase();
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(process);
    }

    let mut result: Vec<ProcessMemoryInfo> = Vec::with_capacity(order.len());
    for key in order {
        let group = &groups[&key];
        let mut primary = &group[0];
        for item in group {
            if item.working_set64 > primary.working_set64 {
                primary = item;
            }
        }

        let total_working_set: i64 = group.iter().map(|x| x.working_set64).sum();
        result.push(ProcessMemoryInfo {
            process_id: primary.process_id,
            process_name: primary.process_name.clone(),
            working_set64: total_working_set,
            percentage_of_tracked: 0.0,
            executable_path: primary.executable_path.clone(),
            icon_png_bytes: None,
            is_responding: group.iter().all(|x| x.is_responding),
            start_time: primary.start_time,
            timestamp: chrono::Utc::now(),
            instance_count: group.len() as i32,
        });
    }

    result
}

/// Applies Filter Presets (§13).
pub fn apply_filter(list: Vec<ProcessMemoryInfo>, filter: &str) -> Vec<ProcessMemoryInfo> {
    match filter {
        "Highest" => {
            let mut sorted = list;
            sort_by_working_set_desc(&mut sorted);
            sorted.truncate(25);
            sorted
        }
        "System" => list
            .into_iter()
            .filter(|x| is_system_process(x.process_id, &x.process_name))
            .collect(),
        "Applications" => list
            .into_iter()
            .filter(|x| !is_system_process(x.process_id, &x.process_name))
            .collect(),
        _ => list,
    }
}

pub fn sort_by_working_set_desc(list: &mut [ProcessMemoryInfo]) {
    // slice::sort_by is stable, matching LINQ's OrderByDescending stability.
    list.sort_by(|a, b| b.working_set64.cmp(&a.working_set64));
}

/// Computes totalTrackedMemory (§6) and assigns relative percentages & icons
/// for the visible top 50 items.
pub fn assign_percentages_and_icons<F>(list: &mut [ProcessMemoryInfo], mut icon_for: F)
where
    F: FnMut(&ProcessMemoryInfo) -> Option<Vec<u8>>,
{
    let total_tracked: i64 = list.iter().map(|x| x.working_set64).sum();
    let top_icon_count = list.len().min(50);

    for i in 0..list.len() {
        let percentage = if total_tracked > 0 {
            (list[i].working_set64 as f64 / total_tracked as f64) * 100.0
        } else {
            0.0
        };

        // Only extract icons for visible candidates (top 50) to keep polling under 15ms
        let icon_bytes = if i < top_icon_count {
            icon_for(&list[i])
        } else {
            None
        };

        list[i].percentage_of_tracked = percentage;
        list[i].icon_png_bytes = icon_bytes;
    }
}
