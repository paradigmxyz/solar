//! LSP server lifecycle middleware.

use async_lsp::{
    AnyEvent, AnyNotification, AnyRequest, Error, ErrorCode, LspService, ResponseError, Result,
};
use either::Either;
use lsp_types::{
    notification::{self, Notification},
    request::{self, Request},
};
use std::{
    future::{Future, ready},
    ops::ControlFlow,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};
use tower::{Layer, Service};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum State {
    #[default]
    Uninitialized,
    Initializing,
    AwaitingInitialized,
    Ready,
    ShuttingDown,
}

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

struct InitializeStateGuard {
    state: Arc<Mutex<State>>,
}

impl InitializeStateGuard {
    fn transition(&self, next: State) {
        let mut state = self.state.lock().unwrap();
        if *state == State::Initializing {
            *state = next;
        }
    }
}

impl Drop for InitializeStateGuard {
    fn drop(&mut self) {
        self.transition(State::Uninitialized);
    }
}

/// Enforces the language server initialization and shutdown sequence.
#[derive(Debug, Default)]
pub(crate) struct Lifecycle<S> {
    service: S,
    state: Arc<Mutex<State>>,
}

impl<S> Lifecycle<S> {
    fn new(service: S) -> Self {
        Self { service, state: Arc::new(Mutex::new(State::Uninitialized)) }
    }

    fn state(&self) -> State {
        *self.state.lock().unwrap()
    }

    fn set_state(&self, state: State) {
        *self.state.lock().unwrap() = state;
    }
}

impl<S> Service<AnyRequest> for Lifecycle<S>
where
    S: LspService,
    S::Error: From<ResponseError> + Send + 'static,
    S::Response: Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Either<S::Future, BoxFuture<Result<S::Response, S::Error>>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, request: AnyRequest) -> Self::Future {
        match (self.state(), request.method.as_str()) {
            (State::Uninitialized, request::Initialize::METHOD) => {
                self.set_state(State::Initializing);
                Either::Right(track_initialize(self.service.call(request), Arc::clone(&self.state)))
            }
            (State::Uninitialized | State::Initializing | State::AwaitingInitialized, _) => {
                Either::Right(Box::pin(ready(Err(ResponseError::new(
                    ErrorCode::SERVER_NOT_INITIALIZED,
                    "Server is not initialized yet",
                )
                .into()))))
            }
            (_, request::Initialize::METHOD) => Either::Right(Box::pin(ready(Err(
                ResponseError::new(ErrorCode::INVALID_REQUEST, "Server is already initialized")
                    .into(),
            )))),
            (State::Ready, _) => {
                if request.method == request::Shutdown::METHOD {
                    self.set_state(State::ShuttingDown);
                }
                Either::Left(self.service.call(request))
            }
            (State::ShuttingDown, _) => Either::Right(Box::pin(ready(Err(ResponseError::new(
                ErrorCode::INVALID_REQUEST,
                "Server is shutting down",
            )
            .into())))),
        }
    }
}

impl<S> LspService for Lifecycle<S>
where
    S: LspService,
    S::Error: From<ResponseError> + Send + 'static,
    S::Response: Send + 'static,
    S::Future: Send + 'static,
{
    fn notify(&mut self, notification: AnyNotification) -> ControlFlow<Result<()>> {
        match (self.state(), notification.method.as_str()) {
            (_, notification::Exit::METHOD) => {
                let graceful = self.state() == State::ShuttingDown;
                if let ControlFlow::Break(Err(error)) = self.service.notify(notification) {
                    return ControlFlow::Break(Err(error));
                }
                if graceful {
                    ControlFlow::Break(Ok(()))
                } else {
                    ControlFlow::Break(Err(Error::Protocol("exit received before shutdown".into())))
                }
            }
            (State::ShuttingDown, _) => {
                // Clients must not send ordinary notifications after shutdown. Dropping them here
                // is a local hardening contract for invalid client behavior.
                ControlFlow::Continue(())
            }
            (State::AwaitingInitialized, notification::Initialized::METHOD) => {
                self.set_state(State::Ready);
                self.service.notify(notification)
            }
            (State::Uninitialized | State::Initializing | State::AwaitingInitialized, _) => {
                ControlFlow::Continue(())
            }
            (_, notification::Initialized::METHOD) => ControlFlow::Break(Err(Error::Protocol(
                format!("Unexpected initialized notification on state {:?}", self.state()),
            ))),
            _ => self.service.notify(notification),
        }
    }

    fn emit(&mut self, event: AnyEvent) -> ControlFlow<Result<()>> {
        self.service.emit(event)
    }
}

fn track_initialize<F, R, E>(future: F, state: Arc<Mutex<State>>) -> BoxFuture<Result<R, E>>
where
    F: Future<Output = Result<R, E>> + Send + 'static,
    R: Send + 'static,
    E: Send + 'static,
{
    let guard = InitializeStateGuard { state };
    Box::pin(async move {
        let result = future.await;
        let next = if result.is_ok() { State::AwaitingInitialized } else { State::Uninitialized };
        guard.transition(next);
        result
    })
}

/// Builds lifecycle middleware around an LSP service.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct LifecycleLayer;

impl<S> Layer<S> for LifecycleLayer {
    type Service = Lifecycle<S>;

    fn layer(&self, service: S) -> Self::Service {
        Lifecycle::new(service)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_lsp::{
        AnyEvent, AnyNotification, AnyRequest, Error, ErrorCode, LspService, ResponseError, Result,
    };
    use lsp_types::{
        notification::{DidOpenTextDocument, Exit, Initialized, Notification},
        request::{HoverRequest, Initialize, Request, Shutdown},
    };
    use serde_json::Value;
    use std::{
        future::{Future, Ready, pending, ready},
        ops::ControlFlow,
        pin::Pin,
        sync::{Arc, Mutex},
        task::{Context, Poll, Waker},
    };
    use tower::{Layer, Service};

    #[derive(Clone, Default)]
    struct NotificationLog(Arc<Mutex<Vec<String>>>);

    impl NotificationLog {
        fn methods(&self) -> Vec<String> {
            self.0.lock().unwrap().clone()
        }
    }

    struct TestService {
        notifications: NotificationLog,
    }

    impl Service<AnyRequest> for TestService {
        type Response = Value;
        type Error = ResponseError;
        type Future = Ready<Result<Value, ResponseError>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _request: AnyRequest) -> Self::Future {
            ready(Ok(Value::Null))
        }
    }

    impl LspService for TestService {
        fn notify(&mut self, notification: AnyNotification) -> ControlFlow<Result<()>> {
            let exits = notification.method == Exit::METHOD;
            self.notifications.0.lock().unwrap().push(notification.method);
            if exits { ControlFlow::Break(Ok(())) } else { ControlFlow::Continue(()) }
        }

        fn emit(&mut self, _event: AnyEvent) -> ControlFlow<Result<()>> {
            ControlFlow::Continue(())
        }
    }

    #[derive(Clone, Copy)]
    enum ResponseBehavior {
        Ok,
        Error,
        Pending,
    }

    struct ControlledService {
        notifications: NotificationLog,
        initialize: ResponseBehavior,
        shutdown: ResponseBehavior,
    }

    impl Service<AnyRequest> for ControlledService {
        type Response = Value;
        type Error = ResponseError;
        type Future = Pin<Box<dyn Future<Output = Result<Value, ResponseError>> + Send>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, request: AnyRequest) -> Self::Future {
            let behavior = match request.method.as_str() {
                Initialize::METHOD => self.initialize,
                Shutdown::METHOD => self.shutdown,
                _ => ResponseBehavior::Ok,
            };
            match behavior {
                ResponseBehavior::Ok => Box::pin(ready(Ok(Value::Null))),
                ResponseBehavior::Error => Box::pin(ready(Err(ResponseError::new(
                    ErrorCode::INVALID_PARAMS,
                    "test initialize failure",
                )))),
                ResponseBehavior::Pending => Box::pin(pending()),
            }
        }
    }

    impl LspService for ControlledService {
        fn notify(&mut self, notification: AnyNotification) -> ControlFlow<Result<()>> {
            let exits = notification.method == Exit::METHOD;
            self.notifications.0.lock().unwrap().push(notification.method);
            if exits { ControlFlow::Break(Ok(())) } else { ControlFlow::Continue(()) }
        }

        fn emit(&mut self, _event: AnyEvent) -> ControlFlow<Result<()>> {
            ControlFlow::Continue(())
        }
    }

    fn notification(method: &str) -> AnyNotification {
        serde_json::from_value(serde_json::json!({ "method": method })).unwrap()
    }

    fn request(method: &str) -> AnyRequest {
        serde_json::from_value(serde_json::json!({ "id": 1, "method": method })).unwrap()
    }

    async fn initialize_and_shutdown(service: &mut Lifecycle<TestService>) {
        service.call(request(Initialize::METHOD)).await.unwrap();
        assert!(service.notify(notification(Initialized::METHOD)).is_continue());
        service.call(request(Shutdown::METHOD)).await.unwrap();
    }

    #[test]
    fn notification_before_initialize_is_dropped() {
        let notifications = NotificationLog::default();
        let mut service =
            LifecycleLayer.layer(TestService { notifications: notifications.clone() });

        let result = service.notify(notification(DidOpenTextDocument::METHOD));

        assert!(result.is_continue());
        assert!(notifications.methods().is_empty());
    }

    #[test]
    fn initialized_before_initialize_is_dropped() {
        let notifications = NotificationLog::default();
        let mut service =
            LifecycleLayer.layer(TestService { notifications: notifications.clone() });

        let result = service.notify(notification(Initialized::METHOD));

        assert!(result.is_continue());
        assert!(notifications.methods().is_empty());
    }

    #[tokio::test]
    async fn failed_initialize_does_not_accept_initialized() {
        let notifications = NotificationLog::default();
        let mut service = LifecycleLayer.layer(ControlledService {
            notifications: notifications.clone(),
            initialize: ResponseBehavior::Error,
            shutdown: ResponseBehavior::Ok,
        });

        let error = service.call(request(Initialize::METHOD)).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::INVALID_PARAMS);

        assert!(service.notify(notification(Initialized::METHOD)).is_continue());
        assert!(notifications.methods().is_empty());
        assert_eq!(
            service.call(request(HoverRequest::METHOD)).await.unwrap_err().code,
            ErrorCode::SERVER_NOT_INITIALIZED
        );
    }

    #[tokio::test]
    async fn pending_initialize_does_not_accept_initialized() {
        let notifications = NotificationLog::default();
        let mut service = LifecycleLayer.layer(ControlledService {
            notifications,
            initialize: ResponseBehavior::Pending,
            shutdown: ResponseBehavior::Ok,
        });
        let mut initialize = Box::pin(service.call(request(Initialize::METHOD)));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(initialize.as_mut().poll(&mut cx).is_pending());

        assert!(service.notify(notification(Initialized::METHOD)).is_continue());
        assert_eq!(
            service.call(request(HoverRequest::METHOD)).await.unwrap_err().code,
            ErrorCode::SERVER_NOT_INITIALIZED
        );
    }

    #[tokio::test]
    async fn dropped_unpolled_initialize_future_allows_retry() {
        let notifications = NotificationLog::default();
        let mut service = LifecycleLayer.layer(ControlledService {
            notifications,
            initialize: ResponseBehavior::Pending,
            shutdown: ResponseBehavior::Ok,
        });

        drop(service.call(request(Initialize::METHOD)));
        service.service.initialize = ResponseBehavior::Ok;

        service.call(request(Initialize::METHOD)).await.unwrap();
        assert!(service.notify(notification(Initialized::METHOD)).is_continue());
        service.call(request(HoverRequest::METHOD)).await.unwrap();
    }

    #[tokio::test]
    async fn dropped_initialize_future_allows_retry() {
        let notifications = NotificationLog::default();
        let mut service = LifecycleLayer.layer(ControlledService {
            notifications,
            initialize: ResponseBehavior::Pending,
            shutdown: ResponseBehavior::Ok,
        });
        let mut initialize = Box::pin(service.call(request(Initialize::METHOD)));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(initialize.as_mut().poll(&mut cx).is_pending());

        drop(initialize);
        service.service.initialize = ResponseBehavior::Ok;

        service.call(request(Initialize::METHOD)).await.unwrap();
        assert!(service.notify(notification(Initialized::METHOD)).is_continue());
        service.call(request(HoverRequest::METHOD)).await.unwrap();
    }

    #[tokio::test]
    async fn pending_shutdown_blocks_requests_and_exits_gracefully() {
        let notifications = NotificationLog::default();
        let mut service = LifecycleLayer.layer(ControlledService {
            notifications: notifications.clone(),
            initialize: ResponseBehavior::Ok,
            shutdown: ResponseBehavior::Pending,
        });
        service.call(request(Initialize::METHOD)).await.unwrap();
        assert!(service.notify(notification(Initialized::METHOD)).is_continue());

        let mut shutdown = Box::pin(service.call(request(Shutdown::METHOD)));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(shutdown.as_mut().poll(&mut cx).is_pending());
        assert_eq!(
            service.call(request(HoverRequest::METHOD)).await.unwrap_err().code,
            ErrorCode::INVALID_REQUEST
        );
        let methods_before = notifications.methods();
        assert!(service.notify(notification(DidOpenTextDocument::METHOD)).is_continue());
        assert_eq!(notifications.methods(), methods_before);
        assert!(matches!(service.notify(notification(Exit::METHOD)), ControlFlow::Break(Ok(()))));
    }

    #[tokio::test]
    async fn failed_shutdown_still_exits_gracefully() {
        let notifications = NotificationLog::default();
        let mut service = LifecycleLayer.layer(ControlledService {
            notifications,
            initialize: ResponseBehavior::Ok,
            shutdown: ResponseBehavior::Error,
        });
        service.call(request(Initialize::METHOD)).await.unwrap();
        assert!(service.notify(notification(Initialized::METHOD)).is_continue());

        let error = service.call(request(Shutdown::METHOD)).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::INVALID_PARAMS);
        assert!(matches!(service.notify(notification(Exit::METHOD)), ControlFlow::Break(Ok(()))));
    }

    #[tokio::test]
    async fn notification_after_shutdown_is_dropped() {
        let notifications = NotificationLog::default();
        let mut service =
            LifecycleLayer.layer(TestService { notifications: notifications.clone() });
        initialize_and_shutdown(&mut service).await;
        let methods_before = notifications.methods();

        let result = service.notify(notification(DidOpenTextDocument::METHOD));

        assert!(result.is_continue());
        assert_eq!(notifications.methods(), methods_before);
    }

    #[tokio::test]
    async fn request_before_initialize_returns_server_not_initialized() {
        let mut service =
            LifecycleLayer.layer(TestService { notifications: NotificationLog::default() });

        let error = service.call(request(HoverRequest::METHOD)).await.unwrap_err();

        assert_eq!(error.code, ErrorCode::SERVER_NOT_INITIALIZED);
    }

    #[tokio::test]
    async fn request_after_shutdown_returns_invalid_request() {
        let mut service =
            LifecycleLayer.layer(TestService { notifications: NotificationLog::default() });
        initialize_and_shutdown(&mut service).await;

        let error = service.call(request(HoverRequest::METHOD)).await.unwrap_err();

        assert_eq!(error.code, ErrorCode::INVALID_REQUEST);
    }

    #[test]
    fn exit_before_shutdown_returns_an_error() {
        let mut service =
            LifecycleLayer.layer(TestService { notifications: NotificationLog::default() });

        let result = service.notify(notification(Exit::METHOD));

        assert!(matches!(result, ControlFlow::Break(Err(Error::Protocol(_)))));
    }

    #[tokio::test]
    async fn exit_after_shutdown_succeeds() {
        let mut service =
            LifecycleLayer.layer(TestService { notifications: NotificationLog::default() });
        initialize_and_shutdown(&mut service).await;

        let result = service.notify(notification(Exit::METHOD));

        assert!(matches!(result, ControlFlow::Break(Ok(()))));
    }
}
