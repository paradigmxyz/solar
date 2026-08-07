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
    future::{Ready, ready},
    ops::ControlFlow,
    task::{Context, Poll},
};
use tower::{Layer, Service};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum State {
    #[default]
    Uninitialized,
    Initializing,
    Ready,
    ShuttingDown,
}

/// Enforces the language server initialization and shutdown sequence.
#[derive(Debug, Default)]
pub(crate) struct Lifecycle<S> {
    service: S,
    state: State,
}

impl<S> Lifecycle<S> {
    fn new(service: S) -> Self {
        Self { service, state: State::Uninitialized }
    }
}

impl<S> Service<AnyRequest> for Lifecycle<S>
where
    S: LspService,
    S::Error: From<ResponseError>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Either<S::Future, Ready<Result<S::Response, S::Error>>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, request: AnyRequest) -> Self::Future {
        match (self.state, request.method.as_str()) {
            (State::Uninitialized, request::Initialize::METHOD) => {
                self.state = State::Initializing;
                Either::Left(self.service.call(request))
            }
            (State::Uninitialized | State::Initializing, _) => {
                Either::Right(ready(Err(ResponseError::new(
                    ErrorCode::SERVER_NOT_INITIALIZED,
                    "Server is not initialized yet",
                )
                .into())))
            }
            (_, request::Initialize::METHOD) => Either::Right(ready(Err(ResponseError::new(
                ErrorCode::INVALID_REQUEST,
                "Server is already initialized",
            )
            .into()))),
            (State::Ready, _) => {
                if request.method == request::Shutdown::METHOD {
                    self.state = State::ShuttingDown;
                }
                Either::Left(self.service.call(request))
            }
            (State::ShuttingDown, _) => Either::Right(ready(Err(ResponseError::new(
                ErrorCode::INVALID_REQUEST,
                "Server is shutting down",
            )
            .into()))),
        }
    }
}

impl<S> LspService for Lifecycle<S>
where
    S: LspService,
    S::Error: From<ResponseError>,
{
    fn notify(&mut self, notification: AnyNotification) -> ControlFlow<Result<()>> {
        match (self.state, notification.method.as_str()) {
            (_, notification::Exit::METHOD) => {
                let graceful = self.state == State::ShuttingDown;
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
            (_, notification::Initialized::METHOD) => {
                if self.state != State::Initializing {
                    return ControlFlow::Break(Err(Error::Protocol(format!(
                        "Unexpected initialized notification on state {:?}",
                        self.state
                    ))));
                }
                self.state = State::Ready;
                self.service.notify(notification)
            }
            (State::Uninitialized | State::Initializing, _) => ControlFlow::Continue(()),
            _ => self.service.notify(notification),
        }
    }

    fn emit(&mut self, event: AnyEvent) -> ControlFlow<Result<()>> {
        self.service.emit(event)
    }
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
        future::{Ready, ready},
        ops::ControlFlow,
        sync::{Arc, Mutex},
        task::{Context, Poll},
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
