use std::time::{Duration, Instant};

use chrono::TimeZone;
use ramview_core::models::ProcessMemoryInfo;
use ramview_core::process_filter::{
    apply_filter, assign_percentages_and_icons, group_processes, is_system_process,
    sort_by_working_set_desc,
};
use sysinfo::{Pid, ProcessesToUpdate, System};

use crate::icon_extractor::IconExtractor;
use crate::responding::is_process_responding;

/// Resilient process monitoring service: enumerates processes, aggregates Working Set
/// memory, filters/groups, and attaches icons for the visible top items.
pub struct ProcessMonitor {
    icon_extractor: IconExtractor,
}

impl ProcessMonitor {
    pub fn new(icon_extractor: IconExtractor) -> Self {
        Self { icon_extractor }
    }

    pub fn icon_extractor(&self) -> &IconExtractor {
        &self.icon_extractor
    }

    pub fn get_processes(
        &self,
        group_processes_flag: bool,
        show_system_processes: bool,
        filter_preset: &str,
    ) -> Vec<ProcessMemoryInfo> {
        let mut system = System::new();
        system.refresh_processes(ProcessesToUpdate::All, true);

        // Enumerate in PID order for deterministic results (Process.GetProcesses order).
        let mut pids: Vec<Pid> = system.processes().keys().copied().collect();
        pids.sort_by_key(|pid| pid.as_u32());

        let mut collected: Vec<ProcessMemoryInfo> = Vec::with_capacity(pids.len());

        for pid in pids {
            let Some(process) = system.process(pid) else {
                continue;
            };

            let process_id = pid.as_u32();
            let working_set = process.memory() as i64;
            if working_set <= 0 {
                continue;
            }

            let process_name = normalize_process_name(process_id, process.name());
            if process_name.is_empty() {
                continue;
            }

            let system_process = is_system_process(process_id, &process_name);
            if !show_system_processes && system_process {
                continue;
            }

            // Each field is probed independently: failures never abort the cycle (§22).
            let is_responding = is_process_responding(process_id);

            let executable_path = process
                .exe()
                .and_then(|path| path.to_str())
                .map(|s| s.to_string());

            let start_time = {
                let seconds = process.start_time();
                if seconds > 0 {
                    chrono::Local
                        .timestamp_opt(seconds as i64, 0)
                        .single()
                } else {
                    None
                }
            };

            let mut info = ProcessMemoryInfo::new(process_id, process_name, working_set);
            info.executable_path = executable_path;
            info.is_responding = is_responding;
            info.start_time = start_time;
            info.instance_count = 1;
            collected.push(info);
        }

        // Apply Process Grouping (§11)
        let mut result_list = if group_processes_flag {
            group_processes(collected)
        } else {
            collected
        };

        // Apply Filter Presets (§13)
        result_list = apply_filter(result_list, filter_preset);

        // Sort descending by WorkingSet64
        sort_by_working_set_desc(&mut result_list);

        // Compute totalTrackedMemory (§6) and assign percentages & icons
        assign_percentages_and_icons(&mut result_list, |item| {
            self.icon_extractor
                .get_icon_png(item.executable_path.as_deref(), &item.process_name)
        });

        result_list
    }
}

/// Resolves a live process name for the local API kill endpoint.
pub fn process_name_for_pid(pid: u32) -> Option<String> {
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);
    system
        .process(Pid::from_u32(pid))
        .map(|process| normalize_process_name(pid, process.name()))
}

/// Maps a sysinfo process name onto the .NET `Process.ProcessName` shape:
/// PID 0 is "Idle", PID 4 is "System", and the ".exe" suffix is trimmed.
pub(crate) fn normalize_process_name(pid: u32, name: &std::ffi::OsStr) -> String {
    if pid == 0 {
        return "Idle".to_string();
    }
    if pid == 4 {
        return "System".to_string();
    }

    let mut value = name.to_string_lossy().to_string();
    if value.len() > 4 && value.to_ascii_lowercase().ends_with(".exe") {
        value.truncate(value.len() - 4);
    }
    value
}

/// Simple wall-clock throttle helper used by the app's tray tooltip updates (§16).
#[derive(Debug)]
pub struct Throttle {
    interval: Duration,
    last: Option<Instant>,
}

impl Default for Throttle {
    fn default() -> Self {
        Self {
            interval: Duration::from_millis(500),
            last: None,
        }
    }
}

impl Throttle {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last: None,
        }
    }

    pub fn ready(&mut self) -> bool {
        let now = Instant::now();
        match self.last {
            Some(last) if now.duration_since(last) < self.interval => false,
            _ => {
                self.last = Some(now);
                true
            }
        }
    }
}
