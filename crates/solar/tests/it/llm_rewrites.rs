//! External rewrite checks through the public session and MIR pipeline APIs.

use snapbox::{assert_data_eq, str};
use solar::{
    codegen::{
        llm::{
            LlmError, LlmRewriter, LlmSession, Proposal, RewriteRequest, Stage, Verdict,
            bind_rewriter,
        },
        mir::{Module, pass::run_pipeline},
    },
    config::{CompileOpts, LlmOptimizeMode, OptimizationMode},
    interface::{ColorChoice, Session, source_map::FileName},
    sema::Compiler,
};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct Rewriter {
    candidates: BTreeMap<&'static str, &'static str>,
    requests: Mutex<Vec<(String, String)>>,
    rejected: Arc<Mutex<Vec<Stage>>>,
}

impl LlmRewriter for Rewriter {
    fn session(&self, request: &RewriteRequest) -> Result<Box<dyn LlmSession>, LlmError> {
        self.requests
            .lock()
            .unwrap()
            .push((request.function_name.clone(), request.context.clone()));
        Ok(Box::new(Conversation {
            proposals: self
                .candidates
                .get(request.function_name.as_str())
                .map(|text| text.to_string())
                .into_iter()
                .collect(),
            rejected: Arc::clone(&self.rejected),
        }))
    }
}

struct Conversation {
    proposals: VecDeque<String>,
    rejected: Arc<Mutex<Vec<Stage>>>,
}

impl LlmSession for Conversation {
    fn propose(&mut self, verdict: Option<&Verdict>) -> Result<Proposal, LlmError> {
        if let Some(Verdict::Rejected { stage, .. }) = verdict {
            self.rejected.lock().unwrap().push(*stage);
        }
        Ok(self.proposals.pop_front().map_or(Proposal::Done, Proposal::Candidate))
    }
}

fn rewrite(
    text: &str,
    rewriter: Arc<impl LlmRewriter + 'static>,
    check: impl FnOnce(&Module) + Send,
) {
    rewrite_pipeline(text, rewriter, "llm-optimize", check);
}

fn rewrite_pipeline(
    text: &str,
    rewriter: Arc<impl LlmRewriter + 'static>,
    pipeline: &str,
    check: impl FnOnce(&Module) + Send,
) {
    let mut opts = CompileOpts { optimization: OptimizationMode::Gas, ..Default::default() };
    opts.unstable.llm_optimize = Some(LlmOptimizeMode::Live);
    opts.unstable.llm_samples = Some(64);
    opts.unstable.mir_pipeline = Some(pipeline.into());
    let session = Session::builder().opts(opts).with_buffer_emitter(ColorChoice::Never).build();
    let compiler = Compiler::new(session);
    compiler.enter(|compiler| {
        let _binding = bind_rewriter(compiler.sess(), rewriter);
        let source = compiler
            .sess()
            .source_map()
            .new_source_file(FileName::Custom("rewrite.mir".into()), text)
            .unwrap();
        let mut module = Module::parse(compiler.sess(), &source).unwrap();
        module.set_unsafe_assembly(false);
        let _ = run_pipeline(compiler.gcx(), &mut module, None);
        assert_eq!(
            compiler.dcx().err_count(),
            0,
            "{}",
            compiler.sess().emitted_diagnostics().unwrap()
        );
        check(&module);
    });
}

#[test]
fn proven_rewrites_keep_composed_context_and_spill_guards() {
    let rewriter = Arc::new(Rewriter {
        candidates: BTreeMap::from([
            ("g", "fn @g(arg0: i256) -> i256 { bb0: ret arg0 }"),
            ("f", "fn @f(arg0: i256) -> i256 { bb0: v0 = icall @g, arg0 ret v0 }"),
        ]),
        ..Default::default()
    });
    rewrite(
        "@module Composed
@phase lowered
fn @main() [entry] { bb0: v0 = calldataload 0 v1 = icall @f, v0 mstore 0, v1 returndata 0, 32 }
fn @f(arg0: i256) -> i256 { bb0: v0 = add arg0, 0 v1 = icall @g, v0 ret v1 }
fn @g(arg0: i256) -> i256 { bb0: v0 = add arg0, 0 v1 = mul v0, 1 ret v1 }
",
        Arc::clone(&rewriter),
        |module| {
            assert!(module.has_unsafe_assembly());
            assert_eq!(module.rewrite_digests.len(), 2);
        },
    );
    assert_data_eq!(
        rewriter
            .requests
            .lock()
            .unwrap()
            .iter()
            .map(|(name, context)| format!("{name}\n{context}"))
            .collect::<Vec<_>>()
            .join("\n"),
        str![[r#"
g

f
fn @g(arg0: i256) -> i256 {
  bb0:
    ret arg0
}

"#]]
    );
    assert!(rewriter.rejected.lock().unwrap().is_empty());
}

#[test]
fn agreeing_samples_do_not_replace_an_equivalence_proof() {
    let rewriter = Arc::new(Rewriter {
        candidates: BTreeMap::from([(
            "cube",
            "fn @cube(arg0: i256) -> i256 { bb0: v0 = mul arg0, arg0 v1 = mul v0, arg0 ret v1 }",
        )]),
        ..Default::default()
    });
    rewrite(
        "@module Unproved
@phase lowered
fn @main() [entry] { bb0: v0 = calldataload 0 v1 = icall @cube, v0 mstore 0, v1 returndata 0, 32 }
fn @cube(arg0: i256) -> i256 { bb0: v0 = exp arg0, 3 v1 = add arg0, 0 v2 = add v0, v1 v3 = sub v2, v1 ret v3 }
",
        Arc::clone(&rewriter),
        |module| {
            assert!(module.rewrite_digests.is_empty());
            assert!(!module.has_unsafe_assembly());
        },
    );
    assert_eq!(*rewriter.rejected.lock().unwrap(), [Stage::Proof]);
}

#[test]
fn comparisons_include_dirty_zero_slot_and_unreserved_memory() {
    let rewriter = Arc::new(Rewriter {
        candidates: BTreeMap::from([
            ("zero_slot", "fn @zero_slot() -> i256 { bb0: ret 0 }"),
            ("clear", "fn @clear() -> i256 { bb0: ret 7 }"),
        ]),
        ..Default::default()
    });
    rewrite("@module Dirty
@phase lowered
fn @main() [entry] { bb0: v0 = icall @zero_slot v1 = icall @clear v2 = add v0, v1 mstore 0, v2 returndata 0, 32 }
fn @zero_slot() -> i256 { bb0: v0 = mload 96 ret v0 }
fn @clear() -> i256 { bb0: v0 = mload 64 v1 = add v0, 32 mstore v1, 0 ret 7 }
", Arc::clone(&rewriter), |module| assert!(module.rewrite_digests.is_empty()));
    assert_eq!(*rewriter.rejected.lock().unwrap(), [Stage::Equivalence, Stage::Equivalence]);
}

#[test]
fn storage_sentry_modules_never_reach_the_rewriter() {
    let rewriter = Arc::new(Rewriter::default());
    rewrite(
        "@module Sentry
@phase lowered
fn @main() [entry] { bb0: v0 = calldataload 0 v1 = icall @helper, v0 sstore 0, v1 stop }
fn @helper(arg0: i256) -> i256 { bb0: v0 = exp 2, arg0 ret v0 }
",
        Arc::clone(&rewriter),
        |module| assert!(module.rewrite_digests.is_empty()),
    );
    assert!(rewriter.requests.lock().unwrap().is_empty());
}

#[test]
fn rewrite_choices_bind_metadata_and_embedded_contracts() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("choices.txt");
    let input = dir.path().join("input.json");
    let pipeline = "lower-checks,lower-builtins,lower-arithmetic,lower-abi,lower-abi-encode,lower-dispatch,lower-frame-slots,lower-mapping-slots,lower-memory-objects,lower-slices,lower-alloc,lower-memory-zero,lower-integers,lower-evm-shaped,llm-optimize,none,egraph,llm-optimize";
    let json = serde_json::json!({
        "language": "Solidity",
        "sources": {"C.sol": {"content": "contract C { function f(uint x) external pure returns(uint) { return h(x); } function h(uint x) internal pure returns(uint) { unchecked { return (x + 0) * 1; } } } contract Parent { function create() external returns(C) { return new C(); } } contract Other { function f(uint x) external pure returns(uint) { return h(x); } function h(uint x) internal pure returns(uint) { unchecked { return (x + 0) * 1; } } }"}},
        "settings": {
            "optimizer": {"enabled": true},
            "solar": {"unstable": [format!("mir-pipeline={pipeline}")]},
            "outputSelection": {"*": {"*": ["metadata", "evm.bytecode.object", "evm.deployedBytecode.object", "ethdebug.resources"]}}
        }
    });
    std::fs::write(&input, json.to_string()).unwrap();
    let compile = |enabled: bool| {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_solar"));
        command.args(["--standard-json", input.to_str().unwrap(), "-j2"]);
        if enabled {
            command.args(["-Zllm-optimize=script", "-Zllm-samples=64", "-Zllm-trace"]);
            command.arg(format!("-Zllm-script={}", script.display()));
        }
        let output = command.output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        if enabled {
            let diagnostics = String::from_utf8_lossy(&output.stderr);
            let offers = diagnostics
                .lines()
                .filter(|line| line.split_whitespace().skip(2).take(2).eq(["@h:", "offered"]))
                .count();
            // C and Other each have one h; preparing again or repeating the pass costs no turns.
            assert_eq!(offers, 2, "{diagnostics}");
        }
        let output: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(
            !output["errors"]
                .as_array()
                .is_some_and(|errors| errors.iter().any(|error| error["severity"] == "error")),
            "{output:#}"
        );
        output
    };
    std::fs::write(
        &script,
        "--- function h\n--- candidate\nfn @h(arg0: i256) -> i256 { bb0: ret arg0 }\n",
    )
    .unwrap();
    let first = compile(true);
    let first_id = first["ethdebug"]["compilation"]["id"].clone();
    assert!(first_id.as_str().is_some());
    for selection in [
        serde_json::json!({"*": {"*": ["metadata"]}}),
        serde_json::json!({"*": {"*": ["metadata", "ethdebug.resources"]}}),
        serde_json::json!({"C.sol": {"C": ["metadata", "ethdebug.resources"]}}),
        serde_json::json!({"C.sol": {"Parent": ["metadata", "ethdebug.resources"]}}),
        serde_json::json!({"C.sol": {"Other": ["metadata", "ethdebug.resources"]}}),
        serde_json::json!({"*": {"*": ["ethdebug.resources"]}}),
        serde_json::json!({"*": {"*": ["metadata", "evm.bytecode.ethdebug"]}}),
    ] {
        let mut request = json.clone();
        request["settings"]["outputSelection"] = selection;
        std::fs::write(&input, request.to_string()).unwrap();
        let selected = compile(true);
        if !selected["ethdebug"].is_null() {
            assert_eq!(selected["ethdebug"]["compilation"]["id"], first_id);
        }
        if let Some(contracts) = selected["contracts"]["C.sol"].as_object() {
            for (name, contract) in contracts {
                if let Some(metadata) = contract.get("metadata") {
                    assert_eq!(metadata, &first["contracts"]["C.sol"][name]["metadata"]);
                }
            }
        }
    }
    std::fs::write(&input, json.to_string()).unwrap();
    // A different verified choice at the same host path must receive a different identity.
    std::fs::write(&script, "--- function h\n--- candidate\nfn @h(arg0: i256) -> i256 { bb0: v0 = xor arg0, 0 ret v0 }\n").unwrap();
    let second = compile(true);
    let plain = compile(false);
    for name in ["C", "Parent"] {
        let metadata = |output: &serde_json::Value| {
            serde_json::from_str::<serde_json::Value>(
                output["contracts"]["C.sol"][name]["metadata"].as_str().unwrap(),
            )
            .unwrap()
        };
        let first_meta = metadata(&first);
        let second_meta = metadata(&second);
        assert_eq!(first_meta["settings"]["solar"]["rewrites"].as_array().unwrap().len(), 1);
        assert_eq!(first_meta["settings"]["solar"]["rewrites"][0]["contract"], "C.sol:C");
        assert_ne!(first_meta, second_meta);
        assert!(metadata(&plain)["settings"]["solar"]["rewrites"].is_null());
        let trailer = |output: &serde_json::Value| {
            let code = output["contracts"]["C.sol"][name]["evm"]["deployedBytecode"]["object"]
                .as_str()
                .unwrap();
            let length = usize::from(u16::from_str_radix(&code[code.len() - 4..], 16).unwrap());
            code[code.len() - 4 - 2 * length..].to_owned()
        };
        assert_ne!(trailer(&first), trailer(&second));
    }
    assert_ne!(first["ethdebug"]["compilation"]["id"], second["ethdebug"]["compilation"]["id"]);
}

#[test]
fn rewritten_artifacts_must_fit_the_target_limit() {
    let mut opts = CompileOpts { optimization: OptimizationMode::Gas, ..Default::default() };
    opts.unstable.llm_optimize = Some(LlmOptimizeMode::Live);
    opts.unstable.llm_samples = Some(64);
    opts.unstable.mir_pipeline = Some("llm-optimize".into());
    let session = Session::builder().opts(opts).with_buffer_emitter(ColorChoice::Never).build();
    let compiler = Compiler::new(session);
    let rewriter = Arc::new(Rewriter {
        candidates: BTreeMap::from([("g", "fn @g(arg0: i256) -> i256 { bb0: ret arg0 }")]),
        ..Default::default()
    });
    compiler.enter(|compiler| {
        let _binding = bind_rewriter(compiler.sess(), rewriter);
        let text = format!("@module Oversized\n@phase lowered\n@data\n payload_0: hex\"{}\"\nfn @main() [entry] {{ bb0: datacopy payload_0, 128, 25000 v0 = calldataload 0 v1 = icall @g, v0 mstore 0, v1 returndata 0, 32 }}\nfn @g(arg0: i256) -> i256 {{ bb0: v0 = add arg0, 0 v1 = mul v0, 1 ret v1 }}", "ab".repeat(25000));
        let source = compiler.sess().source_map().new_source_file(FileName::Custom("oversized.mir".into()), text).unwrap();
        let mut module = Module::parse(compiler.sess(), &source).unwrap_or_else(|_| panic!("{}", compiler.sess().emitted_diagnostics().unwrap()));
        let _ = run_pipeline(compiler.gcx(), &mut module, None);
        assert_eq!(module.rewrite_digests.len(), 1);
        assert!(solar::codegen::generate_mir_input_bytecode(compiler.gcx(), &mut module).is_err());
        assert_data_eq!(compiler.sess().emitted_diagnostics().unwrap().to_string(), str![[r#"
error: external rewrites produce [..] bytes of runtime code, exceeding the 24576-byte limit
...
"#]]);
    });
}

#[test]
fn candidates_are_bounded_before_execution() {
    struct Oversized {
        text: String,
        rejected: Arc<Mutex<Vec<Stage>>>,
    }
    impl LlmRewriter for Oversized {
        fn session(&self, _: &RewriteRequest) -> Result<Box<dyn LlmSession>, LlmError> {
            Ok(Box::new(Conversation {
                proposals: VecDeque::from([self.text.clone()]),
                rejected: Arc::clone(&self.rejected),
            }))
        }
    }
    let instructions = (0..257).map(|i| format!("v{i} = add arg0, 0 ")).collect::<String>();
    for text in [
        " ".repeat(256 * 1024 + 1),
        format!("fn @g(arg0: i256) -> i256 {{ bb0: {instructions} ret arg0 }}"),
    ] {
        let rejected = Arc::new(Mutex::new(Vec::new()));
        rewrite(
            "@module Limits\n@phase lowered\nfn @main() [entry] { bb0: v0 = calldataload 0 v1 = icall @g, v0 mstore 0, v1 returndata 0, 32 }\nfn @g(arg0: i256) -> i256 { bb0: v0 = add arg0, 0 v1 = mul v0, 1 ret v1 }",
            Arc::new(Oversized { text, rejected: Arc::clone(&rejected) }),
            |module| assert!(module.rewrite_digests.is_empty()),
        );
        assert_eq!(*rejected.lock().unwrap(), [Stage::Constraints]);
    }
}

#[test]
fn repeated_pipeline_entries_cannot_reopen_rewriter_sessions() {
    let source = "@module Once
@phase lowered
fn @main() [entry] { bb0: v0 = calldataload 0 v1 = icall @g, v0 mstore 0, v1 returndata 0, 32 }
fn @g(arg0: i256) -> i256 { bb0: v0 = add arg0, 0 v1 = mul v0, 1 ret v1 }";
    // Empty and successful sessions both consume the opportunity. Separators and intervening
    // mutations must not reset it, even after analysis caches are invalidated.
    for accepts in [false, true] {
        let rewriter = Arc::new(Rewriter {
            candidates: if accepts {
                BTreeMap::from([("g", "fn @g(arg0: i256) -> i256 { bb0: ret arg0 }")])
            } else {
                BTreeMap::new()
            },
            ..Default::default()
        });
        rewrite_pipeline(
            source,
            Arc::clone(&rewriter),
            "llm-optimize,none,egraph,llm-optimize,none,llm-optimize",
            move |module| assert_eq!(module.rewrite_digests.len(), usize::from(accepts)),
        );
        let requests = rewriter.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].0, "g");
    }
}
