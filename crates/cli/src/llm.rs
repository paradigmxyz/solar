//! The model behind `-Zllm-optimize=live`.
//!
//! With the `llm` feature, `install` registers a [`solar_codegen::llm::LlmRewriter`] that opens
//! one nanocodex agent per offered function. Each agent gets the rewriting brief in
//! `llm/instructions.md` as its instructions, no tools, and a fixed execution environment, so it
//! cannot read files, run commands, or search, and sees neither the host's date nor its
//! `AGENTS.md`. Prompts carry the function, its callees, the objective, and the verdict on the
//! previous candidate; each reply must hold one fenced `mir` block or `NO_IMPROVEMENT`, and a reply
//! with neither gets one reminder.
//!
//! nanocodex runs on Tokio. The rewriter owns a runtime of its own and hands it every turn through
//! a channel, so compilation threads wait for replies without entering an async context, whatever
//! runtime an embedder runs. At most four turns run at once, each may take ten minutes, and no
//! turn starts once the estimated spend reaches five dollars. The key comes from `OPENAI_API_KEY`
//! and goes nowhere but the client. When compilation ends, a note reports turns, tokens, and the
//! estimated cost.

use solar_config::LlmOptimizeMode;
use solar_interface::{Result, Session};

#[cfg(feature = "llm")]
use nanocodex::{
    AgentEvents, Model, Nanocodex, OpenAi, Tools, UsdAmount, agent::ExecutionEnvironment,
};
#[cfg(feature = "llm")]
use solar_codegen::llm::{
    CostReport, LlmError, LlmRewriter, LlmSession, RewriteRequest, Verdict, set_rewriter,
};
#[cfg(feature = "llm")]
use std::{
    fmt::Write,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};
#[cfg(feature = "llm")]
use tokio::{runtime::Runtime, sync::Semaphore};

#[cfg(any(feature = "llm", test))]
use solar_codegen::llm::Proposal;

/// The rewriting brief agents follow.
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
/// The date agents see: a fixed environment keeps host context out of prompts.
#[cfg(feature = "llm")]
const DATE: &str = "2026-01-01";
/// The reply to a reply that held no candidate.
#[cfg(feature = "llm")]
const FORMAT_REMINDER: &str = "Your reply held no candidate. Reply with exactly one fenced code \
                               block tagged `mir` holding the whole function, or with the single \
                               line `NO_IMPROVEMENT`.";

/// Installs the rewriter `-Zllm-optimize=live` asks, returning it for [`Installed::finish`].
pub(crate) fn install(sess: &Session) -> Result<Option<Installed>> {
    if sess.opts.unstable.llm_optimize != Some(LlmOptimizeMode::Live) {
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

/// An installed rewriter and the runtime its agents run on.
#[cfg(feature = "llm")]
pub(crate) struct Installed {
    shared: Arc<Shared>,
    runtime: Runtime,
}

#[cfg(feature = "llm")]
impl Installed {
    fn new(sess: &Session) -> Result<Self> {
        // The key goes to the client alone: never to diagnostics, traces, or the cache.
        let Ok(key) = std::env::var("OPENAI_API_KEY") else {
            let message = "`-Zllm-optimize=live` requires `OPENAI_API_KEY`";
            return Err(sess.dcx.err(message).emit());
        };
        let model = match sess.opts.unstable.llm_model.as_deref() {
            Some(model) => Some(
                model
                    .parse::<Model>()
                    .map_err(|error| sess.dcx.err("unknown `-Zllm-model`").note(error).emit())?,
            ),
            None => None,
        };
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("solar-llm")
            .enable_all()
            .build()
            .map_err(|error| {
                sess.dcx.err(format!("cannot start the model runtime: {error}")).emit()
            })?;
        let openai = wait(runtime.handle(), async move { OpenAi::new(key) }, TURN_TIMEOUT)
            .map_err(|error| error.to_string())
            .and_then(|client| client.map_err(|error| error.to_string()))
            .map_err(|error| {
                sess.dcx.err(format!("cannot configure the model client: {error}")).emit()
            })?;
        sess.dcx
            .warn("`-Zllm-optimize=live` sends the MIR of offered functions to the model provider")
            .emit();
        let shared = Arc::new(Shared {
            runtime: runtime.handle().clone(),
            openai,
            model,
            turns: Arc::new(Semaphore::new(MAX_TURNS)),
            asked: AtomicU64::new(0),
            tokens: AtomicU64::new(0),
            spent_nano_usd: AtomicU64::new(0),
        });
        set_rewriter(Some(Arc::new(Rewriter(Arc::clone(&shared)))));
        Ok(Self { shared, runtime })
    }

    /// Removes the rewriter and reports what it asked.
    pub(crate) fn finish(self, sess: &Session) {
        set_rewriter(None);
        let asked = self.shared.asked.load(Ordering::Relaxed);
        if asked != 0 {
            let tokens = self.shared.tokens.load(Ordering::Relaxed);
            let spent = self.shared.spent_nano_usd.load(Ordering::Relaxed);
            let dollars = UsdAmount::from_nano_usd(spent);
            let message = format!(
                "`llm-optimize` asked {asked} turns using {tokens} tokens, an estimated {dollars}"
            );
            sess.dcx.note(message).emit();
        }
        self.runtime.shutdown_timeout(Duration::from_secs(5));
    }
}

/// State every session shares.
#[cfg(feature = "llm")]
struct Shared {
    runtime: tokio::runtime::Handle,
    openai: OpenAi,
    model: Option<Model>,
    turns: Arc<Semaphore>,
    asked: AtomicU64,
    tokens: AtomicU64,
    spent_nano_usd: AtomicU64,
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
        let (openai, model) = (shared.openai.clone(), shared.model);
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
                builder.build().map_err(|error| error.to_string())
            })?
            .map_err(LlmError::new)?;
        let best = request.baseline;
        Ok(Box::new(Conversation {
            shared,
            agent,
            _events: events,
            request: request.clone(),
            best,
        }))
    }
}

/// One agent's conversation about one function.
#[cfg(feature = "llm")]
struct Conversation {
    shared: Arc<Shared>,
    agent: Nanocodex,
    /// Kept open so the agent can publish its events.
    _events: AgentEvents,
    request: RewriteRequest,
    best: CostReport,
}

#[cfg(feature = "llm")]
impl LlmSession for Conversation {
    fn propose(&mut self, verdict: Option<&Verdict>) -> Result<Proposal, LlmError> {
        let prompt = match verdict {
            None => first_prompt(&self.request),
            Some(verdict) => {
                if let Verdict::Accepted { cost } = verdict {
                    self.best = *cost;
                }
                verdict_prompt(verdict, self.best)
            }
        };
        if let Some(proposal) = extract(&self.turn(prompt)?) {
            return Ok(proposal);
        }
        extract(&self.turn(FORMAT_REMINDER.to_string())?)
            .ok_or_else(|| LlmError::new("the model's reply held no candidate"))
    }
}

#[cfg(feature = "llm")]
impl Conversation {
    /// Sends `prompt` and returns the reply.
    fn turn(&self, prompt: String) -> Result<String, LlmError> {
        let shared = &self.shared;
        if shared.spent_nano_usd.load(Ordering::Relaxed) >= BUDGET_NANO_USD {
            return Err(LlmError::new("the estimated spend reached the budget"));
        }
        let (agent, turns) = (self.agent.clone(), Arc::clone(&shared.turns));
        let result = shared
            .run(async move {
                let _permit = turns.acquire_owned().await;
                agent.prompt(prompt).await?.await
            })?
            .map_err(|error| LlmError::new(error.to_string()))?;
        shared.asked.fetch_add(1, Ordering::Relaxed);
        if let Some(usage) = result.usage() {
            shared.tokens.fetch_add(usage.total_tokens(), Ordering::Relaxed);
            if let Some(cost) = usage.estimated_cost() {
                shared.spent_nano_usd.fetch_add(cost.amount().nano_usd(), Ordering::Relaxed);
            }
        }
        Ok(result.into_final_message())
    }
}

#[cfg(feature = "llm")]
impl Drop for Conversation {
    fn drop(&mut self) {
        let agent = self.agent.clone();
        self.shared.runtime.spawn(async move {
            let _ = agent.shutdown().await;
        });
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
