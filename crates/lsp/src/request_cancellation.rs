//! Cancellation tracking for in-flight LSP requests.

use async_lsp::{
    AnyEvent, AnyNotification, AnyRequest, ErrorCode, LspService, RequestId, ResponseError, Result,
};
use lsp_types::notification::{self, Notification};
use std::{
    collections::HashMap,
    future::{Future, pending},
    ops::ControlFlow,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::{mpsc, oneshot};
use tower::{Layer, Service};

/// Tracks request cancellation without applying global readiness backpressure.
///
/// async-lsp's main loop stops polling in-flight requests and incoming notifications while a
/// service is not ready. Capacity limits therefore cannot be enforced through `poll_ready`
/// without preventing the requests that hold that capacity from completing or being cancelled.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RequestCancellationLayer;

impl<S> Layer<S> for RequestCancellationLayer {
    type Service = RequestCancellation<S>;

    fn layer(&self, service: S) -> Self::Service {
        let (completed_tx, completed_rx) = mpsc::unbounded_channel();
        RequestCancellation {
            service,
            ongoing: HashMap::new(),
            completed_tx,
            completed_rx,
            next_generation: 0,
        }
    }
}

struct OngoingRequest {
    generation: u64,
    cancel: oneshot::Sender<()>,
}

pub(crate) struct RequestCancellation<S> {
    service: S,
    ongoing: HashMap<RequestId, OngoingRequest>,
    completed_tx: mpsc::UnboundedSender<(RequestId, u64)>,
    completed_rx: mpsc::UnboundedReceiver<(RequestId, u64)>,
    next_generation: u64,
}

impl<S> RequestCancellation<S> {
    fn remove_completed(&mut self) {
        while let Ok((id, generation)) = self.completed_rx.try_recv() {
            if self.ongoing.get(&id).is_some_and(|request| request.generation == generation) {
                self.ongoing.remove(&id);
            }
        }
    }
}

struct CompletionGuard {
    completed: mpsc::UnboundedSender<(RequestId, u64)>,
    id: RequestId,
    generation: u64,
}

impl Drop for CompletionGuard {
    fn drop(&mut self) {
        let _ = self.completed.send((self.id.clone(), self.generation));
    }
}

impl<S> Service<AnyRequest> for RequestCancellation<S>
where
    S: LspService,
    S::Future: Send + 'static,
    S::Response: Send + 'static,
    S::Error: From<ResponseError> + Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, request: AnyRequest) -> Self::Future {
        self.remove_completed();

        let id = request.id.clone();
        let generation = self.next_generation;
        self.next_generation = self.next_generation.wrapping_add(1);
        let (cancel, cancelled) = oneshot::channel();
        self.ongoing.insert(id.clone(), OngoingRequest { generation, cancel });
        let response = self.service.call(request);
        let completion = CompletionGuard { completed: self.completed_tx.clone(), id, generation };

        Box::pin(async move {
            let _completion = completion;
            let cancelled = async move {
                if cancelled.await.is_err() {
                    pending::<()>().await;
                }
            };
            tokio::select! {
                biased;
                () = cancelled => Err(ResponseError::new(
                    ErrorCode::REQUEST_CANCELLED,
                    "Client cancelled the request",
                ).into()),
                response = response => response,
            }
        })
    }
}

impl<S> LspService for RequestCancellation<S>
where
    S: LspService,
    S::Future: Send + 'static,
    S::Response: Send + 'static,
    S::Error: From<ResponseError> + Send + 'static,
{
    fn notify(&mut self, notification: AnyNotification) -> ControlFlow<Result<()>> {
        self.remove_completed();
        if notification.method == notification::Cancel::METHOD {
            if let Ok(params) =
                serde_json::from_value::<lsp_types::CancelParams>(notification.params)
                && let Some(request) = self.ongoing.remove(&params.id)
            {
                let _ = request.cancel.send(());
            }
            return ControlFlow::Continue(());
        }
        self.service.notify(notification)
    }

    fn emit(&mut self, event: AnyEvent) -> ControlFlow<Result<()>> {
        self.service.emit(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_lsp::router::Router;
    use lsp_types::request::{Request, Shutdown};

    #[tokio::test]
    async fn completed_requests_are_removed_from_the_registry() {
        let mut router = Router::new(());
        router.request::<Shutdown, _>(|_, ()| std::future::ready(Ok(())));
        let mut service = RequestCancellationLayer.layer(router);
        std::future::poll_fn(|cx| service.poll_ready(cx)).await.unwrap();
        let request = serde_json::from_value(serde_json::json!({
            "id": 1,
            "method": Shutdown::METHOD,
        }))
        .unwrap();

        assert_eq!(service.call(request).await.unwrap(), serde_json::Value::Null);
        assert_eq!(service.ongoing.len(), 1);
        service.remove_completed();
        assert!(service.ongoing.is_empty());
    }

    #[tokio::test]
    async fn cancellation_wins_over_a_ready_response_for_string_ids() {
        let mut router = Router::new(());
        router.request::<Shutdown, _>(|_, ()| std::future::ready(Ok(())));
        let mut service = RequestCancellationLayer.layer(router);
        std::future::poll_fn(|cx| service.poll_ready(cx)).await.unwrap();
        let request = serde_json::from_value(serde_json::json!({
            "id": "request-id",
            "method": Shutdown::METHOD,
        }))
        .unwrap();
        let response = service.call(request);
        let cancel = serde_json::from_value(serde_json::json!({
            "method": notification::Cancel::METHOD,
            "params": { "id": "request-id" },
        }))
        .unwrap();

        assert!(service.notify(cancel).is_continue());
        let error = response.await.unwrap_err();
        assert_eq!(error.code, ErrorCode::REQUEST_CANCELLED);
        assert_eq!(error.message, "Client cancelled the request");
    }
}
