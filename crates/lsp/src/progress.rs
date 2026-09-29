//! Server-initiated work-done progress for long-running workspace operations.
//!
//! The analysis pipeline runs on blocking workers, so this module deliberately exposes
//! synchronous methods. The methods only update a small shared state machine and enqueue LSP
//! messages; creating the progress token and waiting for the client response happen in a Tokio
//! task.

use async_lsp::ClientSocket;
use lsp_types::{
    NumberOrString, ProgressParams, ProgressParamsValue, WorkDoneProgress, WorkDoneProgressBegin,
    WorkDoneProgressCreateParams, WorkDoneProgressEnd, WorkDoneProgressReport,
    notification as notif, request as req,
};
use solar_interface::data_structures::sync::Mutex;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::time::sleep;

const PROGRESS_TITLE: &str = "Indexing workspace";
const RESTART_MESSAGE: &str = "Workspace changed, restarting analysis";

#[derive(Clone, Copy)]
struct Timing {
    delay: Duration,
    create_timeout: Duration,
}

/// Coordinates one continuous progress wave across successive analysis versions.
#[derive(Clone)]
pub(crate) struct ProgressCoordinator {
    inner: Arc<CoordinatorInner>,
}

struct CoordinatorInner {
    client: ClientSocket,
    enabled: Arc<AtomicBool>,
    timing: Timing,
    active: Mutex<Option<Arc<WorkDoneProgressGuard>>>,
}

impl ProgressCoordinator {
    /// Builds a coordinator with explicit timing values.
    pub(crate) fn with_timing(
        client: ClientSocket,
        enabled: bool,
        delay: Duration,
        create_timeout: Duration,
    ) -> Self {
        Self {
            inner: Arc::new(CoordinatorInner {
                client,
                enabled: Arc::new(AtomicBool::new(enabled)),
                timing: Timing { delay, create_timeout },
                active: Mutex::new(None),
            }),
        }
    }

    /// Updates the negotiated client capability.
    pub(crate) fn set_enabled(&self, enabled: bool) {
        self.inner.enabled.store(enabled, Ordering::Release);
    }

    /// Closes the active progress wave when the client cancels its token.
    pub(crate) fn cancel(&self, token: &NumberOrString) {
        let active = self.inner.active.lock();
        if let Some(guard) = active.as_ref() {
            guard.cancel(token);
        }
    }

    /// Starts or joins the progress wave for `version`.
    ///
    /// A newer version replaces an invisible wave so its progress clock starts over. Once progress
    /// is visible, the newer version reuses that wave and reports at most one restart. Tickets for
    /// older versions remain valid handles but cannot report or finish the newer wave.
    #[cfg(test)]
    pub(crate) fn start(&self, version: usize) -> ProgressTicket {
        let ticket = self.reserve(version);
        ticket.begin();
        ticket
    }

    /// Reserves a progress wave that starts when its ticket is begun.
    pub(crate) fn reserve(&self, version: usize) -> ProgressTicket {
        if !self.inner.enabled.load(Ordering::Acquire) {
            return ProgressTicket::disabled(version);
        }

        let mut active = self.inner.active.lock();
        if !self.inner.enabled.load(Ordering::Acquire) {
            return ProgressTicket::disabled(version);
        }
        let restarted = active.as_ref().filter(|guard| guard.restart(version)).cloned();
        // `restart` may have waited for a failing guard that disabled the connection.
        if !self.inner.enabled.load(Ordering::Acquire) {
            return ProgressTicket::disabled(version);
        }

        let guard = restarted.unwrap_or_else(|| {
            let guard = Arc::new(WorkDoneProgressGuard::new(
                self.inner.client.clone(),
                self.inner.enabled.clone(),
                version,
                self.inner.timing,
            ));
            *active = Some(Arc::clone(&guard));
            guard
        });
        ProgressTicket { guard: Some(guard), version }
    }

    /// Runs `publish` while blocking a pending create response, then finishes the active wave.
    pub(crate) fn finish_active_after<T>(
        &self,
        message: &'static str,
        publish: impl FnOnce() -> T,
    ) -> T {
        let active = self.inner.active.lock();
        let Some(guard) = active.as_ref() else {
            drop(active);
            return publish();
        };
        guard.finish_active_after(message, publish)
    }

    #[cfg(test)]
    fn is_active_for_test(&self, version: usize) -> bool {
        self.inner.active.lock().as_ref().is_some_and(|guard| guard.is_current_and_open(version))
    }
}

/// A handle tied to one analysis version.
///
/// The handle is intentionally cheap to clone so the worker and its completion monitor can both
/// attempt to report a terminal state. The guard's version check and idempotent state transition
/// ensure that only the latest worker can close the wave.
#[derive(Clone)]
pub(crate) struct ProgressTicket {
    guard: Option<Arc<WorkDoneProgressGuard>>,
    version: usize,
}

impl ProgressTicket {
    fn disabled(version: usize) -> Self {
        Self { guard: None, version }
    }

    pub(crate) fn is_disabled(&self) -> bool {
        self.guard.is_none()
    }

    pub(crate) fn begin(&self) {
        if let Some(guard) = &self.guard {
            guard.schedule(self.version);
        }
    }

    pub(crate) fn report(&self, message: &'static str) {
        if let Some(guard) = &self.guard {
            guard.report(self.version, message);
        }
    }

    pub(crate) fn finish(&self, message: &'static str) {
        if let Some(guard) = &self.guard {
            guard.finish(self.version, message);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Pending,
    Delayed,
    Creating,
    Begun,
    Closed,
}

struct ProgressState {
    version: usize,
    phase: Phase,
    message: Option<&'static str>,
    terminal: Option<&'static str>,
    restart_reported: bool,
    create_timed_out: bool,
}

/// Owns one server-created progress token and serializes all wire-visible transitions.
///
/// Keeping notification enqueueing under the state lock is intentional: `ClientSocket::notify`
/// is nonblocking, and this prevents a terminal `end` from overtaking a `begin` when the create
/// response and a worker completion happen on different executor turns.
struct WorkDoneProgressGuard {
    client: ClientSocket,
    enabled: Arc<AtomicBool>,
    token: NumberOrString,
    timing: Timing,
    state: Mutex<ProgressState>,
}

impl ProgressState {
    fn close(&mut self) {
        self.phase = Phase::Closed;
        self.message = None;
        self.terminal = None;
        self.restart_reported = false;
    }
}

impl WorkDoneProgressGuard {
    fn new(client: ClientSocket, enabled: Arc<AtomicBool>, version: usize, timing: Timing) -> Self {
        Self {
            client,
            enabled,
            token: NumberOrString::String(format!("solar/workspace-index/{version}")),
            timing,
            state: Mutex::new(ProgressState {
                version,
                phase: Phase::Pending,
                message: None,
                terminal: None,
                restart_reported: false,
                create_timed_out: false,
            }),
        }
    }

    fn schedule(self: &Arc<Self>, version: usize) {
        {
            let mut state = self.state.lock();
            if !self.enabled.load(Ordering::Acquire)
                || state.version != version
                || state.phase != Phase::Pending
            {
                return;
            }
            state.phase = Phase::Delayed;
        }

        let weak = Arc::downgrade(self);
        let delay = self.timing.delay;
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            self.disable("no Tokio runtime available");
            return;
        };

        handle.spawn(async move {
            sleep(delay).await;
            let Some(guard) = weak.upgrade() else { return };
            if !guard.mark_creating() {
                return;
            }

            let client = guard.client.clone();
            let token = guard.token.clone();
            let create_timeout = guard.timing.create_timeout;
            let request = client
                .request::<req::WorkDoneProgressCreate>(WorkDoneProgressCreateParams { token });
            tokio::pin!(request);
            let result = tokio::select! {
                result = &mut request => result,
                _ = sleep(create_timeout) => {
                    guard.observe_create_timeout();
                    request.await
                }
            };

            match result {
                Ok(()) => guard.created(),
                Err(error) => guard.disable(&format!("client could not create progress: {error}")),
            }
        });
    }

    fn mark_creating(&self) -> bool {
        let mut state = self.state.lock();
        if !self.enabled.load(Ordering::Acquire) || state.phase != Phase::Delayed {
            return false;
        }
        state.phase = Phase::Creating;
        true
    }

    fn observe_create_timeout(&self) {
        let mut state = self.state.lock();
        if state.phase != Phase::Creating || state.create_timed_out {
            return;
        }

        state.create_timed_out = true;
        tracing::debug!(
            token = ?self.token,
            timeout = ?self.timing.create_timeout,
            "work-done progress create response is slow"
        );
    }

    fn cancel(&self, token: &NumberOrString) {
        let mut state = self.state.lock();
        if &self.token != token || state.phase == Phase::Closed {
            return;
        }

        let was_begun = state.phase == Phase::Begun;
        state.close();
        if was_begun {
            self.end(None);
        }
    }

    fn restart(&self, version: usize) -> bool {
        let mut state = self.state.lock();
        match state.phase {
            Phase::Closed => return false,
            _ if version <= state.version => return true,
            Phase::Pending | Phase::Delayed | Phase::Creating => {
                state.close();
                return false;
            }
            Phase::Begun => {}
        }

        state.version = version;
        state.terminal = None;
        if !state.restart_reported {
            if self.send_report(RESTART_MESSAGE) {
                state.restart_reported = true;
            } else {
                self.disable_locked(&mut state, "failed to enqueue replacement report");
            }
        }
        true
    }

    fn report(&self, version: usize, message: &'static str) {
        let mut state = self.state.lock();
        if state.version != version || state.terminal.is_some() {
            return;
        }

        match state.phase {
            Phase::Begun => {
                if !self.send_report(message) {
                    self.disable_locked(&mut state, "failed to enqueue progress report");
                }
            }
            Phase::Closed => {}
            Phase::Pending | Phase::Delayed | Phase::Creating => state.message = Some(message),
        }
    }

    fn finish(&self, version: usize, message: &'static str) {
        let mut state = self.state.lock();
        if state.version == version {
            self.finish_locked(&mut state, message, false);
        }
    }

    fn finish_active_after<T>(&self, message: &'static str, publish: impl FnOnce() -> T) -> T {
        let mut state = self.state.lock();
        if state.phase == Phase::Closed {
            drop(state);
            return publish();
        }

        let result = publish();
        self.finish_locked(&mut state, message, true);
        result
    }

    /// Closes the wave, or defers `message` until a pending create response arrives.
    fn finish_locked(&self, state: &mut ProgressState, message: &'static str, replace: bool) {
        match state.phase {
            Phase::Pending | Phase::Delayed => state.close(),
            Phase::Creating => {
                if replace || state.terminal.is_none() {
                    state.message = Some(message);
                    state.terminal = Some(message);
                }
            }
            Phase::Begun => {
                state.close();
                self.end(Some(message));
            }
            Phase::Closed => {}
        }
    }

    fn created(&self) {
        let mut state = self.state.lock();
        if state.phase != Phase::Creating {
            return;
        }
        if state.terminal.is_some() {
            state.close();
            return;
        }

        let begin = WorkDoneProgress::Begin(WorkDoneProgressBegin {
            title: PROGRESS_TITLE.into(),
            cancellable: Some(false),
            message: state.message.take().map(str::to_owned),
            percentage: None,
        });
        if send_progress(&self.client, &self.token, begin) {
            state.phase = Phase::Begun;
        } else {
            self.disable_locked(&mut state, "failed to enqueue progress begin");
        }
    }

    fn disable(&self, reason: &str) {
        let mut state = self.state.lock();
        if matches!(state.phase, Phase::Begun | Phase::Closed) {
            return;
        }
        self.disable_locked(&mut state, reason);
    }

    fn disable_locked(&self, state: &mut ProgressState, reason: &str) {
        tracing::debug!(token = ?self.token, %reason, "work-done progress unavailable");
        self.enabled.store(false, Ordering::Release);
        state.close();
    }

    fn send_report(&self, message: &str) -> bool {
        let report = WorkDoneProgressReport {
            cancellable: Some(false),
            message: Some(message.into()),
            percentage: None,
        };
        send_progress(&self.client, &self.token, WorkDoneProgress::Report(report))
    }

    /// Ends a begun wave, disabling progress when the client connection is gone.
    fn end(&self, message: Option<&'static str>) {
        let end = WorkDoneProgressEnd { message: message.map(str::to_owned) };
        if !send_progress(&self.client, &self.token, WorkDoneProgress::End(end)) {
            self.enabled.store(false, Ordering::Release);
        }
    }

    #[cfg(test)]
    fn is_current_and_open(&self, version: usize) -> bool {
        let state = self.state.lock();
        state.version == version && state.phase != Phase::Closed
    }

    #[cfg(test)]
    fn create_timed_out_for_test(&self) -> bool {
        self.state.lock().create_timed_out
    }
}

impl Drop for WorkDoneProgressGuard {
    fn drop(&mut self) {
        let mut state = self.state.lock();
        if state.phase == Phase::Begun {
            state.close();
            self.end(None);
        }
    }
}

pub(crate) fn send_progress(
    client: &ClientSocket,
    token: &NumberOrString,
    value: WorkDoneProgress,
) -> bool {
    client
        .notify::<notif::Progress>(ProgressParams {
            token: token.clone(),
            value: ProgressParamsValue::WorkDone(value),
        })
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ClientHarness;
    use std::sync::mpsc as std_mpsc;

    const TIMEOUT: Duration = Duration::from_secs(1);

    fn coordinator(
        harness: &ClientHarness,
        delay: Duration,
        create_timeout: Duration,
    ) -> ProgressCoordinator {
        ProgressCoordinator::with_timing(harness.client().clone(), true, delay, create_timeout)
    }

    /// Starts `version` and waits until the client observes its `begin`.
    async fn start_visible(
        harness: &mut ClientHarness,
        coordinator: &ProgressCoordinator,
        version: usize,
    ) -> (ProgressTicket, NumberOrString) {
        let ticket = coordinator.start(version);
        let token = harness.expect_create().await;
        harness.acknowledge_create();
        assert_eq!(harness.expect_progress(&token).await, begin(None));
        (ticket, token)
    }

    fn closed_coordinator(enabled: bool, delay: Duration) -> ProgressCoordinator {
        ProgressCoordinator::with_timing(ClientSocket::new_closed(), enabled, delay, TIMEOUT)
    }

    fn guard(enabled: bool) -> WorkDoneProgressGuard {
        WorkDoneProgressGuard::new(
            ClientSocket::new_closed(),
            Arc::new(AtomicBool::new(enabled)),
            1,
            Timing { delay: Duration::ZERO, create_timeout: TIMEOUT },
        )
    }

    fn creating_guard() -> WorkDoneProgressGuard {
        let guard = guard(true);
        guard.state.lock().phase = Phase::Delayed;
        assert!(guard.mark_creating());
        guard
    }

    fn begin(message: Option<&str>) -> WorkDoneProgress {
        WorkDoneProgress::Begin(WorkDoneProgressBegin {
            title: PROGRESS_TITLE.into(),
            cancellable: Some(false),
            message: message.map(Into::into),
            percentage: None,
        })
    }

    fn report(message: &str) -> WorkDoneProgress {
        WorkDoneProgress::Report(WorkDoneProgressReport {
            cancellable: Some(false),
            message: Some(message.into()),
            percentage: None,
        })
    }

    fn end(message: Option<&str>) -> WorkDoneProgress {
        WorkDoneProgress::End(WorkDoneProgressEnd { message: message.map(Into::into) })
    }

    #[test]
    fn guard_terminal_transitions() {
        let pending = guard(true);
        pending.report(1, "pending");
        pending.finish(1, "finished");
        {
            let state = pending.state.lock();
            assert_eq!(state.phase, Phase::Closed);
            assert!(state.message.is_none());
        }
        pending.finish_active_after("ignored", || assert!(pending.state.try_lock().is_some()));

        let creating = creating_guard();
        creating.finish(1, "first");
        creating.report(1, "ignored");
        creating.finish(1, "second");
        assert_eq!(creating.state.lock().terminal, Some("first"));
    }

    #[test]
    fn disabled_progress_is_a_noop() {
        let coordinator = closed_coordinator(false, Duration::ZERO);
        let ticket = coordinator.start(1);
        assert!(ticket.is_disabled());
        ticket.report("ignored");
        ticket.finish("ignored");
        assert!(!coordinator.is_active_for_test(1));

        let guard = guard(false);
        guard.state.lock().phase = Phase::Delayed;
        assert!(!guard.mark_creating());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stale_tickets_and_tokens_cannot_close_newer_work() {
        let coordinator = closed_coordinator(true, Duration::from_secs(60));
        let closed = coordinator.start(1);
        closed.finish("closed");
        let stale = coordinator.start(2);
        let current = coordinator.start(3);
        let token = |ticket: &ProgressTicket| ticket.guard.as_ref().unwrap().token.clone();
        assert_ne!(token(&stale), token(&current));

        stale.finish("stale");
        coordinator.cancel(&NumberOrString::String("unknown".into()));
        coordinator.cancel(&token(&closed));
        coordinator.cancel(&token(&stale));
        assert!(coordinator.is_active_for_test(3));
        current.finish("done");
        assert!(!coordinator.is_active_for_test(3));
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn finishing_just_before_delay_never_creates_progress() {
        let delay = Duration::from_millis(100);
        let coordinator = closed_coordinator(true, delay);
        let ticket = coordinator.start(1);
        tokio::task::yield_now().await;

        tokio::time::advance(delay - Duration::from_millis(1)).await;
        ticket.finish("done");
        tokio::time::advance(Duration::from_millis(1)).await;
        tokio::task::yield_now().await;

        assert!(!coordinator.is_active_for_test(1));
        assert!(coordinator.inner.enabled.load(Ordering::Acquire));
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn replacement_before_delay_restarts_the_progress_clock() {
        let delay = Duration::from_millis(100);
        let coordinator = closed_coordinator(true, delay);
        let first = coordinator.start(1);
        tokio::task::yield_now().await;

        tokio::time::advance(delay / 2).await;
        let latest = coordinator.start(2);
        assert!(!Arc::ptr_eq(first.guard.as_ref().unwrap(), latest.guard.as_ref().unwrap()));
        tokio::task::yield_now().await;

        tokio::time::advance(delay / 2).await;
        tokio::task::yield_now().await;
        assert!(coordinator.is_active_for_test(2));
        assert!(coordinator.inner.enabled.load(Ordering::Acquire));

        latest.finish("done");
        tokio::time::advance(delay / 2).await;
        tokio::task::yield_now().await;
        assert!(!coordinator.is_active_for_test(2));
        assert!(coordinator.inner.enabled.load(Ordering::Acquire));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn create_failure_disables_progress_for_the_connection() {
        let coordinator = closed_coordinator(true, Duration::ZERO);
        coordinator.start(1);
        tokio::time::timeout(TIMEOUT, async {
            while coordinator.inner.enabled.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("create failure should disable progress");

        assert!(coordinator.start(2).guard.is_none());
    }

    #[test]
    fn create_response_cannot_interleave_with_a_publication() {
        let guard = Arc::new(creating_guard());
        guard.finish(1, "obsolete completion");

        let published = Arc::new(AtomicBool::new(false));
        let (publish_started_tx, publish_started_rx) = std_mpsc::channel();
        let (release_publish_tx, release_publish_rx) = std_mpsc::channel();
        let publish_guard = guard.clone();
        let publish_complete = published.clone();
        let publish_task = std::thread::spawn(move || {
            publish_guard.finish_active_after("publication complete", || {
                publish_started_tx.send(()).unwrap();
                release_publish_rx.recv().unwrap();
                publish_complete.store(true, Ordering::Release);
            });
        });
        publish_started_rx
            .recv_timeout(TIMEOUT)
            .expect("publication should start while the progress state is locked");

        let (create_started_tx, create_started_rx) = std_mpsc::channel();
        let (create_done_tx, create_done_rx) = std_mpsc::channel();
        let create_task = std::thread::spawn(move || {
            create_started_tx.send(()).unwrap();
            guard.created();
            assert!(published.load(Ordering::Acquire));
            create_done_tx.send(()).unwrap();
        });
        create_started_rx.recv_timeout(TIMEOUT).unwrap();
        assert!(matches!(
            create_done_rx.recv_timeout(Duration::from_millis(25)),
            Err(std_mpsc::RecvTimeoutError::Timeout)
        ));

        release_publish_tx.send(()).unwrap();
        publish_task.join().unwrap();
        create_done_rx
            .recv_timeout(TIMEOUT)
            .expect("create response should resume after publication");
        create_task.join().unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reserved_wave_emits_create_begin_report_end_once_begun() {
        let mut harness = ClientHarness::new();
        let coordinator = coordinator(&harness, Duration::ZERO, TIMEOUT);
        let ticket = coordinator.reserve(7);
        harness.assert_silent().await;

        ticket.begin();
        let token = harness.expect_create().await;
        ticket.report("reading sources");
        harness.acknowledge_create();
        assert_eq!(harness.expect_progress(&token).await, begin(Some("reading sources")));
        ticket.report("analyzing");
        assert_eq!(harness.expect_progress(&token).await, report("analyzing"));
        ticket.finish("done");
        assert_eq!(harness.expect_progress(&token).await, end(Some("done")));

        harness.exit().await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancellation_before_delay_suppresses_creation() {
        let mut harness = ClientHarness::new();
        let coordinator = coordinator(&harness, Duration::from_millis(10), TIMEOUT);
        let ticket = coordinator.start(1);

        coordinator.cancel(&ticket.guard.as_ref().unwrap().token);
        sleep(Duration::from_millis(25)).await;
        harness.assert_silent().await;

        assert!(!coordinator.is_active_for_test(1));
        harness.exit().await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancellation_while_create_is_pending_suppresses_late_begin() {
        let mut harness = ClientHarness::new();
        let coordinator = coordinator(&harness, Duration::ZERO, TIMEOUT);
        let _ticket = coordinator.start(1);
        let token = harness.expect_create().await;

        coordinator.cancel(&token);
        harness.acknowledge_create();
        harness.assert_silent().await;

        assert!(!coordinator.is_active_for_test(1));
        harness.exit().await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancellation_after_begin_sends_one_end_and_suppresses_reports() {
        let mut harness = ClientHarness::new();
        let coordinator = coordinator(&harness, Duration::ZERO, TIMEOUT);
        let (ticket, token) = start_visible(&mut harness, &coordinator, 1).await;

        coordinator.cancel(&token);
        assert_eq!(harness.expect_progress(&token).await, end(None));

        ticket.report("late report");
        ticket.finish("late finish");
        harness.assert_silent().await;
        assert!(!coordinator.is_active_for_test(1));
        harness.exit().await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn replacement_while_create_is_pending_finishes_silently() {
        let mut harness = ClientHarness::new();
        let coordinator = coordinator(&harness, Duration::ZERO, Duration::from_millis(10));
        let first = coordinator.start(1);
        harness.expect_create().await;
        let first_guard = first.guard.as_ref().unwrap();
        tokio::time::timeout(TIMEOUT, async {
            while !first_guard.create_timed_out_for_test() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("create timeout should be observed");
        first.finish("first finished");

        let second = coordinator.start(2);
        assert!(!Arc::ptr_eq(first_guard, second.guard.as_ref().unwrap()));
        second.finish("second finished");
        harness.acknowledge_create();
        harness.assert_silent().await;

        assert!(!coordinator.is_active_for_test(2));
        assert!(coordinator.inner.enabled.load(Ordering::Acquire));
        harness.exit().await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn replacements_after_begin_reuse_the_visible_wave_and_report_one_restart() {
        let mut harness = ClientHarness::new();
        let coordinator = coordinator(&harness, Duration::ZERO, TIMEOUT);
        let (first, token) = start_visible(&mut harness, &coordinator, 1).await;

        let second = coordinator.start(2);
        assert!(Arc::ptr_eq(first.guard.as_ref().unwrap(), second.guard.as_ref().unwrap()));
        assert_eq!(harness.expect_progress(&token).await, report(RESTART_MESSAGE));

        let _third = coordinator.start(3);
        let latest = coordinator.start(4);
        harness.assert_silent().await;

        first.finish("stale");
        latest.finish("done");
        assert_eq!(harness.expect_progress(&token).await, end(Some("done")));
        harness.exit().await;
    }
}
