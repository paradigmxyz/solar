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
    use crate::test_support::start_request;
    use lsp_types::{
        notification::{DidOpenTextDocument, Exit, Initialized, Notification},
        request::{HoverRequest, Initialize, Request, Shutdown},
    };
    use serde_json::Value;
    use std::{
        future::{pending, ready},
        sync::{Arc, Mutex},
    };

    type NotificationLog = Arc<Mutex<Vec<String>>>;

    #[derive(Clone, Copy, Default)]
    enum ResponseBehavior {
        #[default]
        Ok,
        Error,
        Pending,
    }

    #[derive(Default)]
    struct ControlledService {
        notifications: NotificationLog,
        initialize: ResponseBehavior,
        shutdown: ResponseBehavior,
    }

    impl Service<AnyRequest> for ControlledService {
        type Response = Value;
        type Error = ResponseError;
        type Future = BoxFuture<Result<Value, ResponseError>>;

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
            self.notifications.lock().unwrap().push(notification.method);
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

    fn service(
        initialize: ResponseBehavior,
        shutdown: ResponseBehavior,
    ) -> (Lifecycle<ControlledService>, NotificationLog) {
        let notifications = NotificationLog::default();
        let inner =
            ControlledService { notifications: notifications.clone(), initialize, shutdown };
        (LifecycleLayer.layer(inner), notifications)
    }

    fn logged(notifications: &NotificationLog) -> Vec<String> {
        notifications.lock().unwrap().clone()
    }

    async fn initialize(service: &mut Lifecycle<ControlledService>) {
        service.call(request(Initialize::METHOD)).await.unwrap();
        assert!(service.notify(notification(Initialized::METHOD)).is_continue());
    }

    async fn initialize_and_shutdown(service: &mut Lifecycle<ControlledService>) {
        initialize(service).await;
        service.call(request(Shutdown::METHOD)).await.unwrap();
    }

    #[test]
    fn notifications_before_initialize_are_dropped() {
        let (mut service, notifications) = service(ResponseBehavior::Ok, ResponseBehavior::Ok);

        for method in [DidOpenTextDocument::METHOD, Initialized::METHOD] {
            assert!(service.notify(notification(method)).is_continue());
        }
        assert!(logged(&notifications).is_empty());
    }

    #[tokio::test]
    async fn failed_initialize_does_not_accept_initialized() {
        let (mut service, notifications) = service(ResponseBehavior::Error, ResponseBehavior::Ok);

        let error = service.call(request(Initialize::METHOD)).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::INVALID_PARAMS);

        assert!(service.notify(notification(Initialized::METHOD)).is_continue());
        assert!(logged(&notifications).is_empty());
        assert_eq!(
            service.call(request(HoverRequest::METHOD)).await.unwrap_err().code,
            ErrorCode::SERVER_NOT_INITIALIZED
        );
    }

    #[tokio::test]
    async fn pending_initialize_does_not_accept_initialized() {
        let (mut service, _) = service(ResponseBehavior::Pending, ResponseBehavior::Ok);
        let _initialize = start_request(service.call(request(Initialize::METHOD)));

        assert!(service.notify(notification(Initialized::METHOD)).is_continue());
        assert_eq!(
            service.call(request(HoverRequest::METHOD)).await.unwrap_err().code,
            ErrorCode::SERVER_NOT_INITIALIZED
        );
    }

    #[tokio::test]
    async fn dropped_unpolled_initialize_future_allows_retry() {
        let (mut service, _) = service(ResponseBehavior::Pending, ResponseBehavior::Ok);

        drop(service.call(request(Initialize::METHOD)));
        service.service.initialize = ResponseBehavior::Ok;

        initialize(&mut service).await;
        service.call(request(HoverRequest::METHOD)).await.unwrap();
    }

    #[tokio::test]
    async fn dropped_initialize_future_allows_retry() {
        let (mut service, _) = service(ResponseBehavior::Pending, ResponseBehavior::Ok);
        drop(start_request(service.call(request(Initialize::METHOD))));
        service.service.initialize = ResponseBehavior::Ok;

        initialize(&mut service).await;
        service.call(request(HoverRequest::METHOD)).await.unwrap();
    }

    #[tokio::test]
    async fn pending_shutdown_blocks_requests_and_exits_gracefully() {
        let (mut service, notifications) = service(ResponseBehavior::Ok, ResponseBehavior::Pending);
        initialize(&mut service).await;

        let _shutdown = start_request(service.call(request(Shutdown::METHOD)));
        assert_eq!(
            service.call(request(HoverRequest::METHOD)).await.unwrap_err().code,
            ErrorCode::INVALID_REQUEST
        );
        let methods_before = logged(&notifications);
        assert!(service.notify(notification(DidOpenTextDocument::METHOD)).is_continue());
        assert_eq!(logged(&notifications), methods_before);
        assert!(matches!(service.notify(notification(Exit::METHOD)), ControlFlow::Break(Ok(()))));
    }

    #[tokio::test]
    async fn failed_shutdown_still_exits_gracefully() {
        let (mut service, _) = service(ResponseBehavior::Ok, ResponseBehavior::Error);
        initialize(&mut service).await;

        let error = service.call(request(Shutdown::METHOD)).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::INVALID_PARAMS);
        assert!(matches!(service.notify(notification(Exit::METHOD)), ControlFlow::Break(Ok(()))));
    }

    #[tokio::test]
    async fn requests_follow_lifecycle_state() {
        let (mut service, _) = service(ResponseBehavior::Ok, ResponseBehavior::Ok);
        assert_eq!(
            service.call(request(HoverRequest::METHOD)).await.unwrap_err().code,
            ErrorCode::SERVER_NOT_INITIALIZED
        );
        initialize_and_shutdown(&mut service).await;

        assert_eq!(
            service.call(request(HoverRequest::METHOD)).await.unwrap_err().code,
            ErrorCode::INVALID_REQUEST
        );
    }

    #[test]
    fn exit_before_shutdown_returns_an_error() {
        let (mut service, _) = service(ResponseBehavior::Ok, ResponseBehavior::Ok);

        let result = service.notify(notification(Exit::METHOD));

        assert!(matches!(result, ControlFlow::Break(Err(Error::Protocol(_)))));
    }

    #[tokio::test]
    async fn notifications_after_shutdown_are_dropped_before_exit() {
        let (mut service, notifications) = service(ResponseBehavior::Ok, ResponseBehavior::Ok);
        initialize_and_shutdown(&mut service).await;
        let methods_before = logged(&notifications);

        assert!(service.notify(notification(DidOpenTextDocument::METHOD)).is_continue());
        assert_eq!(logged(&notifications), methods_before);
        assert!(matches!(service.notify(notification(Exit::METHOD)), ControlFlow::Break(Ok(()))));
    }
}
