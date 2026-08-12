//! LSP server execution trace state and middleware.

use async_lsp::{AnyEvent, AnyNotification, AnyRequest, ClientSocket, LspService};
use either::Either;
use lsp_types::{LogTraceParams, TraceValue, notification, request::Request};
use std::{
    future::Future,
    ops::ControlFlow,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    task::{Context, Poll},
    time::Instant,
};
use tokio::sync::oneshot;
use tower::{Layer, Service};

const TRACE_OFF: u8 = 0;
const TRACE_MESSAGES: u8 = 1;
const TRACE_VERBOSE: u8 = 2;

#[derive(Clone)]
pub(crate) struct ProtocolTrace {
    client: ClientSocket,
    level: Arc<AtomicU8>,
    initialized: Arc<AtomicBool>,
}

impl ProtocolTrace {
    pub(crate) fn new(client: ClientSocket) -> Self {
        Self {
            client,
            level: Arc::new(AtomicU8::new(TRACE_OFF)),
            initialized: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) fn set_level(&self, level: TraceValue) {
        self.level.store(encode_level(level), Ordering::Release);
    }

    pub(crate) fn update_level(&self, level: TraceValue) {
        if self.initialized.load(Ordering::Acquire) {
            self.set_level(level);
        }
    }

    fn mark_initialized(&self) {
        self.initialized.store(true, Ordering::Release);
    }

    fn enabled_level(&self) -> Option<TraceValue> {
        if !self.initialized.load(Ordering::Acquire) {
            return None;
        }
        match self.level.load(Ordering::Acquire) {
            TRACE_MESSAGES => Some(TraceValue::Messages),
            TRACE_VERBOSE => Some(TraceValue::Verbose),
            _ => None,
        }
    }

    fn track_request(&self, method: &str) -> Option<ActiveRequest> {
        // LSP does not allow arbitrary server notifications before initialize completes.
        if method == lsp_types::request::Initialize::METHOD {
            return None;
        }
        self.enabled_level()?;
        let request = ActiveRequest {
            trace: self.clone(),
            method: display_method(method).to_owned(),
            started: Instant::now(),
        };
        Some(request)
    }

    fn emit(&self, message: String, verbose: Option<String>) {
        let _ = self.client.notify::<notification::LogTrace>(LogTraceParams { message, verbose });
    }
}

struct ActiveRequest {
    trace: ProtocolTrace,
    method: String,
    started: Instant,
}

struct TraceBarrier(oneshot::Sender<()>);

impl ActiveRequest {
    async fn complete(self, succeeded: bool) {
        let Some(level) = self.trace.enabled_level() else { return };
        let elapsed = self.started.elapsed().as_millis();
        let message = if succeeded {
            format!("Server completed request `{}` successfully", self.method)
        } else {
            format!("Server completed request `{}` with an error", self.method)
        };
        let verbose =
            (level == TraceValue::Verbose).then(|| format!("Server processing took {elapsed} ms"));
        self.trace.emit(message, verbose);

        // The loopback event follows the completion trace in the queue and gates the response.
        let (reached, barrier) = oneshot::channel();
        if self.trace.client.emit(TraceBarrier(reached)).is_ok() {
            let _ = barrier.await;
        }
    }
}

fn display_method(method: &str) -> &str {
    const MAX_METHOD_LENGTH: usize = 96;

    let method_body = method.strip_prefix("$/").unwrap_or(method);
    if method.len() <= MAX_METHOD_LENGTH
        && method_body.split('/').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        })
    {
        method
    } else {
        "<redacted method>"
    }
}

fn encode_level(level: TraceValue) -> u8 {
    match level {
        TraceValue::Off => TRACE_OFF,
        TraceValue::Messages => TRACE_MESSAGES,
        TraceValue::Verbose => TRACE_VERBOSE,
    }
}

#[derive(Clone)]
pub(crate) struct ProtocolTraceLayer {
    trace: ProtocolTrace,
}

impl ProtocolTraceLayer {
    pub(crate) fn new(trace: ProtocolTrace) -> Self {
        Self { trace }
    }
}

impl<S> Layer<S> for ProtocolTraceLayer {
    type Service = ProtocolTraceService<S>;

    fn layer(&self, service: S) -> Self::Service {
        ProtocolTraceService { service, trace: self.trace.clone() }
    }
}

pub(crate) struct ProtocolTraceService<S> {
    service: S,
    trace: ProtocolTrace,
}

impl<S> Service<AnyRequest> for ProtocolTraceService<S>
where
    S: LspService,
    S::Future: Send + 'static,
    S::Response: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future =
        Either<S::Future, Pin<Box<dyn Future<Output = Result<S::Response, S::Error>> + Send>>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, request: AnyRequest) -> Self::Future {
        let initializes =
            (request.method == lsp_types::request::Initialize::METHOD).then(|| self.trace.clone());
        let active = self.trace.track_request(&request.method);
        let response = self.service.call(request);
        if initializes.is_none() && active.is_none() {
            return Either::Left(response);
        }
        Either::Right(Box::pin(async move {
            let response = response.await;
            if let Some(trace) = initializes
                && response.is_ok()
            {
                trace.mark_initialized();
            }
            if let Some(active) = active {
                active.complete(response.is_ok()).await;
            }
            response
        }))
    }
}

impl<S> LspService for ProtocolTraceService<S>
where
    S: LspService,
    S::Future: Send + 'static,
    S::Response: Send + 'static,
    S::Error: Send + 'static,
{
    fn notify(&mut self, notification: AnyNotification) -> ControlFlow<async_lsp::Result<()>> {
        self.service.notify(notification)
    }

    fn emit(&mut self, event: AnyEvent) -> ControlFlow<async_lsp::Result<()>> {
        match event.downcast::<TraceBarrier>() {
            Ok(TraceBarrier(reached)) => {
                let _ = reached.send(());
                ControlFlow::Continue(())
            }
            Err(event) => self.service.emit(event),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::display_method;

    #[test]
    fn display_method_accepts_only_bounded_ascii_names() {
        let cases = [
            ("a".repeat(96), true),
            ("a".repeat(97), false),
            ("$/cancelRequest".into(), true),
            ("$/".into(), false),
            ("textDocument//hover".into(), false),
            ("textDocument/\u{6587}\u{6863}".into(), false),
            ("textDocument/hover?".into(), false),
        ];

        for (method, accepted) in cases {
            assert_eq!(
                display_method(&method),
                if accepted { method.as_str() } else { "<redacted method>" },
                "unexpected display form for {method:?}",
            );
        }
    }
}
