//! Chat providers reached over HTTP.
//!
//! A [`ChatClient`] posts a [`Transcript`](super::wire::Transcript)'s requests to one provider's
//! endpoint with the provider's key, over the native-trust TLS configuration nanocodex uses for
//! OpenAI. A request that meets a rate limit, overload, a server error, or a failed connection is
//! sent up to four times, waiting between tries as long as the provider asks through
//! `retry-after`, or two seconds and then twice as long each time, but never more than a minute;
//! any other failure ends the turn with the provider's message. The key travels in a request
//! header marked sensitive and nowhere else.

use super::{
    provider::{Prices, Provider},
    wire::{ANTHROPIC_VERSION, Protocol},
};
use reqwest::{
    Client, RequestBuilder, StatusCode,
    header::{HeaderValue, RETRY_AFTER},
};
use serde_json::Value;
use std::time::Duration;

/// Attempts at one request before its turn fails.
const ATTEMPTS: u32 = 4;
/// The first wait before a retry; each later wait doubles.
const FIRST_RETRY: Duration = Duration::from_secs(2);
/// The longest wait before a retry.
const MAX_RETRY: Duration = Duration::from_secs(60);
/// The most characters of a provider's error message a diagnostic repeats.
const MAX_MESSAGE: usize = 500;

/// One provider's chat endpoint.
pub(super) struct ChatClient {
    client: Client,
    url: String,
    provider: Provider,
    protocol: Protocol,
    key: HeaderValue,
    /// What the model's tokens cost, when known.
    pub(super) prices: Option<Prices>,
}

impl ChatClient {
    /// Connects to `provider` at the API base URL `endpoint`.
    pub(super) async fn new(
        provider: Provider,
        endpoint: &str,
        key: &str,
        prices: Option<Prices>,
    ) -> Result<Self, String> {
        let protocol = match provider {
            Provider::Anthropic => Protocol::Messages,
            Provider::OpenCode => Protocol::ChatCompletions,
            Provider::OpenAi => unreachable!("OpenAI models are asked through nanocodex"),
        };
        let key = match protocol {
            Protocol::Messages => HeaderValue::from_str(key),
            Protocol::ChatCompletions => HeaderValue::from_str(&format!("Bearer {key}")),
        };
        // The message must not repeat the key.
        let Ok(mut key) = key else {
            return Err(format!("`{}` is not a valid key", provider.key_variable()));
        };
        key.set_sensitive(true);
        let tls = nanocodex::oai::tls::native_client_config()
            .await
            .map_err(|error| format!("cannot load trusted certificates: {error}"))?;
        let client = Client::builder()
            .use_preconfigured_tls((*tls).clone())
            .build()
            .map_err(|error| error.to_string())?;
        let url = format!("{}/{}", endpoint.trim_end_matches('/'), protocol.path());
        Ok(Self { client, url, provider, protocol, key, prices })
    }

    /// The wire format the endpoint speaks.
    pub(super) const fn protocol(&self) -> Protocol {
        self.protocol
    }

    /// Sends `body` and returns the provider's reply.
    pub(super) async fn send(&self, body: &Value) -> Result<Value, String> {
        let name = self.provider.name();
        let mut wait = FIRST_RETRY;
        let mut attempt = 1;
        loop {
            let retry = |after: Option<Duration>| {
                (attempt < ATTEMPTS).then(|| after.unwrap_or(wait).min(MAX_RETRY))
            };
            let pause = match self.request().json(body).send().await {
                Ok(response) if response.status().is_success() => {
                    return response
                        .json::<Value>()
                        .await
                        .map_err(|error| format!("{name} sent an unreadable reply: {error}"));
                }
                Ok(response) => {
                    let status = response.status();
                    let after = response
                        .headers()
                        .get(RETRY_AFTER)
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value| value.trim().parse().ok())
                        .map(Duration::from_secs);
                    let text = response.text().await.unwrap_or_default();
                    match retry(after).filter(|_| retryable(status)) {
                        Some(pause) => pause,
                        None => {
                            return Err(format!("{name} answered {status}: {}", message(&text)));
                        }
                    }
                }
                Err(error) => {
                    match retry(None).filter(|_| error.is_connect() || error.is_timeout()) {
                        Some(pause) => pause,
                        None => return Err(format!("cannot reach {name}: {error}")),
                    }
                }
            };
            tokio::time::sleep(pause).await;
            wait = (wait * 2).min(MAX_RETRY);
            attempt += 1;
        }
    }

    /// A request to the endpoint, carrying the key.
    fn request(&self) -> RequestBuilder {
        let request = self.client.post(&self.url);
        match self.protocol {
            Protocol::Messages => request
                .header("x-api-key", self.key.clone())
                .header("anthropic-version", ANTHROPIC_VERSION),
            Protocol::ChatCompletions => request.header("authorization", self.key.clone()),
        }
    }
}

/// Whether a request that failed with `status` may succeed later.
fn retryable(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

/// The message in an error reply, which both formats keep at `error.message`, or its text.
fn message(text: &str) -> String {
    let message = serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|reply| reply["error"]["message"].as_str().map(str::to_string))
        .unwrap_or_else(|| text.trim().to_string());
    message.chars().take(MAX_MESSAGE).collect()
}
