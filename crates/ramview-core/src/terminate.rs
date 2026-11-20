/// Critical process denylist (§12, §23) enforced by ProcessTerminator.
const CRITICAL_PROCESS_NAMES: [&str; 22] = [
    "System",
    "Idle",
    "Registry",
    "smss",
    "smss.exe",
    "csrss",
    "csrss.exe",
    "wininit",
    "wininit.exe",
    "services",
    "services.exe",
    "lsass",
    "lsass.exe",
    "winlogon",
    "winlogon.exe",
    "fontdrvhost",
    "fontdrvhost.exe",
    "dwm",
    "dwm.exe",
    "sihost",
    "sihost.exe",
    "Memory Compression",
];

pub fn is_critical_process(process_id: u32, process_name: &str) -> bool {
    if process_id <= 4 {
        return true;
    }

    let with_extension = format!("{process_name}.exe");
    CRITICAL_PROCESS_NAMES.iter().any(|candidate| {
        candidate.eq_ignore_ascii_case(process_name)
            || candidate.eq_ignore_ascii_case(&with_extension)
    })
}

/// Mirrors ProcessTerminator.CanTerminate.
pub fn can_terminate(process_id: u32, process_name: &str) -> Result<(), String> {
    if is_critical_process(process_id, process_name) {
        return Err(format!(
            "Action blocked: '{process_name}' (PID {process_id}) is a protected Windows core process. Terminating it would crash or destabilize your system."
        ));
    }
    Ok(())
}

/// Protected set used by the local HTTP API (§ LocalApiServer.ProtectedProcesses).
const API_PROTECTED_PROCESSES: [&str; 15] = [
    "system",
    "idle",
    "smss",
    "csrss",
    "wininit",
    "winlogon",
    "services",
    "lsass",
    "lsaiso",
    "svchost",
    "fontdrvhost",
    "dwm",
    "registry",
    "memory compression",
    "system idle process",
];

pub fn is_api_protected(process_name: &str) -> bool {
    let lower = process_name.to_lowercase();
    API_PROTECTED_PROCESSES.contains(&lower.as_str())
        || lower == "secure system"
        || API_PROTECTED_PROCESSES.contains(&lower.trim_end_matches(".exe"))
}
