//! The models behind `-Zllm-optimize=live`.
//!
//! With the `llm` feature, `install` registers a [`solar_codegen::llm::LlmRewriter`] that holds
//! one conversation per offered function with the model `-Zllm-model` names as `PROVIDER/MODEL`:
//!
//! - `openai/MODEL`, or `MODEL` alone: a nanocodex agent over OpenAI's Responses API, keyed by
//!   `OPENAI_API_KEY`;
//! - `anthropic/MODEL`: Anthropic's Messages API, keyed by `ANTHROPIC_API_KEY`;
//! - `opencode/MODEL`: OpenCode Zen's chat completions API, keyed by `OPENCODE_ZEN_API_KEY`.
//!
//! Every conversation opens with the rewriting brief in `llm/instructions.md` and has no tools, so
//! the model cannot read files, run commands, or search. nanocodex agents also get a fixed
//! execution environment, so they see neither the host's date nor its `AGENTS.md`. Prompts carry
//! the function, its callees, the objective, and the verdict on the previous candidate; each reply
//! must hold one fenced `mir` block or `NO_IMPROVEMENT`, and a reply with neither gets one
//! reminder. `-Zllm-effort` sets how much the model reasons, in each provider's terms.
//!
//! Replies arrive on a Tokio runtime the rewriter owns, which takes every turn through a channel,
//! so compilation threads wait for replies without entering an async context, whatever runtime
//! an embedder runs. At most four turns run at once, each may take ten minutes, and no turn starts
//! once the estimated spend reaches five dollars or the conversations have used ten million
//! tokens, which bounds a model without known prices. Keys go nowhere but their provider's client.
//! When compilation ends, a note reports turns, tokens, and the estimated cost.
//!
//! While it works, every conversation reports on stderr through `console`: each round, the
//! model's reasoning and reply as they stream in, what each turn used, and each verdict.

use solar_config::LlmOptimizeMode;
use solar_interface::{Result, Session};

#[cfg(feature = "llm")]
use console::Voice;
#[cfg(feature = "llm")]
use http::ChatClient;
#[cfg(feature = "llm")]
use nanocodex::{
    AgentEvents, Model, Nanocodex, OpenAi, Thinking, Tools, UsdAmount, agent::ExecutionEnvironment,
    oai::events::AgentEventKind,
};
#[cfg(feature = "llm")]
use provider::Provider;
#[cfg(feature = "llm")]
use serde_json::Value;
#[cfg(feature = "llm")]
use solar_codegen::llm::{
    CostReport, LlmError, LlmRewriter, LlmSession, RewriteRequest, Verdict, set_rewriter,
};
#[cfg(feature = "llm")]
use solar_config::{ErrorFormat, LlmEffort};
#[cfg(feature = "llm")]
use std::{
    fmt::Write,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
#[cfg(feature = "llm")]
use tokio::{runtime::Runtime, sync::Semaphore};
#[cfg(feature = "llm")]
use wire::{Delta, Transcript};

#[cfg(any(feature = "llm", test))]
use solar_codegen::llm::Proposal;

#[cfg(any(feature = "llm", test))]
#[cfg_attr(not(feature = "llm"), allow(dead_code))]
mod console;
#[cfg(feature = "llm")]
mod http;
#[cfg(any(feature = "llm", test))]
#[cfg_attr(not(feature = "llm"), allow(dead_code))]
mod provider;
#[cfg(any(feature = "llm", test))]
#[cfg_attr(not(feature = "llm"), allow(dead_code))]
mod wire;

/// The rewriting brief every conversation opens with.
#[cfg(feature = "llm")]
const INSTRUCTIONS: &str = include_str!("llm/instructions.md");
/// Turns in flight at once, so parallel compilation does not flood the provider.
#[cfg(feature = "llm")]
const MAX_TURNS: usize = 4;
/// The longest wait for one reply, including the wait for a turn to start.
#[cfg(feature = "llm")]
const TURN_TIMEOUT: Duration = Duration::from_secs(600);
/// Estimated spend, in nano-USD, after which no turn starts.
#[cfg(feature = "llm")]
const BUDGET_NANO_USD: u64 = 5_000_000_000;
/// Tokens after which no turn starts, whatever they cost.
#[cfg(feature = "llm")]
const BUDGET_TOKENS: u64 = 10_000_000;
/// The date agents see: a fixed environment keeps host context out of prompts.
#[cfg(feature = "llm")]
const DATE: &str = "2026-01-01";
/// The reply to a reply that held no candidate.
#[cfg(feature = "llm")]
const FORMAT_REMINDER: &str = "Your reply held no candidate. Reply with exactly one fenced code \
                               block tagged `mir` holding the whole function, or with the single \
                               line `NO_IMPROVEMENT`.";

/// Installs the rewriter `-Zllm-optimize=live` asks, returning it for [`Installed::finish`].
///
/// A rewriter an embedder installed first stays in place.
pub(crate) fn install(sess: &Session) -> Result<Option<Installed>> {
    if sess.opts.unstable.llm_optimize != Some(LlmOptimizeMode::Live)
        || solar_codegen::llm::rewriter().is_some()
    {
        return Ok(None);
    }
    #[cfg(feature = "llm")]
    return Installed::new(sess).map(Some);
    #[cfg(not(feature = "llm"))]
    return Err(sess
        .dcx
        .err("`-Zllm-optimize=live` requires the compiler's `llm` feature")
        .help("build the compiler with `--features llm`")
        .emit());
}

/// An installed rewriter.
#[cfg(not(feature = "llm"))]
pub(crate) enum Installed {}

#[cfg(not(feature = "llm"))]
impl Installed {
    /// Removes the rewriter.
    pub(crate) fn finish(self, _sess: &Session) {
        match self {}
    }
}

/// An installed rewriter and the runtime its conversations run on.
#[cfg(feature = "llm")]
pub(crate) struct Installed {
    shared: Arc<Shared>,
    runtime: Runtime,
}

#[cfg(feature = "llm")]
impl Installed {
    fn new(sess: &Session) -> Result<Self> {
        let unstable = &sess.opts.unstable;
        let (provider, model) = match unstable.llm_model.as_deref() {
            Some(model) => {
                let (provider, model) = Provider::parse(model);
                (provider, Some(model))
            }
            None => (Provider::OpenAi, None),
        };
        // The key goes to the client alone: never to diagnostics, traces, or the cache.
        let variable = provider.key_variable();
        let Ok(key) = std::env::var(variable) else {
            let message =
                format!("`-Zllm-optimize=live` with {} requires `{variable}`", provider.name());
            return Err(sess.dcx.err(message).emit());
        };
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("solar-llm")
            .enable_all()
            .build()
            .map_err(|error| {
                sess.dcx.err(format!("cannot start the model runtime: {error}")).emit()
            })?;
        let configure = |error: String| {
            sess.dcx.err(format!("cannot configure the model client: {error}")).emit()
        };
        let backend = match provider {
            Provider::OpenAi => {
                let model = model
                    .map(str::parse::<Model>)
                    .transpose()
                    .map_err(|error| sess.dcx.err("unknown `-Zllm-model`").note(error).emit())?;
                let thinking = unstable
                    .llm_effort
                    .map(|effort| effort.to_str().parse::<Thinking>())
                    .transpose()
                    .map_err(|error| {
                        sess.dcx.err("unsupported `-Zllm-effort`").note(error).emit()
                    })?;
                let endpoint = unstable.llm_endpoint.clone();
                let client = async move {
                    let mut builder = OpenAi::builder(key);
                    if let Some(endpoint) = endpoint {
                        builder = builder.api_base_url(endpoint);
                    }
                    builder.build()
                };
                let openai = wait(runtime.handle(), client, TURN_TIMEOUT)
                    .map_err(|error| error.to_string())
                    .and_then(|client| client.map_err(|error| error.to_string()))
                    .map_err(configure)?;
                Backend::Agent { openai, model, thinking }
            }
            Provider::Anthropic | Provider::OpenCode => {
                let Some(model) = model.filter(|model| !model.is_empty()) else {
                    let message = format!("`-Zllm-model` names no {} model", provider.name());
                    return Err(sess.dcx.err(message).emit());
                };
                let info = provider::model_info(provider, model);
                if let (Some(effort), Some(info)) = (unstable.llm_effort, info)
                    && !info.efforts.contains(&effort)
                {
                    let efforts = info
                        .efforts
                        .iter()
                        .map(|effort| format!("`{effort}`"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    return Err(sess
                        .dcx
                        .err(format!("`{model}` does not take `-Zllm-effort={effort}`"))
                        .note(format!("it takes {efforts}"))
                        .emit());
                }
                let endpoint = unstable
                    .llm_endpoint
                    .as_deref()
                    .or(provider.default_endpoint())
                    .expect("chat providers have a default endpoint")
                    .to_string();
                let prices = info.map(|info| info.prices);
                let client =
                    async move { ChatClient::new(provider, &endpoint, &key, prices).await };
                let client = wait(runtime.handle(), client, TURN_TIMEOUT)
                    .map_err(|error| error.to_string())
                    .and_then(|client| client)
                    .map_err(configure)?;
                Backend::Chat {
                    client: Arc::new(client),
                    model: model.to_string(),
                    effort: unstable.llm_effort,
                }
            }
        };
        let warning = format!(
            "`-Zllm-optimize=live` sends the MIR of offered functions to {}",
            provider.name()
        );
        sess.dcx.warn(warning).emit();
        let shared = Arc::new(Shared {
            runtime: runtime.handle().clone(),
            provider,
            label: unstable.llm_model.clone().unwrap_or_else(|| "the default OpenAI model".into()),
            console: sess.opts.error_format == ErrorFormat::Human,
            backend,
            turns: Arc::new(Semaphore::new(MAX_TURNS)),
            asked: AtomicU64::new(0),
            tokens: AtomicU64::new(0),
            spent_nano_usd: AtomicU64::new(0),
            unpriced: AtomicU64::new(0),
        });
        set_rewriter(Some(Arc::new(Rewriter(Arc::clone(&shared)))));
        Ok(Self { shared, runtime })
    }

    /// Removes the rewriter and reports what it asked.
    pub(crate) fn finish(self, sess: &Session) {
        set_rewriter(None);
        let shared = &self.shared;
        let asked = shared.asked.load(Ordering::Relaxed);
        if asked != 0 {
            let tokens = shared.tokens.load(Ordering::Relaxed);
            let mut message = format!(
                "`llm-optimize` asked {} {asked} turns using {tokens} tokens",
                shared.provider.name()
            );
            if shared.unpriced.load(Ordering::Relaxed) == 0 {
                let spent = UsdAmount::from_nano_usd(shared.spent_nano_usd.load(Ordering::Relaxed));
                let _ = write!(message, ", an estimated {spent}");
            } else {
                message.push_str(", at a cost the compiler cannot estimate");
            }
            sess.dcx.note(message).emit();
        }
        self.runtime.shutdown_timeout(Duration::from_secs(5));
    }
}

/// State every conversation shares.
#[cfg(feature = "llm")]
struct Shared {
    runtime: tokio::runtime::Handle,
    provider: Provider,
    /// The model as `-Zllm-model` names it.
    label: String,
    /// Whether conversations report on stderr.
    console: bool,
    backend: Backend,
    turns: Arc<Semaphore>,
    asked: AtomicU64,
    tokens: AtomicU64,
    spent_nano_usd: AtomicU64,
    /// Turns whose cost is unknown.
    unpriced: AtomicU64,
}

/// How conversations reach the model.
#[cfg(feature = "llm")]
enum Backend {
    /// A nanocodex agent per conversation.
    Agent { openai: OpenAi, model: Option<Model>, thinking: Option<Thinking> },
    /// A chat API over HTTP.
    Chat { client: Arc<ChatClient>, model: String, effort: Option<LlmEffort> },
}

#[cfg(feature = "llm")]
impl Shared {
    /// Runs `future` on the runtime and waits at most a turn for it.
    fn run<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Result<T, LlmError> {
        wait(&self.runtime, future, TURN_TIMEOUT)
    }

    /// Whether another turn fits the budget.
    fn affordable(&self) -> bool {
        self.spent_nano_usd.load(Ordering::Relaxed) < BUDGET_NANO_USD
            && self.tokens.load(Ordering::Relaxed) < BUDGET_TOKENS
    }

    /// Records an answered turn that used `tokens` and cost `nano_usd`, when that is known.
    fn record(&self, tokens: u64, nano_usd: Option<u64>) {
        self.asked.fetch_add(1, Ordering::Relaxed);
        self.tokens.fetch_add(tokens, Ordering::Relaxed);
        match nano_usd {
            Some(nano_usd) => self.spent_nano_usd.fetch_add(nano_usd, Ordering::Relaxed),
            None => self.unpriced.fetch_add(1, Ordering::Relaxed),
        };
    }
}

/// Runs `future` on `runtime` and waits at most `timeout` for it, from any thread: the caller
/// never enters an async context.
#[cfg(feature = "llm")]
fn wait<T: Send + 'static>(
    runtime: &tokio::runtime::Handle,
    future: impl Future<Output = T> + Send + 'static,
    timeout: Duration,
) -> Result<T, LlmError> {
    let (sender, receiver) = mpsc::sync_channel(1);
    runtime.spawn(async move {
        let _ = sender.send(future.await);
    });
    receiver.recv_timeout(timeout).map_err(|_| LlmError::new("the model did not answer in time"))
}

#[cfg(feature = "llm")]
struct Rewriter(Arc<Shared>);

#[cfg(feature = "llm")]
impl LlmRewriter for Rewriter {
    fn session(&self, request: &RewriteRequest) -> Result<Box<dyn LlmSession>, LlmError> {
        let shared = Arc::clone(&self.0);
        let chat: Box<dyn Chat> = match &shared.backend {
            Backend::Agent { openai, model, thinking } => {
                let (openai, model, thinking) = (openai.clone(), *model, *thinking);
                let (agent, events) = shared
                    .run(async move {
                        let tools = Tools::builder()
                            .without_defaults()
                            .build()
                            .map_err(|error| error.to_string())?;
                        let mut builder = Nanocodex::builder(openai)
                            .instructions(INSTRUCTIONS)
                            .tools(tools)
                            .execution_environment(ExecutionEnvironment::new(DATE, "Etc/UTC"));
                        if let Some(model) = model {
                            builder = builder.model(model);
                        }
                        if let Some(thinking) = thinking {
                            builder = builder.thinking(thinking);
                        }
                        builder.build().map_err(|error| error.to_string())
                    })?
                    .map_err(LlmError::new)?;
                Box::new(AgentChat { runtime: shared.runtime.clone(), agent, events: Some(events) })
            }
            Backend::Chat { client, model, effort } => Box::new(HttpChat {
                transcript: Transcript::new(client.protocol(), model.clone(), *effort),
                client: Arc::clone(client),
            }),
        };
        let voice =
            Arc::new(Voice::new(shared.console, &request.module_name, &request.function_name));
        voice.say(format_args!(
            "costs {}; asking {} for something cheaper",
            request.baseline, shared.label
        ));
        Ok(Box::new(Conversation {
            shared,
            chat,
            best: request.baseline,
            request: request.clone(),
            voice,
            round: 0,
        }))
    }
}

/// A turn's reply and what it used.
#[cfg(feature = "llm")]
struct Turn {
    text: String,
    tokens: u64,
    /// What it cost in nano-USD, when that is known.
    nano_usd: Option<u64>,
}

/// One conversation's exchanges with its model.
#[cfg(feature = "llm")]
trait Chat: Send {
    /// Sends `prompt` and returns the reply, streaming it to `voice` as it arrives.
    fn turn(
        &mut self,
        shared: &Shared,
        prompt: String,
        voice: &Arc<Voice>,
    ) -> Result<Turn, LlmError>;
}

/// A conversation held by a nanocodex agent, which keeps its history.
#[cfg(feature = "llm")]
struct AgentChat {
    runtime: tokio::runtime::Handle,
    agent: Nanocodex,
    /// The agent's events, which each turn reads while it runs.
    events: Option<AgentEvents>,
}

#[cfg(feature = "llm")]
impl Chat for AgentChat {
    fn turn(
        &mut self,
        shared: &Shared,
        prompt: String,
        voice: &Arc<Voice>,
    ) -> Result<Turn, LlmError> {
        let Some(mut events) = self.events.take() else {
            return Err(LlmError::new("an earlier turn lost the agent's events"));
        };
        let (agent, turns, voice) =
            (self.agent.clone(), Arc::clone(&shared.turns), Arc::clone(voice));
        let (result, events) = shared.run(async move {
            let _permit = turns.acquire_owned().await;
            let result = async {
                let turn = agent.prompt(prompt).await?;
                // The reply streams in until the event that ends the turn.
                while let Some(event) = events.recv().await {
                    let delta = match event.kind {
                        AgentEventKind::AssistantDelta => Delta::Reply,
                        AgentEventKind::ReasoningSummaryDelta => Delta::Reasoning,
                        kind if kind.is_terminal() => break,
                        _ => continue,
                    };
                    if let Ok(payload) = serde_json::from_str::<Value>(event.payload.get())
                        && let Some(text) = payload["text"].as_str()
                    {
                        voice.stream(&delta(text.to_string()));
                    }
                }
                turn.await
            }
            .await;
            (result, events)
        })?;
        self.events = Some(events);
        let result = result.map_err(|error| LlmError::new(error.to_string()))?;
        let usage = result.usage();
        Ok(Turn {
            tokens: usage.map_or(0, |usage| usage.total_tokens()),
            nano_usd: usage
                .and_then(|usage| usage.estimated_cost())
                .map(|cost| cost.amount().nano_usd()),
            text: result.into_final_message(),
        })
    }
}

#[cfg(feature = "llm")]
impl Drop for AgentChat {
    fn drop(&mut self) {
        let agent = self.agent.clone();
        self.runtime.spawn(async move {
            let _ = agent.shutdown().await;
        });
    }
}

/// A conversation with a chat API, whose history the transcript keeps.
#[cfg(feature = "llm")]
struct HttpChat {
    client: Arc<ChatClient>,
    transcript: Transcript,
}

#[cfg(feature = "llm")]
impl Chat for HttpChat {
    fn turn(
        &mut self,
        shared: &Shared,
        prompt: String,
        voice: &Arc<Voice>,
    ) -> Result<Turn, LlmError> {
        self.transcript.push_prompt(&prompt);
        let body = self.transcript.request(INSTRUCTIONS);
        let reply = self.transcript.reply_stream();
        let (client, turns, voice) =
            (Arc::clone(&self.client), Arc::clone(&shared.turns), Arc::clone(voice));
        let reply = shared
            .run(async move {
                let _permit = turns.acquire_owned().await;
                client
                    .send(&body, reply, |delta| voice.stream(&delta), |note| voice.say(note))
                    .await
            })?
            .map_err(LlmError::new)?;
        let (text, usage) = self.transcript.push_reply(&reply).map_err(LlmError::new)?;
        let nano_usd = self.client.prices.map(|prices| prices.cost(usage));
        Ok(Turn { text, tokens: usage.total(), nano_usd })
    }
}

/// One conversation about one function.
#[cfg(feature = "llm")]
struct Conversation {
    shared: Arc<Shared>,
    chat: Box<dyn Chat>,
    request: RewriteRequest,
    best: CostReport,
    /// Where the conversation reports.
    voice: Arc<Voice>,
    /// Candidates asked for so far.
    round: usize,
}

#[cfg(feature = "llm")]
impl LlmSession for Conversation {
    fn propose(&mut self, verdict: Option<&Verdict>) -> Result<Proposal, LlmError> {
        let proposal = self.ask(verdict);
        match &proposal {
            Ok(Proposal::Candidate(_)) => {}
            Ok(Proposal::Done) => self.voice.say("the model has nothing cheaper"),
            Err(error) => self.voice.say(format_args!("stopped: {error}")),
        }
        proposal
    }

    fn finish(&mut self, verdict: Option<&Verdict>, kept: Option<CostReport>) {
        if let Some(verdict) = verdict {
            self.hear(verdict);
        }
        match kept {
            Some(cost) => self.voice.say(format_args!(
                "keeps a rewrite at {cost}, down from {}",
                self.request.baseline
            )),
            None => self.voice.say("keeps the function as it was"),
        }
    }
}

#[cfg(feature = "llm")]
impl Conversation {
    /// Asks for the next candidate after `verdict`.
    fn ask(&mut self, verdict: Option<&Verdict>) -> Result<Proposal, LlmError> {
        let prompt = match verdict {
            None => first_prompt(&self.request),
            Some(verdict) => {
                self.hear(verdict);
                verdict_prompt(verdict, self.best)
            }
        };
        self.round += 1;
        self.voice.say(format_args!("round {}", self.round));
        if let Some(proposal) = extract(&self.turn(prompt)?) {
            return Ok(proposal);
        }
        self.voice.say("the reply held no candidate; reminding the model of the format");
        extract(&self.turn(FORMAT_REMINDER.to_string())?)
            .ok_or_else(|| LlmError::new("the model's reply held no candidate"))
    }

    /// Reports a verdict and keeps the cost to beat.
    fn hear(&mut self, verdict: &Verdict) {
        match verdict {
            Verdict::Accepted { cost } => {
                self.best = *cost;
                self.voice.say(format_args!("accepted at {cost}"));
            }
            Verdict::Rejected { stage, reason, counterexample } => {
                let mut message = format!("rejected at {stage}: {reason}");
                if let Some(input) = counterexample {
                    let _ = write!(message, "; input {input}");
                }
                self.voice.say(message);
            }
        }
    }

    /// Sends `prompt` and returns the reply, unless the budget is spent.
    fn turn(&mut self, prompt: String) -> Result<String, LlmError> {
        if !self.shared.affordable() {
            return Err(LlmError::new("the conversations spent their budget"));
        }
        let start = Instant::now();
        let turn = self.chat.turn(&self.shared, prompt, &self.voice);
        self.voice.flush();
        let Turn { text, tokens, nano_usd } = turn?;
        self.shared.record(tokens, nano_usd);
        let mut message =
            format!("replied in {:.1} s using {tokens} tokens", start.elapsed().as_secs_f64());
        if let Some(nano_usd) = nano_usd {
            let _ = write!(message, ", an estimated {}", UsdAmount::from_nano_usd(nano_usd));
        }
        self.voice.say(message);
        Ok(text)
    }
}

/// Renders the first prompt of a conversation.
#[cfg(feature = "llm")]
fn first_prompt(request: &RewriteRequest) -> String {
    let mut prompt = String::new();
    let _ = writeln!(
        prompt,
        "Optimize this function for {} on the {} EVM, expecting {} calls per deployment. It \
         costs {}.\n\n```mir\n{}```\n",
        request.objective,
        request.evm_version,
        request.optimizer_runs,
        request.baseline,
        request.function_text
    );
    if request.context.is_empty() {
        let _ = writeln!(prompt, "It calls no functions.\n");
    } else {
        let _ = writeln!(
            prompt,
            "It may call these functions, and no others:\n\n```\n{}```\n",
            request.context
        );
    }
    let _ = write!(
        prompt,
        "Operations you may use: {}.\n\nReply with a cheaper equivalent in one `mir` block, or \
         `NO_IMPROVEMENT`.",
        request.vocabulary
    );
    prompt
}

/// Renders the prompt answering a verdict.
#[cfg(feature = "llm")]
fn verdict_prompt(verdict: &Verdict, best: CostReport) -> String {
    match verdict {
        Verdict::Accepted { cost } => format!(
            "Accepted: it costs {cost} and is now the best. Send a strictly cheaper equivalent, \
             or `NO_IMPROVEMENT`."
        ),
        Verdict::Rejected { stage, reason, counterexample } => {
            let mut prompt = format!("Rejected at {stage}: {reason}");
            if let Some(counterexample) = counterexample {
                let _ = write!(prompt, "\nInput: {counterexample}");
            }
            let _ = write!(
                prompt,
                "\n\nThe best so far costs {best}. Send a corrected candidate cheaper than that, \
                 or `NO_IMPROVEMENT`."
            );
            prompt
        }
    }
}

/// Returns the proposal in a reply: its only `mir` block, or `NO_IMPROVEMENT` when it has
/// no block.
#[cfg(any(feature = "llm", test))]
fn extract(reply: &str) -> Option<Proposal> {
    let mut blocks = Vec::new();
    let mut lines = reply.lines();
    while let Some(line) = lines.next() {
        let Some(info) = line.trim().strip_prefix("```") else { continue };
        let mut body = String::new();
        let mut closed = false;
        for line in lines.by_ref() {
            if line.trim().starts_with("```") {
                closed = true;
                break;
            }
            body.push_str(line);
            body.push('\n');
        }
        if !closed {
            return None;
        }
        if info.trim() == "mir" {
            blocks.push(body);
        }
    }
    match blocks.as_slice() {
        [block] => Some(Proposal::Candidate(block.clone())),
        [] if reply.lines().any(|line| line.trim() == "NO_IMPROVEMENT") => Some(Proposal::Done),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies() {
        let candidate = |text: &str| Some(Proposal::Candidate(text.to_string()));
        assert_eq!(
            extract("Shifts are cheaper.\n```mir\nfn @f() {\n}\n```\nDone."),
            candidate("fn @f() {\n}\n")
        );
        assert_eq!(extract("```rust\nx\n```\n```mir\ny\n```"), candidate("y\n"));
        assert_eq!(extract("NO_IMPROVEMENT"), Some(Proposal::Done));
        assert_eq!(extract("Nothing left.\n  NO_IMPROVEMENT  \n"), Some(Proposal::Done));
        assert_eq!(extract("```mir\na\n```\n```mir\nb\n```"), None);
        assert_eq!(extract("```mir\nunclosed\n"), None);
        assert_eq!(extract("I would use a shift."), None);
    }
}
