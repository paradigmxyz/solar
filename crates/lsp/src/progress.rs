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
use tokio::time::{sleep, timeout};

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

    /// Starts or joins the progress wave for `version`.
    ///
    /// A newer version reuses a visible wave and reports a restart. If the previous wave has
    /// already ended or failed, a fresh token is allocated. Tickets for older versions remain
    /// valid handles but cannot report or finish the newer wave.
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
        message: impl Into<String>,
        publish: impl FnOnce() -> T,
    ) -> T {
        let active = self.inner.active.lock();
        let Some(guard) = active.as_ref() else {
            drop(active);
            return publish();
        };
        guard.finish_active_after(message.into(), publish)
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

    pub(crate) fn report(&self, message: impl Into<String>) {
        if let Some(guard) = &self.guard {
            guard.report(self.version, message.into());
        }
    }

    pub(crate) fn finish(&self, message: impl Into<String>) {
        if let Some(guard) = &self.guard {
            guard.finish(self.version, message.into());
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Delayed,
    Creating,
    Begun,
    Failed,
    Ended,
}

struct ProgressState {
    version: usize,
    phase: Phase,
    message: Option<String>,
    terminal: Option<String>,
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
            drop(guard);

            let result = timeout(
                create_timeout,
                client
                    .request::<req::WorkDoneProgressCreate>(WorkDoneProgressCreateParams { token }),
            )
            .await;

            let Some(guard) = weak.upgrade() else { return };
            match result {
                Ok(Ok(())) => guard.created(),
                Ok(Err(error)) => guard.disable(&format!("client rejected progress: {error}")),
                Err(_) => guard.disable("client did not create progress before timeout"),
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

    fn restart(&self, version: usize) -> bool {
        let mut state = self.state.lock();
        if matches!(state.phase, Phase::Failed | Phase::Ended) {
            return false;
        }
        if version <= state.version {
            return true;
        }

        state.version = version;
        state.terminal = None;
        state.message = Some(RESTART_MESSAGE.into());
        if state.phase == Phase::Begun
            && !send_progress(
                &self.client,
                &self.token,
                WorkDoneProgress::Report(WorkDoneProgressReport {
                    cancellable: Some(false),
                    message: state.message.clone(),
                    percentage: None,
                }),
            )
        {
            self.disable_locked(&mut state, "failed to enqueue replacement report");
        }
        true
    }

    fn report(&self, version: usize, message: String) {
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
                    message: Some(message),
                    percentage: None,
                }),
            ) {
                self.disable_locked(&mut state, "failed to enqueue progress report");
            }
        } else if matches!(state.phase, Phase::Delayed | Phase::Creating) {
            state.message = Some(message);
        }
    }

    fn finish(&self, version: usize, message: String) {
        let mut state = self.state.lock();
        if state.version != version || matches!(state.phase, Phase::Failed | Phase::Ended) {
            return;
        }

        match state.phase {
            Phase::Delayed => state.phase = Phase::Ended,
            Phase::Creating => {
                state.terminal.get_or_insert(message);
            }
            Phase::Begun => {
                state.phase = Phase::Ended;
                if !send_progress(
                    &self.client,
                    &self.token,
                    WorkDoneProgress::End(WorkDoneProgressEnd { message: Some(message) }),
                ) {
                    self.enabled.store(false, Ordering::Release);
                }
            }
            Phase::Failed | Phase::Ended => {}
        }
    }

    fn finish_active_after<T>(&self, message: String, publish: impl FnOnce() -> T) -> T {
        let mut state = self.state.lock();
        let result = publish();
        self.finish_active_locked(&mut state, message);
        result
    }

    fn finish_active_locked(&self, state: &mut ProgressState, message: String) {
        if matches!(state.phase, Phase::Failed | Phase::Ended) {
            return;
        }

        match state.phase {
            Phase::Delayed => state.phase = Phase::Ended,
            Phase::Creating => {
                state.message = Some(message.clone());
                state.terminal = Some(message);
            }
            Phase::Begun => {
                state.phase = Phase::Ended;
                if !send_progress(
                    &self.client,
                    &self.token,
                    WorkDoneProgress::End(WorkDoneProgressEnd { message: Some(message) }),
                ) {
                    self.enabled.store(false, Ordering::Release);
                }
            }
            Phase::Failed | Phase::Ended => {}
        }
    }

    fn created(&self) {
        let mut state = self.state.lock();
        if state.phase != Phase::Creating {
            return;
        }

        state.phase = Phase::Begun;
        let message = state.message.take();
        let begin = WorkDoneProgress::Begin(WorkDoneProgressBegin {
            title: PROGRESS_TITLE.into(),
            cancellable: Some(false),
            message: None,
            percentage: None,
        });
        if !send_progress(&self.client, &self.token, begin) {
            self.disable_locked(&mut state, "failed to enqueue progress begin");
            return;
        }

        if let Some(message) = message
            && !send_progress(
                &self.client,
                &self.token,
                WorkDoneProgress::Report(WorkDoneProgressReport {
                    cancellable: Some(false),
                    message: Some(message),
                    percentage: None,
                }),
            )
        {
            self.disable_locked(&mut state, "failed to enqueue pending progress report");
            return;
        }

        if let Some(message) = state.terminal.take() {
            state.phase = Phase::Ended;
            if !send_progress(
                &self.client,
                &self.token,
                WorkDoneProgress::End(WorkDoneProgressEnd { message: Some(message) }),
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
        state.phase = Phase::Failed;
        state.message = None;
        state.terminal = None;
    }

    #[cfg(test)]
    fn is_current_and_open(&self, version: usize) -> bool {
        let state = self.state.lock();
        state.version == version && !matches!(state.phase, Phase::Failed | Phase::Ended)
    }
}

impl Drop for WorkDoneProgressGuard {
    fn drop(&mut self) {
        let mut state = self.state.lock();
        if state.phase != Phase::Begun {
            return;
        }
        state.phase = Phase::Ended;
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
            self.server.request::<req::Shutdown>(()).await.unwrap();
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
            router
                .request::<req::Shutdown, _>(|_, ()| async { Ok(()) })
                .notification::<notif::Exit>(|_, ()| ControlFlow::Break(Ok(())));
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

    #[test]
    fn disabled_coordinator_returns_noop_ticket() {
        let coordinator = ProgressCoordinator::new(ClientSocket::new_closed(), false);
        let ticket = coordinator.start(1);
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

        assert!(matches!(
            next_event(&mut harness.events).await,
            ClientEvent::Progress(ProgressParams {
                token: actual,
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(_)),
            }) if actual == token
        ));
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
    async fn create_timeout_ignores_a_late_success_and_disables_future_progress() {
        let mut harness = progress_harness();
        let coordinator = ProgressCoordinator::with_timing(
            harness.client.clone(),
            true,
            Duration::ZERO,
            Duration::from_millis(10),
        );
        coordinator.start(1);

        assert!(matches!(next_event(&mut harness.events).await, ClientEvent::Create(_)));
        tokio::time::timeout(Duration::from_secs(1), async {
            while coordinator.inner.enabled.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("create timeout should disable progress");
        assert!(coordinator.start(2).guard.is_none());

        harness.acknowledge_create();
        harness.probe().await;
        assert!(
            tokio::time::timeout(Duration::from_millis(25), harness.events.recv()).await.is_err()
        );

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
        guard.finish(1, "obsolete completion".into());

        let published = Arc::new(AtomicBool::new(false));
        let (publish_started_tx, publish_started_rx) = std_mpsc::channel();
        let (release_publish_tx, release_publish_rx) = std_mpsc::channel();
        let publish_guard = guard.clone();
        let publish_complete = published.clone();
        let publish_task = std::thread::spawn(move || {
            publish_guard.finish_active_after("publication complete".into(), || {
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
                assert!(begin.message.is_none());
                assert!(begin.percentage.is_none());
            }
            event => panic!("expected begin, got {event:?}"),
        }
        match next_event(&mut harness.events).await {
            ClientEvent::Progress(ProgressParams {
                token: actual,
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::Report(report)),
            }) => {
                assert_eq!(actual, token);
                assert_eq!(report.cancellable, Some(false));
                assert_eq!(report.message.as_deref(), Some("reading sources"));
                assert!(report.percentage.is_none());
            }
            event => panic!("expected initial report, got {event:?}"),
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
        assert!(matches!(harness.events.try_recv(), Err(mpsc::error::TryRecvError::Empty)));

        harness.shutdown().await;
    }
}
