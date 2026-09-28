//! The `llm-optimize` rewriter interface, as an embedder uses it.

use serde_json::{Value, json};
use solar::{
    codegen::llm::{
        LlmError, LlmRewriter, LlmSession, Proposal, RewriteRequest, Stage, Verdict, set_rewriter,
    },
    config::{CompileOpts, LlmOptimizeMode, UnstableOpts},
};
use std::sync::{Arc, Mutex};

#[cfg(feature = "llm")]
use snapbox::{assert_data_eq, str};
#[cfg(feature = "llm")]
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    process::{Command, Output},
    thread,
};

const SOURCE: &str = include_str!("../../../../tests/ui/codegen/mir/llm-optimize/triangle.sol");

/// `sumBelow` as `n * (n + 1) / 2`: wrong by `n`.
const WRONG: &str = "fn @sumBelow(arg0: i256) -> i256 [pure] {
  bb0:
    v0 = add arg0, 1
    v1 = mul arg0, v0
    v2 = shr 1, v1
    ret v2
}
";

/// `sumBelow` as `n * (n - 1) / 2`.
const RIGHT: &str = "fn @sumBelow(arg0: i256) -> i256 [pure] {
  bb0:
    v0 = sub arg0, 1
    v1 = mul arg0, v0
    v2 = shr 1, v1
    ret v2
}
";

/// Proposes the wrong closed form and then the right one, recording the verdicts.
struct Rewriter {
    verdicts: Arc<Mutex<Vec<(String, Verdict)>>>,
}

impl LlmRewriter for Rewriter {
    fn session(&self, request: &RewriteRequest) -> Result<Box<dyn LlmSession>, LlmError> {
        let candidates =
            if request.function_name == "sumBelow" { vec![WRONG, RIGHT] } else { vec![] };
        Ok(Box::new(Session {
            function: request.function_name.clone(),
            candidates: candidates.into_iter(),
            verdicts: Arc::clone(&self.verdicts),
        }))
    }
}

struct Session {
    function: String,
    candidates: std::vec::IntoIter<&'static str>,
    verdicts: Arc<Mutex<Vec<(String, Verdict)>>>,
}

impl LlmSession for Session {
    fn propose(&mut self, verdict: Option<&Verdict>) -> Result<Proposal, LlmError> {
        if let Some(verdict) = verdict {
            self.verdicts.lock().unwrap().push((self.function.clone(), verdict.clone()));
        }
        Ok(self.candidates.next().map_or(Proposal::Done, |text| Proposal::Candidate(text.into())))
    }
}

fn compile(mode: Option<LlmOptimizeMode>) -> Value {
    let input = json!({
        "language": "Solidity",
        "sources": {"triangle.sol": {"content": SOURCE}},
        "settings": {
            "evmVersion": "cancun",
            "optimizer": {"enabled": true, "runs": 200},
            "outputSelection": {"*": {"*": ["evm.deployedBytecode.object"]}}
        }
    });
    let opts = CompileOpts {
        unstable: UnstableOpts { llm_optimize: mode, ..Default::default() },
        ..Default::default()
    };
    let mut output = Vec::new();
    solar::cli::standard_json::compile_standard_json(&input.to_string(), opts, None, &mut output)
        .unwrap();
    serde_json::from_slice(&output).unwrap()
}

#[test]
fn embedded_rewriter() {
    let runtime = |output: &Value| {
        output["contracts"]["triangle.sol"]["Triangle"]["evm"]["deployedBytecode"]["object"].clone()
    };
    let plain = compile(None);
    let verdicts = Arc::new(Mutex::new(Vec::new()));
    set_rewriter(Some(Arc::new(Rewriter { verdicts: Arc::clone(&verdicts) })));
    let rewritten = compile(Some(LlmOptimizeMode::Live));
    set_rewriter(None);

    // The embedder's rewriter stays in place, and hears why its first candidate failed.
    let verdicts = verdicts.lock().unwrap();
    let [(first_function, first), (second_function, second)] = verdicts.as_slice() else {
        panic!("expected two verdicts, got {verdicts:?}");
    };
    assert_eq!((first_function.as_str(), second_function.as_str()), ("sumBelow", "sumBelow"));
    let Verdict::Rejected { stage: Stage::Equivalence, counterexample: Some(_), .. } = first else {
        panic!("the wrong closed form was not rejected with an input: {first:?}");
    };
    let Verdict::Accepted { cost } = second else {
        panic!("the right closed form was not accepted: {second:?}");
    };
    assert!(cost.gas > 0 && cost.bytes > 0);
    assert!(runtime(&plain).as_str().is_some_and(|code| !code.is_empty()));
    assert_ne!(runtime(&rewritten), runtime(&plain));
}

/// The compiler, built with the features the tests run with.
#[cfg(feature = "llm")]
const SOLAR: &str = env!("CARGO_BIN_EXE_solar");
/// The directory holding `triangle.sol`.
#[cfg(feature = "llm")]
const FIXTURES: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/ui/codegen/mir/llm-optimize");

/// A request the stand-in provider received.
#[cfg(feature = "llm")]
struct Request {
    path: String,
    headers: Vec<(String, String)>,
    body: Value,
}

#[cfg(feature = "llm")]
impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(header, _)| header == name).map(|(_, value)| value.as_str())
    }

    fn messages(&self) -> &[Value] {
        self.body["messages"].as_array().unwrap()
    }
}

/// Plays a provider on a local port: the first `limited` requests hear that the rate limit is
/// reached, and the others get `reply` of their body, streamed when they ask for a stream and the
/// provider `streams`. Returns the API base URL and the requests, kept before they are answered.
#[cfg(feature = "llm")]
fn serve(
    reply: fn(&Value) -> Value,
    limited: usize,
    streams: bool,
) -> (String, Arc<Mutex<Vec<Request>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&requests);
    thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let Some(request) = read_request(&mut stream) else { continue };
            let mut seen = seen.lock().unwrap();
            let (status, kind, body) = if seen.len() < limited {
                let body = json!({"error": {"message": "slow down"}}).to_string();
                ("429 Too Many Requests\r\nretry-after: 0", "application/json", body)
            } else if streams && request.body["stream"] == true {
                let body = events(&request.path, &reply(&request.body));
                ("200 OK", "text/event-stream", body)
            } else {
                ("200 OK", "application/json", reply(&request.body).to_string())
            };
            seen.push(request);
            drop(seen);
            let head = format!(
                "HTTP/1.1 {status}\r\ncontent-type: {kind}\r\ncontent-length: {}\r\n\
                 connection: close\r\n\r\n",
                body.len()
            );
            let _ =
                stream.write_all(head.as_bytes()).and_then(|()| stream.write_all(body.as_bytes()));
        }
    });
    (url, requests)
}

#[cfg(feature = "llm")]
fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let path = line.split_whitespace().nth(1)?.to_string();
    let mut headers = Vec::new();
    loop {
        line.clear();
        reader.read_line(&mut line).ok()?;
        let Some((name, value)) = line.trim_end().split_once(':') else { break };
        headers.push((name.to_ascii_lowercase(), value.trim().to_string()));
    }
    let length = headers.iter().find(|(name, _)| name == "content-length")?.1.parse().ok()?;
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    Some(Request { path, headers, body: serde_json::from_slice(&body).ok()? })
}

/// The server-sent events that stream `reply`, the whole answer to a request for `path`, with
/// every text split in two.
#[cfg(feature = "llm")]
fn events(path: &str, reply: &Value) -> String {
    let mut events = String::new();
    let mut event = |name: Option<&str>, data: Value| {
        if let Some(name) = name {
            events.push_str(&format!("event: {name}\n"));
        }
        events.push_str(&format!("data: {data}\n\n"));
    };
    let halves = |text: &Value| {
        let text = text.as_str().unwrap_or_default();
        let middle = text.char_indices().map(|(i, _)| i).nth(text.chars().count() / 2);
        let (first, second) = text.split_at(middle.unwrap_or(text.len()));
        [first.to_string(), second.to_string()].into_iter().filter(|half| !half.is_empty())
    };
    if path.ends_with("/messages") {
        let start = json!({"type": "message_start", "message": {"usage": reply["usage"]}});
        event(Some("message_start"), start);
        for (index, block) in reply["content"].as_array().unwrap().iter().enumerate() {
            let (kind, field) = match block["type"].as_str().unwrap() {
                "thinking" => ("thinking_delta", "thinking"),
                _ => ("text_delta", "text"),
            };
            let mut empty = block.clone();
            empty[field] = json!("");
            let start =
                json!({"type": "content_block_start", "index": index, "content_block": empty});
            event(Some("content_block_start"), start);
            for half in halves(&block[field]) {
                let delta = json!({"type": kind, field: half});
                let data = json!({"type": "content_block_delta", "index": index, "delta": delta});
                event(Some("content_block_delta"), data);
            }
            let stop = json!({"type": "content_block_stop", "index": index});
            event(Some("content_block_stop"), stop);
        }
        let usage = json!({"output_tokens": reply["usage"]["output_tokens"]});
        let delta = json!({"stop_reason": reply["stop_reason"]});
        event(
            Some("message_delta"),
            json!({"type": "message_delta", "delta": delta, "usage": usage}),
        );
        event(Some("message_stop"), json!({"type": "message_stop"}));
    } else {
        let choice = &reply["choices"][0];
        for (field, text) in [
            ("reasoning_content", &choice["message"]["reasoning_content"]),
            ("content", &choice["message"]["content"]),
        ] {
            for half in halves(text) {
                event(None, json!({"choices": [{"index": 0, "delta": {field: half}}]}));
            }
        }
        let finish = json!({"index": 0, "delta": {}, "finish_reason": choice["finish_reason"]});
        event(None, json!({"choices": [finish]}));
        event(None, json!({"choices": [], "usage": reply["usage"]}));
        events.push_str("data: [DONE]\n\n");
    }
    events
}

/// The stand-in model: the right closed form when first asked about `sumBelow`, and no
/// improvement otherwise.
#[cfg(feature = "llm")]
fn answer(messages: &[Value], prompt: &str) -> String {
    let prompts = messages.iter().filter(|message| message["role"] == "user").count();
    if prompts == 1 && prompt.contains("fn @sumBelow") {
        format!("The loop sums an arithmetic series.\n```mir\n{RIGHT}```")
    } else {
        "NO_IMPROVEMENT".into()
    }
}

/// Anthropic's reply to a Messages request, with a reasoning block before the text.
#[cfg(feature = "llm")]
fn anthropic_reply(body: &Value) -> Value {
    let messages = body["messages"].as_array().unwrap();
    let prompt = messages.last().unwrap()["content"][0]["text"].as_str().unwrap();
    json!({
        "id": "msg_test",
        "type": "message",
        "role": "assistant",
        "model": body["model"],
        "content": [
            {"type": "thinking", "thinking": "Summing an arithmetic series.", "signature": "sig"},
            {"type": "text", "text": answer(messages, prompt)},
        ],
        "stop_reason": "end_turn",
        "usage": {
            "input_tokens": 1000,
            "output_tokens": 200,
            "cache_creation_input_tokens": 0,
            "cache_read_input_tokens": 0,
        },
    })
}

/// A chat completions reply, with DeepSeek's reasoning field.
#[cfg(feature = "llm")]
fn chat_reply(body: &Value) -> Value {
    let messages = body["messages"].as_array().unwrap();
    let prompt = messages.last().unwrap()["content"].as_str().unwrap();
    json!({
        "id": "chat_test",
        "object": "chat.completion",
        "model": body["model"],
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": answer(messages, prompt),
                "reasoning_content": "Summing an arithmetic series.",
            },
            "finish_reason": "stop",
        }],
        "usage": {"prompt_tokens": 1000, "completion_tokens": 200, "total_tokens": 1200},
    })
}

/// Compiles `triangle.sol` for gas with `args`, giving the compiler only the key in `key`.
#[cfg(feature = "llm")]
fn build(args: &[&str], key: Option<&str>) -> Output {
    let mut command = Command::new(SOLAR);
    command.current_dir(FIXTURES).args(["triangle.sol", "-O", "gas", "--emit=bin-runtime"]);
    command.args(["--threads", "1", "--evm-version", "cancun", "--allow", "2264", "-Zui-testing"]);
    command.args(args);
    for variable in ["OPENAI_API_KEY", "ANTHROPIC_API_KEY", "OPENCODE_ZEN_API_KEY"] {
        command.env_remove(variable);
    }
    if let Some(key) = key {
        command.env(key, "test-key");
    }
    command.output().unwrap()
}

/// The runtime bytecode of a successful build.
#[cfg(feature = "llm")]
fn runtime(output: &Output) -> Value {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let output = serde_json::from_slice::<Value>(&output.stdout).unwrap();
    let [contract] = &output["contracts"].as_object().unwrap().values().collect::<Vec<_>>()[..]
    else {
        panic!("expected one contract: {output}");
    };
    contract["bin-runtime"].clone()
}

/// Asks `model` at a local stand-in for its provider, returning the build and the requests.
#[cfg(feature = "llm")]
fn ask(model: &str, effort: &str, key: &str, reply: fn(&Value) -> Value) -> (Output, Vec<Request>) {
    let (url, requests) = serve(reply, 0, true);
    let model = format!("-Zllm-model={model}");
    let endpoint = format!("-Zllm-endpoint={url}");
    let effort = format!("-Zllm-effort={effort}");
    let output = build(&["-Zllm-optimize=live", &model, &endpoint, &effort], Some(key));
    let requests = std::mem::take(&mut *requests.lock().unwrap());
    (output, requests)
}

#[cfg(feature = "llm")]
#[test]
fn anthropic_rewrites() {
    let plain = runtime(&build(&[], None));
    let (output, requests) =
        ask("anthropic/claude-opus-5-5", "high", "ANTHROPIC_API_KEY", anthropic_reply);
    assert_ne!(runtime(&output), plain);
    assert_data_eq!(
        String::from_utf8_lossy(&output.stderr).into_owned(),
        str![[r#"
warning: `-Zllm-optimize=live` sends the MIR of offered functions to Anthropic

llm-optimize Triangle @sumBelow: costs 7540 gas, 43 bytes; asking anthropic/claude-opus-5-5 for something cheaper
llm-optimize Triangle @sumBelow: round 1
  Triangle @sumBelow ┆ Summing an arithmetic series.
  Triangle @sumBelow │ The loop sums an arithmetic series.
  Triangle @sumBelow │ ```mir
  Triangle @sumBelow │ fn @sumBelow(arg0: i256) -> i256 [pure] {
  Triangle @sumBelow │   bb0:
  Triangle @sumBelow │     v0 = sub arg0, 1
  Triangle @sumBelow │     v1 = mul arg0, v0
  Triangle @sumBelow │     v2 = shr 1, v1
  Triangle @sumBelow │     ret v2
  Triangle @sumBelow │ }
  Triangle @sumBelow │ ```
llm-optimize Triangle @sumBelow: replied in [..] s using 1200 tokens, an estimated $0.008
llm-optimize Triangle @sumBelow: accepted at 72 gas, 28 bytes
llm-optimize Triangle @sumBelow: round 2
  Triangle @sumBelow ┆ Summing an arithmetic series.
  Triangle @sumBelow │ NO_IMPROVEMENT
llm-optimize Triangle @sumBelow: replied in [..] s using 1200 tokens, an estimated $0.008
llm-optimize Triangle @sumBelow: the model has nothing cheaper
llm-optimize Triangle @sumBelow: keeps a rewrite at 72 gas, 28 bytes, down from 7540 gas, 43 bytes
note: `llm-optimize` asked Anthropic 2 turns using 2400 tokens, an estimated $0.016


"#]]
    );

    let first = &requests[0];
    assert_eq!(first.path, "/v1/messages");
    assert_eq!(first.header("x-api-key"), Some("test-key"));
    assert_eq!(first.header("anthropic-version"), Some("2023-06-01"));
    assert_eq!(first.body["model"], "claude-opus-5-5");
    assert_eq!(first.body["thinking"], json!({"type": "adaptive"}));
    assert_eq!(first.body["output_config"], json!({"effort": "high"}));
    assert_eq!(first.body["system"][0]["cache_control"], json!({"type": "ephemeral"}));
    // The verdict on the rewrite follows the reply it answers, reasoning block and all, and only
    // the newest prompt is a cache breakpoint.
    let verdict = requests.iter().find(|request| request.messages().len() == 3).unwrap();
    let [first_prompt, reply, verdict] = verdict.messages() else { unreachable!() };
    assert!(first_prompt["content"][0].get("cache_control").is_none());
    assert_eq!(
        reply["content"][0],
        json!({"type": "thinking", "thinking": "Summing an arithmetic series.", "signature": "sig"})
    );
    assert!(verdict["content"][0]["text"].as_str().unwrap().starts_with("Accepted"));
    assert_eq!(verdict["content"][0]["cache_control"], json!({"type": "ephemeral"}));
}

#[cfg(feature = "llm")]
#[test]
fn opencode_rewrites() {
    let plain = runtime(&build(&[], None));
    let (output, requests) =
        ask("opencode/deepseek-v4.1-flash", "max", "OPENCODE_ZEN_API_KEY", chat_reply);
    assert_ne!(runtime(&output), plain);
    assert_data_eq!(
        String::from_utf8_lossy(&output.stderr).into_owned(),
        str![[r#"
warning: `-Zllm-optimize=live` sends the MIR of offered functions to OpenCode Zen

llm-optimize Triangle @sumBelow: costs 7540 gas, 43 bytes; asking opencode/deepseek-v4.1-flash for something cheaper
llm-optimize Triangle @sumBelow: round 1
  Triangle @sumBelow ┆ Summing an arithmetic series.
  Triangle @sumBelow │ The loop sums an arithmetic series.
  Triangle @sumBelow │ ```mir
  Triangle @sumBelow │ fn @sumBelow(arg0: i256) -> i256 [pure] {
  Triangle @sumBelow │   bb0:
  Triangle @sumBelow │     v0 = sub arg0, 1
  Triangle @sumBelow │     v1 = mul arg0, v0
  Triangle @sumBelow │     v2 = shr 1, v1
  Triangle @sumBelow │     ret v2
  Triangle @sumBelow │ }
  Triangle @sumBelow │ ```
llm-optimize Triangle @sumBelow: replied in [..] s using 1200 tokens, an estimated $0.00054
llm-optimize Triangle @sumBelow: accepted at 72 gas, 28 bytes
llm-optimize Triangle @sumBelow: round 2
  Triangle @sumBelow ┆ Summing an arithmetic series.
  Triangle @sumBelow │ NO_IMPROVEMENT
llm-optimize Triangle @sumBelow: replied in [..] s using 1200 tokens, an estimated $0.00054
llm-optimize Triangle @sumBelow: the model has nothing cheaper
llm-optimize Triangle @sumBelow: keeps a rewrite at 72 gas, 28 bytes, down from 7540 gas, 43 bytes
note: `llm-optimize` asked OpenCode Zen 2 turns using 2400 tokens, an estimated $0.00108


"#]]
    );

    let first = &requests[0];
    assert_eq!(first.path, "/v1/chat/completions");
    assert_eq!(first.header("authorization"), Some("Bearer test-key"));
    assert_eq!(first.body["model"], "deepseek-v4.1-flash");
    assert_eq!(first.body["reasoning_effort"], "max");
    assert_eq!(first.messages()[0]["role"], "system");
    // The verdict on the rewrite follows the reply it answers, with the reasoning behind it.
    let verdict = requests.iter().find(|request| request.messages().len() == 4).unwrap();
    let [_, _, reply, verdict] = verdict.messages() else { unreachable!() };
    assert_eq!(reply["reasoning_content"], "Summing an arithmetic series.");
    assert!(verdict["content"].as_str().unwrap().starts_with("Accepted"));
}

#[cfg(feature = "llm")]
#[test]
fn chat_provider_errors() {
    let live = ["-Zllm-optimize=live", "-Zllm-model=opencode/deepseek-v4.1-flash"];
    let missing_key = build(&live, None);
    assert!(!missing_key.status.success());
    assert_data_eq!(
        String::from_utf8_lossy(&missing_key.stderr).into_owned(),
        str![[r#"
error: `-Zllm-optimize=live` with OpenCode Zen requires `OPENCODE_ZEN_API_KEY`


"#]]
    );
    let medium =
        build(&[&live[..], &["-Zllm-effort=medium"]].concat(), Some("OPENCODE_ZEN_API_KEY"));
    assert!(!medium.status.success());
    assert_data_eq!(
        String::from_utf8_lossy(&medium.stderr).into_owned(),
        str![[r#"
error: `deepseek-v4.1-flash` does not take `-Zllm-effort=medium`
   │
   ╰ note: it takes `low`, `high`, `max`


"#]]
    );
}

#[cfg(feature = "llm")]
#[test]
fn chat_provider_retries() {
    let plain = runtime(&build(&[], None));
    // This provider does not stream, so each reply arrives whole.
    let (url, requests) = serve(chat_reply, 2, false);
    let endpoint = format!("-Zllm-endpoint={url}");
    let live = ["-Zllm-optimize=live", "-Zllm-model=opencode/deepseek-v4.1-flash", &endpoint];
    let output = build(&live, Some("OPENCODE_ZEN_API_KEY"));
    // Two rate-limited attempts precede the conversation, which goes on as if they never happened.
    assert_ne!(runtime(&output), plain);
    assert_data_eq!(
        String::from_utf8_lossy(&output.stderr).into_owned(),
        str![[r#"
warning: `-Zllm-optimize=live` sends the MIR of offered functions to OpenCode Zen

llm-optimize Triangle @sumBelow: costs 7540 gas, 43 bytes; asking opencode/deepseek-v4.1-flash for something cheaper
llm-optimize Triangle @sumBelow: round 1
llm-optimize Triangle @sumBelow: OpenCode Zen answered 429 Too Many Requests: slow down; sending again in 0.0 s
llm-optimize Triangle @sumBelow: OpenCode Zen answered 429 Too Many Requests: slow down; sending again in 0.0 s
  Triangle @sumBelow ┆ Summing an arithmetic series.
  Triangle @sumBelow │ The loop sums an arithmetic series.
  Triangle @sumBelow │ ```mir
  Triangle @sumBelow │ fn @sumBelow(arg0: i256) -> i256 [pure] {
  Triangle @sumBelow │   bb0:
  Triangle @sumBelow │     v0 = sub arg0, 1
  Triangle @sumBelow │     v1 = mul arg0, v0
  Triangle @sumBelow │     v2 = shr 1, v1
  Triangle @sumBelow │     ret v2
  Triangle @sumBelow │ }
  Triangle @sumBelow │ ```
llm-optimize Triangle @sumBelow: replied in [..] s using 1200 tokens, an estimated $0.00054
llm-optimize Triangle @sumBelow: accepted at 72 gas, 28 bytes
llm-optimize Triangle @sumBelow: round 2
  Triangle @sumBelow ┆ Summing an arithmetic series.
  Triangle @sumBelow │ NO_IMPROVEMENT
llm-optimize Triangle @sumBelow: replied in [..] s using 1200 tokens, an estimated $0.00054
llm-optimize Triangle @sumBelow: the model has nothing cheaper
llm-optimize Triangle @sumBelow: keeps a rewrite at 72 gas, 28 bytes, down from 7540 gas, 43 bytes
note: `llm-optimize` asked OpenCode Zen 2 turns using 2400 tokens, an estimated $0.00108


"#]]
    );
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[0].body, requests[2].body);
}

#[cfg(feature = "llm")]
#[test]
fn cached_rewrites_skip_the_model() {
    let dir = tempfile::tempdir().unwrap();
    let cache = format!("-Zllm-cache={}", dir.path().display());
    let (url, requests) = serve(chat_reply, 0, true);
    let endpoint = format!("-Zllm-endpoint={url}");
    let model = "-Zllm-model=opencode/deepseek-v4.1-flash";
    let live = ["-Zllm-optimize=live", model, &endpoint, &cache];
    let first = build(&live, Some("OPENCODE_ZEN_API_KEY"));
    let asked = requests.lock().unwrap().len();
    assert!(asked > 0);
    // The second build finds the rewrite in the cache and asks nothing, which it says.
    let second = build(&live, Some("OPENCODE_ZEN_API_KEY"));
    assert_eq!(requests.lock().unwrap().len(), asked);
    assert_eq!(runtime(&second), runtime(&first));
    assert_data_eq!(
        String::from_utf8_lossy(&second.stderr).into_owned(),
        str![[r#"
warning: `-Zllm-optimize=live` sends the MIR of offered functions to OpenCode Zen

llm-optimize Triangle @sumBelow: reuses its cached rewrite at 72 gas, 28 bytes, down from 7540 gas, 43 bytes, without asking opencode/deepseek-v4.1-flash

"#]]
    );
}
