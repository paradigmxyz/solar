//! The `llm-optimize` rewriter interface, as an embedder uses it.

use serde_json::{Value, json};
use solar::{
    codegen::llm::{
        LlmError, LlmRewriter, LlmSession, Proposal, RewriteRequest, Stage, Verdict, bind_rewriter,
        rewriter,
    },
    config::{CompileOpts, CompilerOutput, EvmVersion, LlmOptimizeMode, UnstableOpts},
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[cfg(feature = "llm")]
use snapbox::{assert_data_eq, str};
#[cfg(feature = "llm")]
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    process::{Command, Output},
    thread,
};

const SOURCE: &str =
    include_str!("../../../../tests/ui/codegen/mir/llm-optimize/auxiliary/provider.sol");
// Retain redundant arithmetic so the provider must propose a cheaper, provable body.
pub(super) const PIPELINE: &str = "lower-checks,lower-builtins,lower-arithmetic,lower-abi,lower-abi-encode,lower-dispatch,lower-frame-slots,lower-mapping-slots,lower-memory-objects,lower-slices,lower-alloc,lower-memory-zero,lower-integers,lower-evm-shaped,llm-optimize";

/// Incorrectly adds one after the byte mask.
const WRONG: &str = "fn @identity(arg0: i256) -> i256 [pure] {
  bb0:
    v0 = and arg0, 255
    v1 = add v0, 1
    ret v1
}
";

/// Preserves the mask and removes only redundant arithmetic.
const RIGHT: &str = "fn @identity(arg0: i256) -> i256 [pure] {
  bb0:
    v0 = and arg0, 255
    ret v0
}
";

/// Proposes an incorrect body and then a provably equivalent one, recording the verdicts.
struct Rewriter {
    verdicts: Arc<Mutex<Vec<(String, Verdict)>>>,
}

impl LlmRewriter for Rewriter {
    fn session(&self, request: &RewriteRequest) -> Result<Box<dyn LlmSession>, LlmError> {
        let candidates =
            if request.function_name == "identity" { vec![WRONG, RIGHT] } else { vec![] };
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

/// Compiles `probe.sol` through Standard JSON in `mode`, with `rewriter` bound to the session.
fn compile(mode: Option<LlmOptimizeMode>, rewriter: Option<Arc<dyn LlmRewriter>>) -> Value {
    compile_outputs(
        mode,
        rewriter,
        &["metadata", "evm.bytecode.object", "evm.deployedBytecode.object", "ethdebug.resources"],
    )
}

fn compile_outputs(
    mode: Option<LlmOptimizeMode>,
    rewriter: Option<Arc<dyn LlmRewriter>>,
    outputs: &[&str],
) -> Value {
    let input = json!({
        "language": "Solidity",
        "sources": {"probe.sol": {"content": SOURCE}},
        "settings": {
            "evmVersion": "cancun",
            "optimizer": {"enabled": true, "runs": 200},
            "solar": {"unstable": [format!("mir-pipeline={PIPELINE}")]},
            "outputSelection": {"*": {"*": outputs}}
        }
    });
    let opts = CompileOpts {
        unstable: UnstableOpts { llm_optimize: mode, ..Default::default() },
        ..Default::default()
    };
    let mut output = Vec::new();
    solar::cli::standard_json::compile_standard_json_with(
        &input.to_string(),
        opts,
        None,
        &mut output,
        |sess| rewriter.map(|rewriter| bind_rewriter(sess, rewriter)),
    )
    .unwrap();
    serde_json::from_slice(&output).unwrap()
}

#[test]
fn embedded_rewriter() {
    let runtime = |output: &Value| {
        output["contracts"]["probe.sol"]["Probe"]["evm"]["deployedBytecode"]["object"].clone()
    };
    let plain = compile(None, None);
    let verdicts = Arc::new(Mutex::new(Vec::new()));
    let embedded = Arc::new(Rewriter { verdicts: Arc::clone(&verdicts) });
    let rewritten = compile(Some(LlmOptimizeMode::Live), Some(embedded));

    // No LLM controls occur in the JSON input: only the host enables this rewriter.
    // The embedder stays in place and hears why its first candidate failed.
    let verdicts = verdicts.lock().unwrap();
    let [(first_function, first), (second_function, second)] = verdicts.as_slice() else {
        panic!("expected two verdicts, got {verdicts:?}");
    };
    assert_eq!((first_function.as_str(), second_function.as_str()), ("identity", "identity"));
    let Verdict::Rejected { stage: Stage::Equivalence, counterexample: Some(_), .. } = first else {
        panic!("the incorrect body was not rejected with an input: {first:?}");
    };
    let Verdict::Accepted { cost } = second else {
        panic!("the equivalent body was not accepted: {second:?}");
    };
    assert!(cost.gas > 0 && cost.bytes > 0);
    assert!(runtime(&plain).as_str().is_some_and(|code| !code.is_empty()));
    assert_ne!(runtime(&rewritten), runtime(&plain));
    let metadata = |output: &Value| {
        serde_json::from_str::<Value>(
            output["contracts"]["probe.sol"]["Probe"]["metadata"].as_str().unwrap(),
        )
        .unwrap()
    };
    assert!(metadata(&plain)["settings"]["solar"]["rewrites"].is_null());
    let rewritten_metadata = metadata(&rewritten);
    let decisions = rewritten_metadata["settings"]["solar"]["rewrites"].as_array().unwrap();
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0]["contract"], "probe.sol:Probe");
    assert!(!decisions[0]["digests"].as_array().unwrap().is_empty());
    assert_ne!(rewritten["ethdebug"]["compilation"]["id"], plain["ethdebug"]["compilation"]["id"]);
    let metadata_only = compile_outputs(
        Some(LlmOptimizeMode::Live),
        Some(Arc::new(Rewriter { verdicts: Arc::new(Mutex::new(Vec::new())) })),
        &["metadata", "ethdebug.resources"],
    );
    assert_eq!(metadata(&metadata_only), rewritten_metadata);
    assert_eq!(
        metadata_only["ethdebug"]["compilation"]["id"],
        rewritten["ethdebug"]["compilation"]["id"]
    );
}

/// An embedder that loads the sources itself gets the command line's outputs, and the rewriter it
/// binds to its session stays in place, while another session never sees it.
#[test]
fn embedded_sources() {
    let out = tempfile::tempdir().unwrap();
    let opts = CompileOpts {
        evm_version: EvmVersion::Cancun,
        emit: vec![CompilerOutput::BinRuntime],
        out_dir: Some(out.path().to_path_buf()),
        unstable: UnstableOpts {
            llm_optimize: Some(LlmOptimizeMode::Live),
            mir_pipeline: Some(PIPELINE.into()),
            ..Default::default()
        },
        ..Default::default()
    };
    let verdicts = Arc::new(Mutex::new(Vec::new()));
    let sess = solar::interface::Session::new(opts);
    let binding = bind_rewriter(&sess, Arc::new(Rewriter { verdicts: Arc::clone(&verdicts) }));
    assert!(rewriter(&solar::interface::Session::new(CompileOpts::default())).is_none());
    let compiled = solar::cli::run_compiler_with_sources(sess, |pcx| {
        let file =
            pcx.sess.source_map().new_source_file(PathBuf::from("probe.sol"), SOURCE).unwrap();
        pcx.add_file(file);
        Ok(())
    });
    drop(binding);
    assert!(compiled.is_ok());
    assert_eq!(verdicts.lock().unwrap().len(), 2);
    let combined = std::fs::File::open(out.path().join("combined.json")).unwrap();
    let combined = serde_json::from_reader::<_, Value>(combined).unwrap();
    let runtime = &combined["contracts"]["probe.sol:Probe"]["bin-runtime"];
    assert!(runtime.as_str().is_some_and(|code| !code.is_empty()), "{combined:#}");
}

/// The compiler, built with the features the tests run with.
#[cfg(feature = "llm")]
const SOLAR: &str = env!("CARGO_BIN_EXE_solar");
/// The directory holding `probe.sol`.
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

/// The stand-in model: the equivalent body when first asked about `identity`, and no
/// improvement otherwise.
#[cfg(feature = "llm")]
fn answer(messages: &[Value], prompt: &str) -> String {
    let prompts = messages.iter().filter(|message| message["role"] == "user").count();
    if prompts == 1 && prompt.contains("fn @identity") {
        format!("Keep the byte mask.\n```mir\n{RIGHT}```")
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
            {"type": "thinking", "thinking": "Removing redundant arithmetic.", "signature": "sig"},
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
                "reasoning_content": "Removing redundant arithmetic.",
            },
            "finish_reason": "stop",
        }],
        "usage": {"prompt_tokens": 1000, "completion_tokens": 200, "total_tokens": 1200},
    })
}

/// Compiles `probe.sol` for gas with `args`, giving the compiler only the key in `key`.
#[cfg(feature = "llm")]
fn build(args: &[&str], key: Option<&str>) -> Output {
    let mut command = Command::new(SOLAR);
    command.current_dir(FIXTURES).args([
        "auxiliary/provider.sol",
        "-O",
        "gas",
        "--emit=bin-runtime,ethdebug-resources",
    ]);
    command.arg(format!("-Zmir-pipeline={PIPELINE}"));
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

/// The runtime bytecode and compilation identity of a successful build.
#[cfg(feature = "llm")]
fn artifact(output: &Output) -> Value {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let output = serde_json::from_slice::<Value>(&output.stdout).unwrap();
    let [contract] = &output["contracts"].as_object().unwrap().values().collect::<Vec<_>>()[..]
    else {
        panic!("expected one contract: {output}");
    };
    let identity = &output["ethdebug"]["compilation"]["id"];
    assert!(identity.as_str().is_some());
    json!({"bytecode": contract["bin-runtime"], "compilationId": identity})
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

/// A trusted `-Zllm-endpoint` sends bounded, tool-free Responses requests to that endpoint.
#[cfg(feature = "llm")]
#[test]
fn openai_endpoint() {
    let (url, requests) = serve(|_| json!({"error": {"message": "stand-in"}}), 0, false);
    let endpoint = format!("-Zllm-endpoint={url}");
    let output = build(
        &["-Zllm-optimize=live", "-Zllm-model=openai/gpt-6-sol", &endpoint],
        Some("OPENAI_API_KEY"),
    );
    // A model that fails never fails the build.
    artifact(&output);
    let requests = requests.lock().unwrap();
    assert!(!requests.is_empty(), "{}", String::from_utf8_lossy(&output.stderr));
    for request in requests.iter() {
        assert_eq!(request.path, "/v1/responses");
        assert_eq!(request.header("authorization"), Some("Bearer test-key"));
    }
}

#[cfg(feature = "llm")]
#[test]
fn anthropic_rewrites() {
    let plain = artifact(&build(&[], None));
    let (output, requests) =
        ask("anthropic/claude-opus-5-5", "high", "ANTHROPIC_API_KEY", anthropic_reply);
    assert_ne!(artifact(&output), plain);
    assert_data_eq!(
        String::from_utf8_lossy(&output.stderr).into_owned(),
        str![[r#"
warning: `-Zllm-optimize=live` sends the MIR of offered functions to `127.0.0.1`

llm-optimize Probe @identity: costs 71 gas, 28 bytes; asking anthropic/claude-opus-5-5 for something cheaper
llm-optimize Probe @identity: round 1
  Probe @identity ┆ Removing redundant arithmetic.
  Probe @identity │ Keep the byte mask.
  Probe @identity │ ```mir
  Probe @identity │ fn @identity(arg0: i256) -> i256 [pure] {
  Probe @identity │   bb0:
  Probe @identity │     v0 = and arg0, 255
  Probe @identity │     ret v0
  Probe @identity │ }
  Probe @identity │ ```
llm-optimize Probe @identity: replied in [..] s using 1200 tokens, an estimated $0.008
llm-optimize Probe @identity: accepted at 52 gas, 21 bytes
llm-optimize Probe @identity: round 2
  Probe @identity ┆ Removing redundant arithmetic.
  Probe @identity │ NO_IMPROVEMENT
llm-optimize Probe @identity: replied in [..] s using 1200 tokens, an estimated $0.008
llm-optimize Probe @identity: the model has nothing cheaper
llm-optimize Probe @identity: keeps a rewrite at 52 gas, 21 bytes, down from 71 gas, 28 bytes
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
        json!({"type": "thinking", "thinking": "Removing redundant arithmetic.", "signature": "sig"})
    );
    assert!(verdict["content"][0]["text"].as_str().unwrap().starts_with("Accepted"));
    assert_eq!(verdict["content"][0]["cache_control"], json!({"type": "ephemeral"}));
}

#[cfg(feature = "llm")]
#[test]
fn opencode_rewrites() {
    let plain = artifact(&build(&[], None));
    let (output, requests) =
        ask("opencode/deepseek-v4.1-flash", "max", "OPENCODE_ZEN_API_KEY", chat_reply);
    assert_ne!(artifact(&output), plain);
    assert_data_eq!(
        String::from_utf8_lossy(&output.stderr).into_owned(),
        str![[r#"
warning: `-Zllm-optimize=live` sends the MIR of offered functions to `127.0.0.1`

llm-optimize Probe @identity: costs 71 gas, 28 bytes; asking opencode/deepseek-v4.1-flash for something cheaper
llm-optimize Probe @identity: round 1
  Probe @identity ┆ Removing redundant arithmetic.
  Probe @identity │ Keep the byte mask.
  Probe @identity │ ```mir
  Probe @identity │ fn @identity(arg0: i256) -> i256 [pure] {
  Probe @identity │   bb0:
  Probe @identity │     v0 = and arg0, 255
  Probe @identity │     ret v0
  Probe @identity │ }
  Probe @identity │ ```
llm-optimize Probe @identity: replied in 0.0 s using 1200 tokens, an estimated $0.00054
llm-optimize Probe @identity: accepted at 52 gas, 21 bytes
llm-optimize Probe @identity: round 2
  Probe @identity ┆ Removing redundant arithmetic.
  Probe @identity │ NO_IMPROVEMENT
llm-optimize Probe @identity: replied in 0.0 s using 1200 tokens, an estimated $0.00054
llm-optimize Probe @identity: the model has nothing cheaper
llm-optimize Probe @identity: keeps a rewrite at 52 gas, 21 bytes, down from 71 gas, 28 bytes
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
    assert_eq!(reply["reasoning_content"], "Removing redundant arithmetic.");
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
    let plain = artifact(&build(&[], None));
    // This provider does not stream, so each reply arrives whole.
    let (url, requests) = serve(chat_reply, 2, false);
    let endpoint = format!("-Zllm-endpoint={url}");
    let live = ["-Zllm-optimize=live", "-Zllm-model=opencode/deepseek-v4.1-flash", &endpoint];
    let output = build(&live, Some("OPENCODE_ZEN_API_KEY"));
    // Two rate-limited attempts precede the conversation, which goes on as if they never happened.
    assert_ne!(artifact(&output), plain);
    assert_data_eq!(
        String::from_utf8_lossy(&output.stderr).into_owned(),
        str![[r#"
warning: `-Zllm-optimize=live` sends the MIR of offered functions to `127.0.0.1`

llm-optimize Probe @identity: costs 71 gas, 28 bytes; asking opencode/deepseek-v4.1-flash for something cheaper
llm-optimize Probe @identity: round 1
llm-optimize Probe @identity: OpenCode Zen answered 429 Too Many Requests: slow down; sending again in 0.0 s
llm-optimize Probe @identity: OpenCode Zen answered 429 Too Many Requests: slow down; sending again in 0.0 s
  Probe @identity ┆ Removing redundant arithmetic.
  Probe @identity │ Keep the byte mask.
  Probe @identity │ ```mir
  Probe @identity │ fn @identity(arg0: i256) -> i256 [pure] {
  Probe @identity │   bb0:
  Probe @identity │     v0 = and arg0, 255
  Probe @identity │     ret v0
  Probe @identity │ }
  Probe @identity │ ```
llm-optimize Probe @identity: replied in [..] s using 1200 tokens, an estimated $0.00054
llm-optimize Probe @identity: accepted at 52 gas, 21 bytes
llm-optimize Probe @identity: round 2
  Probe @identity ┆ Removing redundant arithmetic.
  Probe @identity │ NO_IMPROVEMENT
llm-optimize Probe @identity: replied in [..] s using 1200 tokens, an estimated $0.00054
llm-optimize Probe @identity: the model has nothing cheaper
llm-optimize Probe @identity: keeps a rewrite at 52 gas, 21 bytes, down from 71 gas, 28 bytes
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
    assert_eq!(artifact(&second), artifact(&first));
    assert_data_eq!(
        String::from_utf8_lossy(&second.stderr).into_owned(),
        str![[r#"
warning: `-Zllm-optimize=live` sends the MIR of offered functions to `127.0.0.1`

llm-optimize Probe @identity: reuses its cached rewrite at 52 gas, 21 bytes, down from 71 gas, 28 bytes, without asking opencode/deepseek-v4.1-flash

"#]]
    );
}

#[test]
fn standard_json_cannot_enable_live_credentials() {
    for flags in [
        vec!["llm-optimize=live"],
        vec!["llm-optimize=live", "llm-model=anthropic/test", "llm-endpoint=invalid endpoint"],
    ] {
        let input =
            json!({"language":"Solidity", "sources":{}, "settings":{"solar":{"unstable":flags}}});
        let prepared = std::sync::atomic::AtomicBool::new(false);
        let mut output = Vec::new();
        solar::cli::standard_json::compile_standard_json_with(
            &input.to_string(),
            CompileOpts::default(),
            None,
            &mut output,
            |_| {
                prepared.store(true, std::sync::atomic::Ordering::Relaxed);
            },
        )
        .unwrap();
        assert!(!prepared.load(std::sync::atomic::Ordering::Relaxed));
        let output: Value = serde_json::from_slice(&output).unwrap();
        snapbox::assert_data_eq!(
            output["errors"][0]["message"].as_str().unwrap(),
            snapbox::str!["live LLM rewriting must be enabled outside Standard JSON input"]
        );
    }
}

#[test]
fn standard_json_cannot_replace_a_trusted_endpoint() {
    let input = json!({"language":"Solidity", "sources":{}, "settings":{"solar":{"unstable":["llm-optimize=live", "llm-endpoint=http://untrusted.invalid"]}}});
    let opts = CompileOpts {
        unstable: UnstableOpts {
            llm_optimize: Some(LlmOptimizeMode::Live),
            llm_endpoint: Some("https://trusted.invalid".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut output = Vec::new();
    solar::cli::standard_json::compile_standard_json(&input.to_string(), opts, None, &mut output)
        .unwrap();
    let output: Value = serde_json::from_slice(&output).unwrap();
    snapbox::assert_data_eq!(
        output["errors"][0]["message"].as_str().unwrap(),
        snapbox::str!["LLM provider options must be configured outside Standard JSON input"]
    );
}
