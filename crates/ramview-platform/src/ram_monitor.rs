use ramview_core::models::MemorySnapshot;
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

/// Native memory monitor querying physical RAM via Win32 GlobalMemoryStatusEx.
pub struct RAMMonitor;

impl Default for RAMMonitor {
    fn default() -> Self {
        Self
    }
}

impl RAMMonitor {
    pub fn new() -> Self {
        Self
    }

    pub fn get_system_memory_snapshot(&self) -> MemorySnapshot {
        let mut mem_status = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };

        if unsafe { GlobalMemoryStatusEx(&mut mem_status) }.is_err() {
            // Fallback in case of unexpected failure
            let mut system = sysinfo::System::new();
            system.refresh_memory();
            let total = system.total_memory();
            let used = system.used_memory();
            let avail = total.saturating_sub(used);
            return MemorySnapshot::new(total, used, avail);
        }

        let total_phys = mem_status.ullTotalPhys;
        let avail_phys = mem_status.ullAvailPhys;
        let used_phys = total_phys.saturating_sub(avail_phys);

        MemorySnapshot::new(total_phys, used_phys, avail_phys)
    }
}
