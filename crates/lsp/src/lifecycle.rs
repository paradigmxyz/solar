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
#[path = "tests/lifecycle.rs"]
mod tests;
