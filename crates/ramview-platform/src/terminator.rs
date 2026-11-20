use std::collections::HashMap;

use ramview_core::terminate::{can_terminate, is_critical_process};
use sysinfo::{Pid, ProcessesToUpdate, System};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{
    OpenProcess, TerminateProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminateOutcome {
    Terminated,
    AlreadyExited,
    AccessDenied,
    Failed,
}

/// Hardened process termination service enforcing the strict denylist (§12, §23).
pub struct ProcessTerminator;

impl Default for ProcessTerminator {
    fn default() -> Self {
        Self
    }
}

impl ProcessTerminator {
    pub fn new() -> Self {
        Self
    }

    pub fn can_terminate(&self, process_id: u32, process_name: &str) -> Result<(), String> {
        can_terminate(process_id, process_name)
    }

    /// Mirrors `ProcessTerminator.TerminateProcessAsync`: true only when the tree was killed.
    pub fn terminate_process(&self, process_id: u32) -> bool {
        self.terminate_tree(process_id) == TerminateOutcome::Terminated
    }

    /// Kills the process and its descendants (Kill(entireProcessTree: true)).
    pub fn terminate_tree(&self, process_id: u32) -> TerminateOutcome {
        let mut system = System::new();
        system.refresh_processes(ProcessesToUpdate::All, true);

        let Some(root) = system.process(Pid::from_u32(process_id)) else {
            return TerminateOutcome::AlreadyExited;
        };

        let root_name = normalize_name(process_id, root.name());
        if is_critical_process(process_id, &root_name) {
            return TerminateOutcome::Failed;
        }

        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        for (pid, process) in system.processes() {
            if let Some(parent) = process.parent() {
                children
                    .entry(parent.as_u32())
                    .or_default()
                    .push(pid.as_u32());
            }
        }

        let mut descendants: Vec<u32> = Vec::new();
        collect_descendants(process_id, &children, &mut descendants);

        for child in descendants {
            match kill_one(child) {
                Ok(()) => {}
                Err(outcome) => return outcome,
            }
        }

        match kill_one(process_id) {
            Ok(()) => TerminateOutcome::Terminated,
            Err(outcome) => outcome,
        }
    }
}

fn collect_descendants(pid: u32, children: &HashMap<u32, Vec<u32>>, out: &mut Vec<u32>) {
    if let Some(list) = children.get(&pid) {
        for child in list {
            collect_descendants(*child, children, out);
            out.push(*child);
        }
    }
}

fn kill_one(process_id: u32) -> Result<(), TerminateOutcome> {
    unsafe {
        let handle = match OpenProcess(
            PROCESS_TERMINATE | PROCESS_SYNCHRONIZE,
            false,
            process_id,
        ) {
            Ok(handle) => handle,
            Err(_) => {
                let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
                return Err(match code {
                    5 => TerminateOutcome::AccessDenied,
                    // ERROR_INVALID_PARAMETER: the process is already gone.
                    87 => TerminateOutcome::AlreadyExited,
                    _ => TerminateOutcome::Failed,
                });
            }
        };

        let terminated = TerminateProcess(handle, 1).is_ok();
        let _ = WaitForSingleObject(handle, 2000);
        let _ = CloseHandle(handle);

        if terminated {
            Ok(())
        } else {
            let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
            if code == 5 {
                Err(TerminateOutcome::AccessDenied)
            } else {
                Err(TerminateOutcome::Failed)
            }
        }
    }
}

fn normalize_name(pid: u32, name: &std::ffi::OsStr) -> String {
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
