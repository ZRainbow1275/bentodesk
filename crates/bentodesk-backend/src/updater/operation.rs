//! Shared updater operation gate and background worker entry points.

use super::*;

/// One shared updater operation state across checks, downloads, and install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum UpdateOperation {
    Idle = 0,
    Checking = 1,
    Downloading = 2,
    Ready = 3,
    Installing = 4,
}

impl UpdateOperation {
    fn from_raw(value: u8) -> Self {
        match value {
            1 => Self::Checking,
            2 => Self::Downloading,
            3 => Self::Ready,
            4 => Self::Installing,
            _ => Self::Idle,
        }
    }
}

struct SchedulerState {
    interval: Option<Duration>,
    generation: u64,
    immediate: bool,
    running: bool,
    remaining_runs: Option<usize>,
}

/// One cancellable, reconfigurable recurring-check worker.
pub(super) struct UpdateScheduler {
    state: Arc<(Mutex<SchedulerState>, std::sync::Condvar)>,
}

impl UpdateScheduler {
    pub(super) fn new() -> Self {
        Self {
            state: Arc::new((
                Mutex::new(SchedulerState {
                    interval: None,
                    generation: 0,
                    immediate: false,
                    running: false,
                    remaining_runs: None,
                }),
                std::sync::Condvar::new(),
            )),
        }
    }

    fn configure(&self, worker: Updater, interval: Option<Duration>) {
        self.configure_inner(worker, interval, None);
    }

    #[cfg(test)]
    fn configure_for_test(&self, worker: Updater, interval: Option<Duration>) {
        self.configure_inner(worker, interval, None);
    }

    #[cfg(test)]
    fn configure_for_test_with_limit(&self, worker: Updater, interval: Duration, max_runs: usize) {
        self.configure_inner(worker, Some(interval), Some(max_runs.max(1)));
    }

    fn configure_inner(
        &self,
        worker: Updater,
        interval: Option<Duration>,
        remaining_runs: Option<usize>,
    ) {
        let (lock, wake) = &*self.state;
        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
        let was_enabled = state.interval.is_some();
        state.interval = interval.map(|value| value.max(Duration::from_millis(1)));
        state.generation = state.generation.wrapping_add(1);
        state.immediate = state.interval.is_some() && !was_enabled;
        state.remaining_runs = remaining_runs;
        if state.interval.is_some() && !state.running {
            state.running = true;
            let shared = Arc::clone(&self.state);
            let spawned = std::thread::Builder::new()
                .name("bentodesk-updater-scheduler".to_owned())
                .spawn(move || Self::run(shared, worker));
            if let Err(error) = spawned {
                state.running = false;
                tracing::warn!(
                    %error,
                    "background updater scheduler thread spawn failed; will retry on reconfigure"
                );
            }
        }
        wake.notify_all();
    }

    fn run(shared: Arc<(Mutex<SchedulerState>, std::sync::Condvar)>, worker: Updater) {
        let (lock, wake) = &*shared;
        let mut observed_generation = 0;
        loop {
            let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
            let Some(interval) = state.interval else {
                state.running = false;
                wake.notify_all();
                return;
            };
            let generation = state.generation;
            if state.immediate {
                state.immediate = false;
                observed_generation = generation;
                drop(state);
                let _ = worker.try_start_check();
                if !Self::record_run(lock, wake, generation) {
                    return;
                }
                continue;
            }
            if observed_generation != generation {
                observed_generation = generation;
            }
            let (state_after_wait, timeout) = wake
                .wait_timeout(state, interval)
                .unwrap_or_else(|error| error.into_inner());
            state = state_after_wait;
            if !timeout.timed_out() || state.generation != observed_generation {
                continue;
            }
            drop(state);
            let _ = worker.try_start_check();
            if !Self::record_run(lock, wake, generation) {
                return;
            }
        }
    }

    fn record_run(
        lock: &Mutex<SchedulerState>,
        wake: &std::sync::Condvar,
        generation: u64,
    ) -> bool {
        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
        if state.generation != generation {
            if state.interval.is_none() {
                state.running = false;
                wake.notify_all();
                return false;
            }
            return true;
        }
        if let Some(remaining) = state.remaining_runs.as_mut() {
            *remaining = remaining.saturating_sub(1);
            if *remaining == 0 {
                state.interval = None;
                state.running = false;
                wake.notify_all();
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod scheduler_state_tests {
    use super::*;

    #[test]
    fn disable_during_attempt_clears_running_and_reenable_spawns() {
        let (tx, rx) = crossbeam_channel::unbounded::<UpdateEvent>();
        let updater = Updater::with_manifest_source(tx, None);
        let (lock, wake) = &*updater.scheduler.state;
        {
            let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
            state.running = true;
            state.interval = None;
            state.generation = 2;
        }

        assert!(!UpdateScheduler::record_run(lock, wake, 1));
        assert!(
            !lock
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .running
        );

        updater.configure_scheduler_for_test(Some(Duration::from_secs(60)));
        assert!(
            lock.lock()
                .unwrap_or_else(|error| error.into_inner())
                .running,
            "reenabling after the cancelled attempt must start a scheduler"
        );
        let event = rx
            .recv_timeout(Duration::from_secs(1))
            .expect("reenabled scheduler must run immediately");
        assert!(matches!(event, UpdateEvent::UpToDate { .. }));
        updater.acknowledge_event(&event);
        updater.configure_scheduler_for_test(None);
        for _ in 0..100 {
            if !lock
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .running
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("disabled scheduler did not stop");
    }
}

/// Result of attempting to start a background updater operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateStart {
    Started,
    Joined(UpdateOperation),
    Rejected(UpdateOperation),
}

impl Updater {
    /// Current shared updater operation state.
    pub fn operation_state(&self) -> UpdateOperation {
        UpdateOperation::from_raw(self.operation.load(Ordering::Acquire))
    }

    /// Start or join the one allowed background check operation.
    pub fn try_start_check(&self) -> UpdateStart {
        let start = self.acquire_operation(UpdateOperation::Checking);
        if start == UpdateStart::Started {
            self.spawn_check_worker();
        }
        start
    }

    /// Back-compatible name for a one-shot background check.
    pub fn spawn_background_check(&self) -> UpdateStart {
        self.try_start_check()
    }

    /// Start or join the one allowed background artifact download.
    pub fn try_start_download(&self) -> UpdateStart {
        let start = self.acquire_operation(UpdateOperation::Downloading);
        if start == UpdateStart::Started {
            self.spawn_download_worker();
        }
        start
    }

    #[cfg(test)]
    pub(super) fn acquire_operation_for_test(&self, desired: UpdateOperation) -> UpdateStart {
        self.acquire_operation(desired)
    }

    /// Fire-and-forget recurring background checks.
    pub fn spawn_recurring_background_check(&self, interval: Duration) {
        self.configure_scheduler(Some(interval));
    }

    #[cfg(test)]
    pub(super) fn spawn_recurring_background_check_for_test(
        &self,
        interval: Duration,
        max_runs: usize,
    ) {
        let worker = self.clone_for_background_worker();
        self.scheduler
            .configure_for_test_with_limit(worker, interval, max_runs);
    }

    /// Configure the single recurring scheduler. `None` cancels its next
    /// scheduled network check; enabling from `None` keeps immediate-first.
    pub fn configure_scheduler(&self, interval: Option<Duration>) {
        let worker = self.clone_for_background_worker();
        self.scheduler.configure(worker, interval);
    }

    #[cfg(test)]
    pub(super) fn configure_scheduler_for_test(&self, interval: Option<Duration>) {
        let worker = self.clone_for_background_worker();
        self.scheduler.configure_for_test(worker, interval);
    }

    fn clone_for_background_worker(&self) -> Self {
        Self {
            event_tx: self.event_tx.clone(),
            skipped_version: self.skipped_version.clone(),
            manifest_source: self.manifest_source.clone(),
            minisign_public_key: self.minisign_public_key.clone(),
            pending_update: Arc::clone(&self.pending_update),
            staged_artifact: Arc::clone(&self.staged_artifact),
            operation: Arc::clone(&self.operation),
            scheduler: Arc::clone(&self.scheduler),
        }
    }

    fn spawn_check_worker(&self) {
        let worker = self.clone_for_background_worker();
        let result = std::thread::Builder::new()
            .name("bentodesk-updater-check".to_owned())
            .spawn(move || worker.run_background_check_once())
            .map(|_| ());
        self.finish_worker_spawn(UpdateOperation::Checking, "check", result);
    }

    fn spawn_download_worker(&self) {
        let worker = self.clone_for_background_worker();
        let result = std::thread::Builder::new()
            .name("bentodesk-updater-download".to_owned())
            .spawn(move || worker.run_background_download_once())
            .map(|_| ());
        self.finish_worker_spawn(UpdateOperation::Downloading, "download", result);
    }

    fn finish_worker_spawn(
        &self,
        owned: UpdateOperation,
        kind: &'static str,
        result: std::io::Result<()>,
    ) {
        let Err(error) = result else {
            return;
        };
        let queued = self
            .event_tx
            .send(UpdateEvent::Error {
                kind: SmolStr::new_static(kind),
                message: format!("background updater thread spawn failed: {error}"),
            })
            .is_ok();
        if !queued {
            self.release_operation(owned, UpdateOperation::Idle);
        }
    }

    #[cfg(test)]
    pub(super) fn finish_worker_spawn_for_test(&self, owned: UpdateOperation, kind: &'static str) {
        self.finish_worker_spawn(
            owned,
            kind,
            Err(std::io::Error::other("test worker spawn failure")),
        );
    }

    fn run_background_check_once(&self) {
        let event = match self.check() {
            Ok(Some(info)) => UpdateEvent::Available { info },
            Ok(None) => UpdateEvent::UpToDate {
                current_version: pkg_version(),
            },
            Err(error) => UpdateEvent::Error {
                kind: SmolStr::new_static("check"),
                message: error.to_string(),
            },
        };
        if self.event_tx.send(event).is_err() {
            self.release_operation(UpdateOperation::Checking, UpdateOperation::Idle);
        }
    }

    fn run_background_download_once(&self) {
        let published = self.download_and_publish();
        if !published.terminal_queued {
            self.release_operation(UpdateOperation::Downloading, UpdateOperation::Idle);
        }
        if let Err(error) = published.result {
            tracing::warn!(error = %error, "background updater download failed");
        }
    }

    fn acquire_operation(&self, desired: UpdateOperation) -> UpdateStart {
        loop {
            let current = self.operation_state();
            if current == desired {
                return UpdateStart::Joined(current);
            }
            if current != UpdateOperation::Idle {
                return UpdateStart::Rejected(current);
            }
            if self
                .operation
                .compare_exchange(
                    UpdateOperation::Idle as u8,
                    desired as u8,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                return UpdateStart::Started;
            }
        }
    }

    /// Advance the gate only when the UI pump consumes a terminal event.
    pub fn acknowledge_event(&self, event: &UpdateEvent) {
        match event {
            UpdateEvent::Available { .. } | UpdateEvent::UpToDate { .. } => {
                self.release_operation(UpdateOperation::Checking, UpdateOperation::Idle);
            }
            UpdateEvent::Ready { .. } => {
                self.release_operation(UpdateOperation::Downloading, UpdateOperation::Ready);
            }
            UpdateEvent::Error { kind, .. } if kind == "check" => {
                self.release_operation(UpdateOperation::Checking, UpdateOperation::Idle);
            }
            UpdateEvent::Error { kind, .. } if kind == "download" || kind == "verify" => {
                self.release_operation(UpdateOperation::Downloading, UpdateOperation::Idle);
            }
            UpdateEvent::Progress { .. }
            | UpdateEvent::Installing { .. }
            | UpdateEvent::Error { .. } => {}
        }
    }

    pub(super) fn release_operation(&self, from: UpdateOperation, to: UpdateOperation) {
        let _ = self.operation.compare_exchange(
            from as u8,
            to as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }
}
