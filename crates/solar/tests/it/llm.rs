//! The `llm-optimize` rewriter interface, as an embedder uses it.

use serde_json::{Value, json};
use solar::{
    codegen::llm::{
        LlmError, LlmRewriter, LlmSession, Proposal, RewriteRequest, Stage, Verdict, set_rewriter,
    },
    config::{CompileOpts, LlmOptimizeMode, UnstableOpts},
};
use std::sync::{Arc, Mutex};

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
