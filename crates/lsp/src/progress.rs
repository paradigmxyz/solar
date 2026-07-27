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
const PROGRESS_DELAY: Duration = Duration::from_millis(250);
const CREATE_TIMEOUT: Duration = Duration::from_secs(1);
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
    pub(crate) fn new(client: ClientSocket, enabled: bool) -> Self {
        Self::with_timing(client, enabled, PROGRESS_DELAY, CREATE_TIMEOUT)
    }

    /// Builds a coordinator with explicit timing values for deterministic tests.
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
    /// A newer version reuses a visible wave and reports at most one restart. If the previous
    /// wave has already ended or failed, a fresh token is allocated. Tickets for older versions
    /// remain valid handles but cannot report or finish the newer wave.
    pub(crate) fn start(&self, version: usize) -> ProgressTicket {
        if !self.inner.enabled.load(Ordering::Acquire) {
            return ProgressTicket::disabled(version);
        }

        let (guard, schedule) = {
            let mut active = self.inner.active.lock();
            if !self.inner.enabled.load(Ordering::Acquire) {
                return ProgressTicket::disabled(version);
            }

            if let Some(guard) = active.as_ref()
                && guard.restart(version)
            {
                if !self.inner.enabled.load(Ordering::Acquire) {
                    return ProgressTicket::disabled(version);
                }
                (Arc::clone(guard), false)
            } else {
                // `restart` may have waited for a failing guard that disabled the connection.
                if !self.inner.enabled.load(Ordering::Acquire) {
                    return ProgressTicket::disabled(version);
                }

                let guard = Arc::new(WorkDoneProgressGuard::new(
                    self.inner.client.clone(),
                    self.inner.enabled.clone(),
                    version,
                    self.inner.timing,
                ));
                *active = Some(Arc::clone(&guard));
                (guard, true)
            }
        };

        if schedule {
            guard.schedule();
        }

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

impl WorkDoneProgressGuard {
    fn new(client: ClientSocket, enabled: Arc<AtomicBool>, version: usize, timing: Timing) -> Self {
        Self {
            client,
            enabled,
            token: NumberOrString::String(format!("solar/workspace-index/{version}")),
            timing,
            state: Mutex::new(ProgressState {
                version,
                phase: Phase::Delayed,
                message: None,
                terminal: None,
                restart_reported: false,
                create_timed_out: false,
            }),
        }
    }

    fn schedule(self: &Arc<Self>) {
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
        state.phase = Phase::Closed;
        state.message = None;
        state.terminal = None;
        state.restart_reported = false;

        if was_begun
            && !send_progress(
                &self.client,
                &self.token,
                WorkDoneProgress::End(WorkDoneProgressEnd { message: None }),
            )
        {
            self.enabled.store(false, Ordering::Release);
        }
    }

    fn restart(&self, version: usize) -> bool {
        let mut state = self.state.lock();
        if state.phase == Phase::Closed {
            return false;
        }
        if version <= state.version {
            return true;
        }

        state.version = version;
        state.terminal = None;
        if state.phase == Phase::Begun {
            if state.restart_reported {
                return true;
            }
            if !send_progress(
                &self.client,
                &self.token,
                WorkDoneProgress::Report(WorkDoneProgressReport {
                    cancellable: Some(false),
                    message: Some(RESTART_MESSAGE.into()),
                    percentage: None,
                }),
            ) {
                self.disable_locked(&mut state, "failed to enqueue replacement report");
            } else {
                state.restart_reported = true;
            }
        } else if state.message != Some(RESTART_MESSAGE) {
            state.message = Some(RESTART_MESSAGE);
        }
        true
    }

    fn report(&self, version: usize, message: &'static str) {
        let mut state = self.state.lock();
        if state.version != version || state.terminal.is_some() {
            return;
        }

        if state.phase == Phase::Begun {
            if !send_progress(
                &self.client,
                &self.token,
                WorkDoneProgress::Report(WorkDoneProgressReport {
                    cancellable: Some(false),
                    message: Some(message.into()),
                    percentage: None,
                }),
            ) {
                self.disable_locked(&mut state, "failed to enqueue progress report");
            }
        } else if matches!(state.phase, Phase::Delayed | Phase::Creating) {
            state.message = Some(message);
        }
    }

    fn finish(&self, version: usize, message: &'static str) {
        let mut state = self.state.lock();
        if state.version != version || state.phase == Phase::Closed {
            return;
        }

        match state.phase {
            Phase::Delayed => {
                state.phase = Phase::Closed;
                state.message = None;
            }
            Phase::Creating => {
                if state.terminal.is_none() {
                    state.message = Some(message);
                    state.terminal = Some(message);
                }
            }
            Phase::Begun => {
                state.phase = Phase::Closed;
                if !send_progress(
                    &self.client,
                    &self.token,
                    WorkDoneProgress::End(WorkDoneProgressEnd { message: Some(message.into()) }),
                ) {
                    self.enabled.store(false, Ordering::Release);
                }
            }
            Phase::Closed => {}
        }
    }

    fn finish_active_after<T>(&self, message: &'static str, publish: impl FnOnce() -> T) -> T {
        let mut state = self.state.lock();
        if state.phase == Phase::Closed {
            drop(state);
            return publish();
        }

        let result = publish();
        match state.phase {
            Phase::Delayed => {
                state.phase = Phase::Closed;
                state.message = None;
            }
            Phase::Creating => {
                state.message = Some(message);
                state.terminal = Some(message);
            }
            Phase::Begun => {
                state.phase = Phase::Closed;
                if !send_progress(
                    &self.client,
                    &self.token,
                    WorkDoneProgress::End(WorkDoneProgressEnd { message: Some(message.into()) }),
                ) {
                    self.enabled.store(false, Ordering::Release);
                }
            }
            Phase::Closed => {}
        }
        result
    }

    fn created(&self) {
        let mut state = self.state.lock();
        if state.phase != Phase::Creating {
            return;
        }

        state.phase = Phase::Begun;
        let message = state.message.take();
        let is_restart = message == Some(RESTART_MESSAGE);
        let begin = WorkDoneProgress::Begin(WorkDoneProgressBegin {
            title: PROGRESS_TITLE.into(),
            cancellable: Some(false),
            message: message.map(str::to_owned),
            percentage: None,
        });
        if !send_progress(&self.client, &self.token, begin) {
            self.disable_locked(&mut state, "failed to enqueue progress begin");
            return;
        }
        if is_restart {
            state.restart_reported = true;
        }

        if let Some(message) = state.terminal.take() {
            state.phase = Phase::Closed;
            if !send_progress(
                &self.client,
                &self.token,
                WorkDoneProgress::End(WorkDoneProgressEnd { message: Some(message.into()) }),
            ) {
                self.enabled.store(false, Ordering::Release);
            }
        }
    }

    fn disable(&self, reason: &str) {
        let mut state = self.state.lock();
        if !matches!(state.phase, Phase::Delayed | Phase::Creating) {
            return;
        }
        self.disable_locked(&mut state, reason);
    }

    fn disable_locked(&self, state: &mut ProgressState, reason: &str) {
        tracing::debug!(token = ?self.token, %reason, "work-done progress unavailable");
        self.enabled.store(false, Ordering::Release);
        state.phase = Phase::Closed;
        state.message = None;
        state.terminal = None;
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
        if state.phase != Phase::Begun {
            return;
        }
        state.phase = Phase::Closed;
        if !send_progress(
            &self.client,
            &self.token,
            WorkDoneProgress::End(WorkDoneProgressEnd { message: None }),
        ) {
            self.enabled.store(false, Ordering::Release);
        }
    }
}

fn send_progress(client: &ClientSocket, token: &NumberOrString, value: WorkDoneProgress) -> bool {
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
    use async_lsp::{ClientSocket, ErrorCode, ResponseError, ServerSocket, router::Router};
    use std::{ops::ControlFlow, sync::mpsc as std_mpsc};
    use tokio::sync::{mpsc, oneshot};
    use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

    #[derive(Debug)]
    enum ClientEvent {
        Create(WorkDoneProgressCreateParams),
        Progress(ProgressParams),
    }

    struct MockClient {
        events: mpsc::UnboundedSender<ClientEvent>,
        create_ack: Option<oneshot::Receiver<()>>,
    }

    struct ProgressHarness {
        client: ClientSocket,
        server: ServerSocket,
        events: mpsc::UnboundedReceiver<ClientEvent>,
        create_ack: Option<oneshot::Sender<()>>,
        server_task: tokio::task::JoinHandle<async_lsp::Result<()>>,
        client_task: tokio::task::JoinHandle<async_lsp::Result<()>>,
    }

    impl ProgressHarness {
        fn acknowledge_create(&mut self) {
            self.create_ack.take().expect("one create acknowledgement").send(()).unwrap();
        }

        async fn probe(&self) {
            self.client.request::<req::Shutdown>(()).await.unwrap();
        }

        async fn shutdown(self) {
            self.server.notify::<notif::Exit>(()).unwrap();
            assert!(self.server_task.await.unwrap().is_ok());
            assert!(matches!(self.client_task.await.unwrap(), Err(async_lsp::Error::Eof)));
        }
    }

    fn progress_harness() -> ProgressHarness {
        let (server_main, client) = async_lsp::MainLoop::new_server(|_| {
            let mut router = Router::new(());
            router.notification::<notif::Exit>(|_, ()| ControlFlow::Break(Ok(())));
            router
        });
        let (events_tx, events) = mpsc::unbounded_channel();
        let (create_ack_tx, create_ack_rx) = oneshot::channel();
        let (client_main, server) = async_lsp::MainLoop::new_client(move |_| {
            let mut router =
                Router::new(MockClient { events: events_tx, create_ack: Some(create_ack_rx) });
            router.request::<req::WorkDoneProgressCreate, _>(|state, params| {
                state.events.send(ClientEvent::Create(params)).unwrap();
                let create_ack = state.create_ack.take().expect("one progress create request");
                async move {
                    create_ack.await.map_err(|_| {
                        ResponseError::new(ErrorCode::REQUEST_FAILED, "test create ack dropped")
                    })?;
                    Ok(())
                }
            });
            router.request::<req::Shutdown, _>(|_, ()| async { Ok(()) });
            router.notification::<notif::Progress>(|state, params| {
                state.events.send(ClientEvent::Progress(params)).unwrap();
                ControlFlow::Continue(())
            });
            router
        });

        let (server_stream, client_stream) = tokio::io::duplex(64 << 10);
        let (server_rx, server_tx) = tokio::io::split(server_stream);
        let server_task =
            tokio::spawn(server_main.run_buffered(server_rx.compat(), server_tx.compat_write()));
        let (client_rx, client_tx) = tokio::io::split(client_stream);
        let client_task =
            tokio::spawn(client_main.run_buffered(client_rx.compat(), client_tx.compat_write()));

        ProgressHarness {
            client,
            server,
            events,
            create_ack: Some(create_ack_tx),
            server_task,
            client_task,
        }
    }

    async fn next_event(events: &mut mpsc::UnboundedReceiver<ClientEvent>) -> ClientEvent {
        tokio::time::timeout(Duration::from_secs(1), events.recv())
            .await
            .expect("client event should arrive")
            .expect("client event channel should stay open")
    }

    #[test]
    fn creating_ticket_keeps_first_terminal_message() {
        let guard = WorkDoneProgressGuard::new(
            ClientSocket::new_closed(),
            Arc::new(AtomicBool::new(true)),
            1,
            Timing { delay: Duration::ZERO, create_timeout: Duration::from_secs(1) },
        );
        assert!(guard.mark_creating());
        guard.finish(1, "first");
        guard.report(1, "ignored");
        guard.finish(1, "second");

        assert_eq!(guard.state.lock().terminal, Some("first"));
    }

    #[test]
    fn finishing_delayed_ticket_discards_pending_message() {
        let guard = WorkDoneProgressGuard::new(
            ClientSocket::new_closed(),
            Arc::new(AtomicBool::new(true)),
            1,
            Timing { delay: Duration::ZERO, create_timeout: Duration::from_secs(1) },
        );
        guard.report(1, "pending");

        guard.finish(1, "finished");

        let state = guard.state.lock();
        assert_eq!(state.phase, Phase::Closed);
        assert!(state.message.is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stale_ticket_cannot_finish_newer_work() {
        let coordinator = ProgressCoordinator::with_timing(
            ClientSocket::new_closed(),
            true,
            Duration::from_secs(60),
            Duration::from_secs(1),
        );
        let stale = coordinator.start(1);
        let current = coordinator.start(2);

        stale.finish("stale");

        assert!(coordinator.is_active_for_test(2));
        current.finish("done");
        assert!(!coordinator.is_active_for_test(2));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn finishing_before_delay_never_creates_progress() {
        let coordinator = ProgressCoordinator::with_timing(
            ClientSocket::new_closed(),
            true,
            Duration::ZERO,
            Duration::from_secs(1),
        );
        let ticket = coordinator.start(1);
        ticket.finish("done");
        tokio::task::yield_now().await;

        assert!(!coordinator.is_active_for_test(1));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancellation_before_delay_suppresses_creation() {
        let mut harness = progress_harness();
        let coordinator = ProgressCoordinator::with_timing(
            harness.client.clone(),
            true,
            Duration::from_millis(10),
            Duration::from_secs(1),
        );
        let ticket = coordinator.start(1);
        let token = ticket.guard.as_ref().unwrap().token.clone();

        coordinator.cancel(&token);
        sleep(Duration::from_millis(25)).await;
        harness.probe().await;

        assert!(!coordinator.is_active_for_test(1));
        assert!(matches!(harness.events.try_recv(), Err(mpsc::error::TryRecvError::Empty)));
        harness.shutdown().await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancellation_while_create_is_pending_suppresses_late_begin() {
        let mut harness = progress_harness();
        let coordinator = ProgressCoordinator::with_timing(
            harness.client.clone(),
            true,
            Duration::ZERO,
            Duration::from_secs(1),
        );
        let ticket = coordinator.start(1);
        let ClientEvent::Create(create) = next_event(&mut harness.events).await else {
            panic!("expected create request")
        };

        coordinator.cancel(&create.token);
        harness.acknowledge_create();
        harness.probe().await;

        assert!(!coordinator.is_active_for_test(1));
        assert!(matches!(harness.events.try_recv(), Err(mpsc::error::TryRecvError::Empty)));
        drop(ticket);
        harness.shutdown().await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancellation_after_begin_sends_one_end_and_suppresses_reports() {
        let mut harness = progress_harness();
        let coordinator = ProgressCoordinator::with_timing(
            harness.client.clone(),
            true,
            Duration::ZERO,
            Duration::from_secs(1),
        );
        let ticket = coordinator.start(1);
        let ClientEvent::Create(create) = next_event(&mut harness.events).await else {
            panic!("expected create request")
        };
        let token = create.token;
        harness.acknowledge_create();
        assert!(matches!(
            next_event(&mut harness.events).await,
            ClientEvent::Progress(ProgressParams {
                token: actual,
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(_)),
            }) if actual == token
        ));

        coordinator.cancel(&token);
        match next_event(&mut harness.events).await {
            ClientEvent::Progress(ProgressParams {
                token: actual,
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(end)),
            }) => {
                assert_eq!(actual, token);
                assert!(end.message.is_none());
            }
            event => panic!("expected cancellation end, got {event:?}"),
        }

        ticket.report("late report");
        ticket.finish("late finish");
        harness.probe().await;
        assert!(!coordinator.is_active_for_test(1));
        assert!(matches!(harness.events.try_recv(), Err(mpsc::error::TryRecvError::Empty)));
        harness.shutdown().await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn unknown_or_stale_cancellation_does_not_close_current_token() {
        let mut harness = progress_harness();
        let coordinator = ProgressCoordinator::with_timing(
            harness.client.clone(),
            true,
            Duration::from_secs(60),
            Duration::from_secs(1),
        );
        let stale = coordinator.start(1);
        let stale_token = stale.guard.as_ref().unwrap().token.clone();
        stale.finish("stale");
        let current = coordinator.start(2);
        let current_token = current.guard.as_ref().unwrap().token.clone();

        coordinator.cancel(&NumberOrString::String("unknown".into()));
        coordinator.cancel(&stale_token);
        assert!(coordinator.is_active_for_test(2));

        current.finish("done");
        assert!(!coordinator.is_active_for_test(2));
        assert!(matches!(harness.events.try_recv(), Err(mpsc::error::TryRecvError::Empty)));
        assert_ne!(stale_token, current_token);
        harness.shutdown().await;
    }

    #[test]
    fn disabled_coordinator_returns_noop_ticket() {
        let coordinator = ProgressCoordinator::new(ClientSocket::new_closed(), false);
        let ticket = coordinator.start(1);
        assert!(ticket.is_disabled());
        ticket.report("ignored");
        ticket.finish("ignored");
        assert!(!coordinator.is_active_for_test(1));
    }

    #[test]
    fn disabled_guard_cannot_start_creation() {
        let enabled = Arc::new(AtomicBool::new(false));
        let guard = WorkDoneProgressGuard::new(
            ClientSocket::new_closed(),
            enabled,
            1,
            Timing { delay: Duration::ZERO, create_timeout: Duration::from_secs(1) },
        );

        assert!(!guard.mark_creating());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn replacement_reuses_a_wave_that_finished_while_create_was_pending() {
        let mut harness = progress_harness();
        let coordinator = ProgressCoordinator::with_timing(
            harness.client.clone(),
            true,
            Duration::ZERO,
            Duration::from_secs(1),
        );
        let first = coordinator.start(1);
        let ClientEvent::Create(create) = next_event(&mut harness.events).await else {
            panic!("expected create request")
        };
        let token = create.token;
        first.finish("first finished");

        let second = coordinator.start(2);
        assert!(Arc::ptr_eq(first.guard.as_ref().unwrap(), second.guard.as_ref().unwrap()));
        harness.acknowledge_create();

        match next_event(&mut harness.events).await {
            ClientEvent::Progress(ProgressParams {
                token: actual,
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(begin)),
            }) => {
                assert_eq!(actual, token);
                assert_eq!(begin.message.as_deref(), Some(RESTART_MESSAGE));
            }
            event => panic!("expected progress begin, got {event:?}"),
        }
        harness.probe().await;
        assert!(matches!(harness.events.try_recv(), Err(mpsc::error::TryRecvError::Empty)));

        second.finish("second finished");
        match next_event(&mut harness.events).await {
            ClientEvent::Progress(ProgressParams {
                token: actual,
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(end)),
            }) => {
                assert_eq!(actual, token);
                assert_eq!(end.message.as_deref(), Some("second finished"));
            }
            event => panic!("expected current end, got {event:?}"),
        }

        harness.shutdown().await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn replacement_across_create_ack_emits_one_restart_message() {
        let mut harness = progress_harness();
        let coordinator = ProgressCoordinator::with_timing(
            harness.client.clone(),
            true,
            Duration::ZERO,
            Duration::from_secs(1),
        );
        let first = coordinator.start(1);

        let ClientEvent::Create(create) = next_event(&mut harness.events).await else {
            panic!("expected create request")
        };
        let token = create.token;
        let second = coordinator.start(2);
        assert!(Arc::ptr_eq(first.guard.as_ref().unwrap(), second.guard.as_ref().unwrap()));

        harness.acknowledge_create();
        match next_event(&mut harness.events).await {
            ClientEvent::Progress(ProgressParams {
                token: actual,
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(begin)),
            }) => {
                assert_eq!(actual, token);
                assert_eq!(begin.message.as_deref(), Some(RESTART_MESSAGE));
            }
            event => panic!("expected progress begin, got {event:?}"),
        }

        let latest = coordinator.start(3);
        assert!(Arc::ptr_eq(first.guard.as_ref().unwrap(), latest.guard.as_ref().unwrap()));
        harness.probe().await;
        assert!(matches!(harness.events.try_recv(), Err(mpsc::error::TryRecvError::Empty)));

        first.finish("stale");
        second.finish("stale");
        latest.finish("done");
        match next_event(&mut harness.events).await {
            ClientEvent::Progress(ProgressParams {
                token: actual,
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(end)),
            }) => {
                assert_eq!(actual, token);
                assert_eq!(end.message.as_deref(), Some("done"));
            }
            event => panic!("expected current end, got {event:?}"),
        }

        harness.shutdown().await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn consecutive_replacements_emit_one_restart_report_per_wave() {
        let mut harness = progress_harness();
        let coordinator = ProgressCoordinator::with_timing(
            harness.client.clone(),
            true,
            Duration::ZERO,
            Duration::from_secs(1),
        );
        let first = coordinator.start(1);

        let ClientEvent::Create(create) = next_event(&mut harness.events).await else {
            panic!("expected create request")
        };
        let token = create.token;
        harness.acknowledge_create();
        assert!(matches!(
            next_event(&mut harness.events).await,
            ClientEvent::Progress(ProgressParams {
                token: actual,
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(_)),
            }) if actual == token
        ));

        let _second = coordinator.start(2);
        match next_event(&mut harness.events).await {
            ClientEvent::Progress(ProgressParams {
                token: actual,
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::Report(report)),
            }) => {
                assert_eq!(actual, token);
                assert_eq!(report.message.as_deref(), Some(RESTART_MESSAGE));
            }
            event => panic!("expected replacement report, got {event:?}"),
        }

        let _third = coordinator.start(3);
        let latest = coordinator.start(4);
        harness.probe().await;
        assert!(matches!(harness.events.try_recv(), Err(mpsc::error::TryRecvError::Empty)));

        first.finish("stale");
        latest.finish("done");
        match next_event(&mut harness.events).await {
            ClientEvent::Progress(ProgressParams {
                token: actual,
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(end)),
            }) => {
                assert_eq!(actual, token);
                assert_eq!(end.message.as_deref(), Some("done"));
            }
            event => panic!("expected current end, got {event:?}"),
        }

        harness.shutdown().await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn create_failure_disables_progress_for_the_connection() {
        let coordinator = ProgressCoordinator::with_timing(
            ClientSocket::new_closed(),
            true,
            Duration::ZERO,
            Duration::from_secs(1),
        );
        coordinator.start(1);
        tokio::time::timeout(Duration::from_secs(1), async {
            while coordinator.inner.enabled.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("create failure should disable progress");

        let second = coordinator.start(2);

        assert!(second.guard.is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn create_timeout_keeps_the_request_alive_for_a_late_success() {
        let mut harness = progress_harness();
        let coordinator = ProgressCoordinator::with_timing(
            harness.client.clone(),
            true,
            Duration::ZERO,
            Duration::from_millis(10),
        );
        let first = coordinator.start(1);

        let ClientEvent::Create(create) = next_event(&mut harness.events).await else {
            panic!("expected create request")
        };
        let token = create.token;
        tokio::time::timeout(Duration::from_secs(1), async {
            while !first.guard.as_ref().unwrap().create_timed_out_for_test() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("create timeout should be observed");
        assert!(coordinator.inner.enabled.load(Ordering::Acquire));

        let latest = coordinator.start(2);
        assert!(Arc::ptr_eq(first.guard.as_ref().unwrap(), latest.guard.as_ref().unwrap()));
        latest.finish("done");

        harness.acknowledge_create();
        match next_event(&mut harness.events).await {
            ClientEvent::Progress(ProgressParams {
                token: actual,
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(begin)),
            }) => {
                assert_eq!(actual, token);
                assert_eq!(begin.message.as_deref(), Some("done"));
            }
            event => panic!("expected late progress begin, got {event:?}"),
        }
        match next_event(&mut harness.events).await {
            ClientEvent::Progress(ProgressParams {
                token: actual,
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(end)),
            }) => {
                assert_eq!(actual, token);
                assert_eq!(end.message.as_deref(), Some("done"));
            }
            event => panic!("expected late progress end, got {event:?}"),
        }

        harness.shutdown().await;
    }

    #[test]
    fn create_response_cannot_interleave_with_a_publication() {
        let guard = Arc::new(WorkDoneProgressGuard::new(
            ClientSocket::new_closed(),
            Arc::new(AtomicBool::new(true)),
            1,
            Timing { delay: Duration::ZERO, create_timeout: Duration::from_secs(1) },
        ));
        assert!(guard.mark_creating());
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
            .recv_timeout(Duration::from_secs(1))
            .expect("publication should start while the progress state is locked");

        let (create_started_tx, create_started_rx) = std_mpsc::channel();
        let (create_done_tx, create_done_rx) = std_mpsc::channel();
        let create_task = std::thread::spawn(move || {
            create_started_tx.send(()).unwrap();
            guard.created();
            assert!(published.load(Ordering::Acquire));
            create_done_tx.send(()).unwrap();
        });
        create_started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(matches!(
            create_done_rx.recv_timeout(Duration::from_millis(25)),
            Err(std_mpsc::RecvTimeoutError::Timeout)
        ));

        release_publish_tx.send(()).unwrap();
        publish_task.join().unwrap();
        create_done_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("create response should resume after publication");
        create_task.join().unwrap();
    }

    #[test]
    fn closed_guard_releases_state_lock_before_publication() {
        let guard = WorkDoneProgressGuard::new(
            ClientSocket::new_closed(),
            Arc::new(AtomicBool::new(true)),
            1,
            Timing { delay: Duration::ZERO, create_timeout: Duration::from_secs(1) },
        );
        guard.finish(1, "done");

        guard.finish_active_after("ignored", || {
            assert!(guard.state.try_lock().is_some());
        });
    }

    #[tokio::test(flavor = "current_thread")]
    async fn emits_create_begin_report_end() {
        let mut harness = progress_harness();
        let coordinator = ProgressCoordinator::with_timing(
            harness.client.clone(),
            true,
            Duration::ZERO,
            Duration::from_secs(1),
        );
        let ticket = coordinator.start(7);

        let ClientEvent::Create(create) = next_event(&mut harness.events).await else {
            panic!("expected create request")
        };
        let token = create.token;
        ticket.report("reading sources");
        harness.acknowledge_create();

        match next_event(&mut harness.events).await {
            ClientEvent::Progress(ProgressParams {
                token: actual,
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(begin)),
            }) => {
                assert_eq!(actual, token);
                assert_eq!(begin.title, PROGRESS_TITLE);
                assert_eq!(begin.cancellable, Some(false));
                assert_eq!(begin.message.as_deref(), Some("reading sources"));
                assert!(begin.percentage.is_none());
            }
            event => panic!("expected begin, got {event:?}"),
        }

        ticket.report("analyzing");
        match next_event(&mut harness.events).await {
            ClientEvent::Progress(ProgressParams {
                token: actual,
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::Report(report)),
            }) => {
                assert_eq!(actual, token);
                assert_eq!(report.cancellable, Some(false));
                assert_eq!(report.message.as_deref(), Some("analyzing"));
                assert!(report.percentage.is_none());
            }
            event => panic!("expected report, got {event:?}"),
        }

        ticket.finish("done");
        match next_event(&mut harness.events).await {
            ClientEvent::Progress(ProgressParams {
                token: actual,
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(end)),
            }) => {
                assert_eq!(actual, token);
                assert_eq!(end.message.as_deref(), Some("done"));
            }
            event => panic!("expected end, got {event:?}"),
        }

        harness.shutdown().await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn quick_and_disabled_work_are_silent() {
        let mut harness = progress_harness();
        let quick = ProgressCoordinator::new(harness.client.clone(), true).start(1);
        quick.report("quick");
        quick.finish("done");
        let disabled = ProgressCoordinator::new(harness.client.clone(), false).start(2);
        disabled.report("ignored");
        disabled.finish("ignored");

        sleep(PROGRESS_DELAY + Duration::from_millis(50)).await;
        harness.probe().await;
        assert!(matches!(harness.events.try_recv(), Err(mpsc::error::TryRecvError::Empty)));

        harness.shutdown().await;
    }
}
