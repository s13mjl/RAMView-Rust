use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use ramview_core::metrics::SystemMetricsProvider;
use ramview_core::models::{
    AppSettings, MemorySnapshot, ProcessMemoryInfo, SystemMemoryVisualState,
};
use ramview_platform::process_monitor::ProcessMonitor;
use ramview_platform::ram_monitor::RAMMonitor;

/// Snapshot of the settings the polling cycle reads, exactly like
/// MainViewModel.PollAsync reading _settingsService.CurrentSettings.
#[derive(Clone)]
pub struct PollConfig {
    pub is_grouped: bool,
    pub show_system_processes: bool,
    pub filter_preset: String,
    pub settings: AppSettings,
}

pub struct Polled {
    pub snapshot: MemorySnapshot,
    pub processes: Vec<ProcessMemoryInfo>,
    pub visual_state: SystemMemoryVisualState,
}

/// Background polling loop mirroring the System.Threading.Timer: 150ms first tick,
/// then one cycle per refresh interval, with an on-demand trigger for commands.
pub struct PollWorker {
    trigger_tx: Option<Sender<PollConfig>>,
    result_rx: Receiver<Polled>,
    handle: Option<JoinHandle<()>>,
}

impl PollWorker {
    pub fn start(
        ram_monitor: Arc<RAMMonitor>,
        process_monitor: Arc<ProcessMonitor>,
        refresh_interval_ms: i32,
        config: PollConfig,
        wake_ui: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let (trigger_tx, trigger_rx) = channel::<PollConfig>();
        let (result_tx, result_rx) = channel::<Polled>();

        let interval = Duration::from_millis(refresh_interval_ms.max(250) as u64);
        let shared_config = Arc::new(Mutex::new(config));

        let thread_config = shared_config.clone();
        let handle = std::thread::spawn(move || {
            let mut next_poll = Instant::now() + Duration::from_millis(150);

            loop {
                let timeout = next_poll.saturating_duration_since(Instant::now());

                let triggered = match trigger_rx.recv_timeout(timeout) {
                    Ok(config) => {
                        if let Ok(mut guard) = thread_config.lock() {
                            *guard = config;
                        }
                        true
                    }
                    Err(RecvTimeoutError::Timeout) => true,
                    Err(RecvTimeoutError::Disconnected) => break,
                };

                if !triggered {
                    break;
                }

                if !poll_once(
                    &ram_monitor,
                    &process_monitor,
                    &thread_config,
                    &result_tx,
                    &wake_ui,
                ) {
                    break;
                }

                next_poll = Instant::now() + interval;
            }
        });

        Self {
            trigger_tx: Some(trigger_tx),
            result_rx,
            handle: Some(handle),
        }
    }

    pub fn trigger(&self, config: PollConfig) {
        if let Some(trigger_tx) = &self.trigger_tx {
            let _ = trigger_tx.send(config);
        }
    }

    pub fn try_recv(&self) -> Option<Polled> {
        match self.result_rx.try_recv() {
            Ok(polled) => Some(polled),
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => None,
        }
    }

    pub fn shutdown(&mut self) {
        self.trigger_tx.take();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for PollWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn poll_once(
    ram_monitor: &RAMMonitor,
    process_monitor: &ProcessMonitor,
    shared_config: &Arc<Mutex<PollConfig>>,
    result_tx: &Sender<Polled>,
    wake_ui: &Arc<dyn Fn() + Send + Sync>,
) -> bool {
    let config = match shared_config.lock() {
        Ok(guard) => guard.clone(),
        Err(_) => return true,
    };

    let mut snapshot = ram_monitor.get_system_memory_snapshot();
    let process_list = process_monitor.get_processes(
        config.is_grouped,
        config.show_system_processes,
        &config.filter_preset,
    );

    let total_tracked: i64 = process_list.iter().map(|p| p.working_set64).sum();
    snapshot.total_tracked_bytes = total_tracked;
    // The UI keeps `processes` separately; avoid duplicating the list in the snapshot.
    snapshot.processes = Vec::new();

    let visual_state = SystemMetricsProvider::evaluate_state(
        snapshot.system_memory_usage_percentage(),
        &config.settings,
    );

    let polled = Polled {
        snapshot,
        processes: process_list,
        visual_state,
    };

    if result_tx.send(polled).is_err() {
        return false;
    }

    wake_ui();
    true
}
