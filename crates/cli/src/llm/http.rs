//! Chat providers reached over HTTP.
//!
//! A [`ChatClient`] posts a [`Transcript`](super::wire::Transcript)'s requests to one provider's
//! endpoint through the [`ChatTransport`] an embedder installed, without a key, or, with the `llm`
//! feature, with the provider's key over the native-trust TLS configuration nanocodex uses for
//! OpenAI. A request that meets a rate limit, overload, a server error, a failed connection, or a
//! failure the transport calls transient is sent up to four times, waiting between tries as long
//! as the provider asks through `retry-after`, or two seconds and then twice as long each time,
//! but never more than a minute; any other failure ends the turn with the provider's message. An
//! endpoint that asks for payment with HTTP 402 is not asked again: only a transport that pays
//! gets past it. The key travels in a request header marked sensitive and nowhere else, and the
//! compiler's client follows no redirect, which would carry it to wherever the redirect points.
//!
//! Replies stream in as server-sent events, each piece passed on as it arrives; once a reply has
//! begun, a failure ends the turn rather than sending the request again. A server that ignores
//! the request to stream sends its reply whole, which is shown whole. A reply beyond 16 MiB ends
//! the turn, and only the first 64 KiB of an error reply are read, so an endpoint cannot exhaust
//! the compiler's memory.

use super::{
    ChatTransport,
    provider::{Prices, Provider},
    wire::{ANTHROPIC_VERSION, Delta, Protocol, ReplyStream, SseParser},
};
use reqwest::{
    Method, Request, Response, StatusCode, Url,
    header::{AUTHORIZATION, CONTENT_TYPE, HeaderValue, RETRY_AFTER},
};
use serde_json::Value;
use std::{sync::Arc, time::Duration};

#[cfg(feature = "llm")]
use super::TransportError;
#[cfg(feature = "llm")]
use reqwest::Client;

/// Attempts at one request before its turn fails.
const ATTEMPTS: u32 = 4;
/// The first wait before a retry; each later wait doubles.
const FIRST_RETRY: Duration = Duration::from_secs(2);
/// The longest wait before a retry.
const MAX_RETRY: Duration = Duration::from_secs(60);
/// The most characters of a provider's error message a diagnostic repeats.
const MAX_MESSAGE: usize = 500;
/// The most bytes a reply may take, streamed or whole: many times what a reply of the most tokens
/// a request allows takes, even with an event's framing around every token.
const MAX_REPLY_BYTES: usize = 16 << 20;
/// The most bytes of an error reply read for its message.
const MAX_ERROR_BYTES: usize = 64 << 10;

/// What carries a chat endpoint's requests.
enum Sender {
    /// The compiler's own client, with the provider's key.
    #[cfg(feature = "llm")]
    Client(Client),
    /// An embedder's transport, which authenticates or pays for each request itself.
    Transport(Arc<dyn ChatTransport>),
}

/// One provider's chat endpoint.
pub(super) struct ChatClient {
    sender: Sender,
    url: Url,
    provider: Provider,
    protocol: Protocol,
    /// The key header's value; an embedder's transport sends none.
    key: Option<HeaderValue>,
    /// What the model's tokens cost, when known.
    pub(super) prices: Option<Prices>,
}

impl ChatClient {
    /// Connects to `provider` at the API base URL `endpoint`, with `key` over the compiler's own
    /// client, or without a key through `transport`.
    pub(super) async fn new(
        provider: Provider,
        endpoint: &str,
        key: Option<&str>,
        transport: Option<Arc<dyn ChatTransport>>,
        prices: Option<Prices>,
    ) -> Result<Self, String> {
        let protocol = match provider {
            Provider::Anthropic => Protocol::Messages,
            Provider::OpenCode | Provider::OpenAiChat => Protocol::ChatCompletions,
            Provider::OpenAi => unreachable!("OpenAI models are asked through nanocodex"),
        };
        let url = format!("{}/{}", endpoint.trim_end_matches('/'), protocol.path());
        let url = Url::parse(&url).map_err(|error| format!("`{endpoint}` is no URL: {error}"))?;
        let key = key
            .map(|key| {
                let key = match protocol {
                    Protocol::Messages => HeaderValue::from_str(key),
                    Protocol::ChatCompletions => HeaderValue::from_str(&format!("Bearer {key}")),
                };
                // The message must not repeat the key.
                let Ok(mut key) = key else {
                    return Err(format!("`{}` is not a valid key", provider.key_variable()));
                };
                key.set_sensitive(true);
                Ok(key)
            })
            .transpose()?;
        let sender = match transport {
            Some(transport) => Sender::Transport(transport),
            #[cfg(not(feature = "llm"))]
            None => return Err("this build sends requests only through a transport".into()),
            #[cfg(feature = "llm")]
            None => {
                let tls = nanocodex::oai::tls::native_client_config()
                    .await
                    .map_err(|error| format!("cannot load trusted certificates: {error}"))?;
                // An API answers where it is asked. A redirect to another host would also carry
                // Anthropic's key there, whose header, unlike `authorization`, redirects keep.
                let client = Client::builder()
                    .use_preconfigured_tls((*tls).clone())
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                    .map_err(|error| error.to_string())?;
                Sender::Client(client)
            }
        };
        Ok(Self { sender, url, provider, protocol, key, prices })
    }

    /// The wire format the endpoint speaks.
    pub(super) const fn protocol(&self) -> Protocol {
        self.protocol
    }

    /// Sends `body`, passing each piece of the reply to `on_delta` as it streams in, and returns
    /// the whole reply, which `reply` assembles. `on_retry` hears why the request is sent again.
    pub(super) async fn send(
        &self,
        body: &Value,
        reply: ReplyStream,
        on_delta: impl Fn(Delta),
        on_retry: impl Fn(String),
    ) -> Result<Value, String> {
        let name = self.provider.name();
        let body = serde_json::to_vec(body).map_err(|error| error.to_string())?;
        let mut wait = FIRST_RETRY;
        let mut attempt = 1;
        loop {
            let retry = |after: Option<Duration>| {
                (attempt < ATTEMPTS).then(|| after.unwrap_or(wait).min(MAX_RETRY))
            };
            let request = self.request(&body);
            let sent = match &self.sender {
                #[cfg(feature = "llm")]
                Sender::Client(client) => client.execute(request).await.map_err(|error| {
                    let message = error.to_string();
                    if error.is_connect() || error.is_timeout() {
                        TransportError::transient(message)
                    } else {
                        TransportError::permanent(message)
                    }
                }),
                Sender::Transport(transport) => transport.send(request).await,
            };
            let pause = match sent {
                Ok(response) if response.status().is_success() => {
                    return self.read(response, reply, on_delta).await;
                }
                Ok(response) => {
                    let status = response.status();
                    let after = response
                        .headers()
                        .get(RETRY_AFTER)
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value| value.trim().parse().ok())
                        .map(Duration::from_secs);
                    if status == StatusCode::PAYMENT_REQUIRED {
                        return Err(match &self.sender {
                            #[cfg(feature = "llm")]
                            Sender::Client(_) => format!(
                                "{name} answered {status}: the endpoint asks for payment, which \
                                 only an embedder's transport can make"
                            ),
                            Sender::Transport(_) => {
                                let answer = format!(
                                    "{name} answered {status} through the embedder's transport, \
                                     which did not settle the payment"
                                );
                                match message(&error_text(response).await) {
                                    message if message.is_empty() => answer,
                                    message => format!("{answer}: {message}"),
                                }
                            }
                        });
                    }
                    let answer = match message(&error_text(response).await) {
                        message if message.is_empty() => format!("{name} answered {status}"),
                        message => format!("{name} answered {status}: {message}"),
                    };
                    match retry(after).filter(|_| retryable(status)) {
                        Some(pause) => {
                            on_retry(format!(
                                "{answer}; sending again in {:.1} s",
                                pause.as_secs_f64()
                            ));
                            pause
                        }
                        None => return Err(answer),
                    }
                }
                Err(error) => {
                    let failure = format!("cannot reach {name}: {error}");
                    match retry(None).filter(|_| error.transient) {
                        Some(pause) => {
                            on_retry(format!(
                                "{failure}; trying again in {:.1} s",
                                pause.as_secs_f64()
                            ));
                            pause
                        }
                        None => return Err(failure),
                    }
                }
            };
            tokio::time::sleep(pause).await;
            wait = (wait * 2).min(MAX_RETRY);
            attempt += 1;
        }
    }

    /// Reads a reply as it streams in.
    async fn read(
        &self,
        mut response: Response,
        mut reply: ReplyStream,
        on_delta: impl Fn(Delta),
    ) -> Result<Value, String> {
        let name = self.provider.name();
        let streamed = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("text/event-stream"));
        let mut received = 0;
        let mut chunk = async || {
            let chunk = response
                .chunk()
                .await
                .map_err(|error| format!("the reply from {name} broke off: {error}"))?;
            received += chunk.as_ref().map_or(0, |chunk| chunk.len());
            if received > MAX_REPLY_BYTES {
                return Err(format!("the reply from {name} exceeds {} MiB", MAX_REPLY_BYTES >> 20));
            }
            Ok(chunk)
        };
        if !streamed {
            let mut body = Vec::new();
            while let Some(piece) = chunk().await? {
                body.extend_from_slice(&piece);
            }
            let whole = serde_json::from_slice::<Value>(&body)
                .map_err(|error| format!("{name} sent an unreadable reply: {error}"))?;
            reply.deltas_of(&whole).into_iter().for_each(&on_delta);
            return Ok(whole);
        }
        let mut parser = SseParser::default();
        loop {
            let events = match chunk().await? {
                Some(chunk) => parser.push(&chunk),
                None => {
                    if let Some(event) = std::mem::take(&mut parser).finish() {
                        reply.read(&event)?.into_iter().for_each(&on_delta);
                    }
                    return reply.finish();
                }
            };
            for event in events {
                reply.read(&event)?.into_iter().for_each(&on_delta);
            }
        }
    }

    /// A request to the endpoint posting `body`, carrying the key when there is one.
    fn request(&self, body: &[u8]) -> Request {
        // POST url
        // content-type: application/json
        // messages: anthropic-version, x-api-key (with a key)
        // chat completions: authorization (with a key)
        let mut request = Request::new(Method::POST, self.url.clone());
        let headers = request.headers_mut();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        match self.protocol {
            Protocol::Messages => {
                headers.insert("anthropic-version", HeaderValue::from_static(ANTHROPIC_VERSION));
                if let Some(key) = &self.key {
                    headers.insert("x-api-key", key.clone());
                }
            }
            Protocol::ChatCompletions => {
                if let Some(key) = &self.key {
                    headers.insert(AUTHORIZATION, key.clone());
                }
            }
        }
        *request.body_mut() = Some(body.to_vec().into());
        request
    }
}

/// Whether a request that failed with `status` may succeed later.
fn retryable(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

/// Returns the start of an error reply's body, at most [`MAX_ERROR_BYTES`] of it, which is all its
/// message needs.
async fn error_text(mut response: Response) -> String {
    let mut body = Vec::new();
    while body.len() < MAX_ERROR_BYTES
        && let Ok(Some(chunk)) = response.chunk().await
    {
        let room = MAX_ERROR_BYTES - body.len();
        body.extend_from_slice(&chunk[..chunk.len().min(room)]);
    }
    String::from_utf8_lossy(&body).into_owned()
}

/// The message in an error reply, which both formats keep at `error.message`, or its text.
fn message(text: &str) -> String {
    let message = serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|reply| reply["error"]["message"].as_str().map(str::to_string))
        .unwrap_or_else(|| text.trim().to_string());
    message.chars().take(MAX_MESSAGE).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{TransportError, set_transport, wire::Transcript};
    use clap::Parser;
    use reqwest::Client;
    use solar_config::CompileOpts;
    use std::{
        future::Future,
        pin::Pin,
        sync::{
            Mutex,
            atomic::{AtomicUsize, Ordering},
        },
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
        runtime::Runtime,
    };

    /// A streamed Messages reply with nothing cheaper to offer.
    const NO_IMPROVEMENT: &str = r#"event: message_start
data: {"type":"message_start","message":{"usage":{"input_tokens":5,"output_tokens":1}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"NO_IMPROVEMENT"}}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":3}}

event: message_stop
data: {"type":"message_stop"}

"#;

    /// The credential a paid request carries.
    const CREDENTIAL: &str = "Payment test";

    /// A gateway that answers a request without the payment credential with an HTTP 402
    /// challenge, and a paid one with `reply`. Returns its API base URL and the requests it saw,
    /// head and body, in lowercase.
    async fn gateway(reply: &'static str) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&seen);
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let request = read_request(&mut socket).await.to_lowercase();
                let paid =
                    request.contains(&format!("authorization: {}", CREDENTIAL.to_lowercase()));
                log.lock().unwrap().push(request);
                let response = if paid {
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\
                         connection: close\r\n\r\n{reply}"
                    )
                } else {
                    "HTTP/1.1 402 Payment Required\r\nwww-authenticate: Payment id=\"test\"\r\n\
                     content-length: 0\r\nconnection: close\r\n\r\n"
                        .to_string()
                };
                socket.write_all(response.as_bytes()).await.unwrap();
                let _ = socket.shutdown().await;
            }
        });
        (url, seen)
    }

    /// Reads one request: its head, then a body of `content-length` bytes.
    async fn read_request(socket: &mut TcpStream) -> String {
        let mut data = Vec::new();
        let mut buffer = [0; 4096];
        loop {
            let read = socket.read(&mut buffer).await.unwrap();
            if read == 0 {
                break;
            }
            data.extend_from_slice(&buffer[..read]);
            let text = String::from_utf8_lossy(&data);
            if let Some(end) = text.find("\r\n\r\n") {
                let length = text[..end]
                    .lines()
                    .filter_map(|line| line.split_once(':'))
                    .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                    .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                if data.len() >= end + 4 + length {
                    break;
                }
            }
        }
        String::from_utf8_lossy(&data).into_owned()
    }

    /// Pays a 402 challenge by sending the request again with a credential, as an MPP client
    /// does, unless its wallet is empty.
    struct Paying {
        client: Client,
        funded: bool,
        payments: AtomicUsize,
    }

    impl Paying {
        async fn new(funded: bool) -> Arc<Self> {
            Arc::new(Self { client: client().await, funded, payments: AtomicUsize::new(0) })
        }
    }

    impl ChatTransport for Paying {
        fn send(
            &self,
            request: Request,
        ) -> Pin<Box<dyn Future<Output = Result<Response, TransportError>> + Send + '_>> {
            Box::pin(async move {
                let mut paid = request.try_clone().expect("chat requests are buffered");
                let failed = |error: reqwest::Error| TransportError::transient(error.to_string());
                let response = self.client.execute(request).await.map_err(failed)?;
                if response.status() != StatusCode::PAYMENT_REQUIRED || !self.funded {
                    return Ok(response);
                }
                self.payments.fetch_add(1, Ordering::Relaxed);
                paid.headers_mut().insert(AUTHORIZATION, HeaderValue::from_static(CREDENTIAL));
                self.client.execute(paid).await.map_err(failed)
            })
        }
    }

    /// A client for the plain-HTTP gateway.
    async fn client() -> Client {
        // Under the `llm` feature, reqwest's TLS has no crypto provider of its own, so the client
        // takes nanocodex's configuration, as the compiler's own client does.
        #[cfg(feature = "llm")]
        {
            let tls = nanocodex::oai::tls::native_client_config().await.unwrap();
            Client::builder().use_preconfigured_tls((*tls).clone()).build().unwrap()
        }
        #[cfg(not(feature = "llm"))]
        Client::new()
    }

    /// Asks `client` one turn, returning the reply's text.
    async fn ask(client: &ChatClient) -> Result<String, String> {
        let mut transcript = Transcript::new(client.protocol(), "test-model".into(), None);
        transcript.push_prompt("make it cheaper");
        let body = transcript.request("brief");
        let reply = client.send(&body, transcript.reply_stream(), |_| {}, |_| {}).await?;
        transcript.push_reply(&reply).map(|(text, _)| text)
    }

    #[tokio::test]
    async fn transport_pays_the_gateway() {
        let (url, seen) = gateway(NO_IMPROVEMENT).await;
        let transport = Paying::new(true).await;
        let client = ChatClient::new(
            Provider::Anthropic,
            &url,
            None,
            Some(transport.clone() as Arc<dyn ChatTransport>),
            None,
        )
        .await
        .unwrap();
        assert_eq!(ask(&client).await.unwrap(), "NO_IMPROVEMENT");
        assert_eq!(transport.payments.load(Ordering::Relaxed), 1);
        let seen = seen.lock().unwrap();
        let [challenged, paid] = seen.as_slice() else { panic!("{seen:#?}") };
        assert!(!challenged.contains("authorization:"), "{challenged}");
        assert!(paid.contains("authorization: payment test"), "{paid}");
        for request in [challenged, paid] {
            assert!(request.starts_with("post /v1/messages "), "{request}");
            assert!(request.contains("anthropic-version:"), "{request}");
            // The transport authenticates: the compiler sends no key.
            assert!(!request.contains("x-api-key"), "{request}");
            assert!(request.contains(r#""model":"test-model""#), "{request}");
        }
    }

    #[tokio::test]
    async fn unsettled_payment_ends_the_turn() {
        let (url, seen) = gateway(NO_IMPROVEMENT).await;
        let transport = Paying::new(false).await;
        let client =
            ChatClient::new(Provider::Anthropic, &url, None, Some(transport), None).await.unwrap();
        assert_eq!(
            ask(&client).await,
            Err("Anthropic answered 402 Payment Required through the embedder's transport, which \
                 did not settle the payment"
                .into())
        );
        // A payment request is not sent again.
        assert_eq!(seen.lock().unwrap().len(), 1);
    }

    /// A server that answers every request with a redirect to `location`. Returns its API base
    /// URL.
    #[cfg(feature = "llm")]
    async fn redirector(location: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                read_request(&mut socket).await;
                let response = format!(
                    "HTTP/1.1 307 Temporary Redirect\r\nlocation: {location}\r\n\
                     content-length: 0\r\nconnection: close\r\n\r\n"
                );
                socket.write_all(response.as_bytes()).await.unwrap();
                let _ = socket.shutdown().await;
            }
        });
        url
    }

    #[cfg(feature = "llm")]
    #[tokio::test]
    async fn keys_follow_no_redirect() {
        let (elsewhere, seen) = gateway(NO_IMPROVEMENT).await;
        let url = redirector(format!("{elsewhere}/messages")).await;
        let client =
            ChatClient::new(Provider::Anthropic, &url, Some("sk-test"), None, None).await.unwrap();
        assert_eq!(ask(&client).await, Err("Anthropic answered 307 Temporary Redirect".into()));
        // Nothing, and so no key, reached where the redirect pointed.
        assert!(seen.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn oversized_replies_end_the_turn() {
        // One line without an end, larger than any reply may be.
        let line = format!("data: {}", "x".repeat(MAX_REPLY_BYTES));
        let (url, _) = gateway(Box::leak(line.into_boxed_str())).await;
        let transport = Paying::new(true).await;
        let client =
            ChatClient::new(Provider::Anthropic, &url, None, Some(transport), None).await.unwrap();
        assert_eq!(ask(&client).await, Err("the reply from Anthropic exceeds 16 MiB".into()));
    }

    #[cfg(feature = "llm")]
    #[tokio::test]
    async fn unpaid_gateway_ends_the_turn() {
        let (url, seen) = gateway(NO_IMPROVEMENT).await;
        let client =
            ChatClient::new(Provider::Anthropic, &url, Some("sk-test"), None, None).await.unwrap();
        assert_eq!(
            ask(&client).await,
            Err("Anthropic answered 402 Payment Required: the endpoint asks for payment, which \
                 only an embedder's transport can make"
                .into())
        );
        // A payment request is not sent again.
        let seen = seen.lock().unwrap();
        let [request] = seen.as_slice() else { panic!("{seen:#?}") };
        assert!(request.contains("x-api-key: sk-test"), "{request}");
    }

    /// A live compilation asks through the installed transport, which pays each request, and
    /// needs no key.
    #[test]
    fn live_compilation_through_a_transport() {
        let server = Runtime::new().unwrap();
        let (url, seen) = server.block_on(gateway(NO_IMPROVEMENT));
        let transport = server.block_on(Paying::new(true));
        set_transport(Some(transport.clone()));
        let mut opts = CompileOpts::try_parse_from([
            "solar",
            "../../tests/ui/codegen/mir/llm-optimize/basics.mir",
            "--evm-version=cancun",
            "-Zmir-pipeline=llm-optimize",
            "-Zllm-optimize=live",
            "-Zllm-model=anthropic/test-model",
            &format!("-Zllm-endpoint={url}"),
        ])
        .unwrap();
        opts.finish().unwrap();
        let compiled = crate::run_compiler_args(opts);
        set_transport(None);
        assert!(compiled.is_ok());
        let seen = seen.lock().unwrap();
        let payments = transport.payments.load(Ordering::Relaxed);
        assert!(payments > 0);
        // Every turn was challenged once, then paid.
        assert_eq!(seen.len(), 2 * payments, "{seen:#?}");
        assert!(seen.iter().all(|request| !request.contains("x-api-key")), "{seen:#?}");
    }
}
