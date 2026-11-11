//! Runs each `run-call` again on a build with `-Zcheck-analyses=true` when
//! `SOLAR_RUN_CALL_CHECK_ANALYSES` is set.
//!
//! `-Zcheck-analyses` turns every fact a MIR optimization relied on into a run-time check that
//! reverts with `AnalysisFactViolated(uint256,uint256,uint256)`. The test runner executes a
//! call as usual, then compiles the test again with the same command and
//! `-Zcheck-analyses=true`, once per command, deploys the checked contract the same way and runs
//! the same call, whether or not the first run ended as the test expects: a fact a wrong result
//! violates often says why, and the test reports both failures. A check that fires is an
//! analysis claiming something false about this execution, and fails the test with the fact
//! decoded. Otherwise the checks change gas and size only, so both runs must succeed or fail
//! alike and return the same data. A call is skipped when only its checked build runs out of gas
//! under an explicit gas limit, exceeds the contract or initcode size limit, or overflows the
//! stack or call depth.
//!
//! The checks are on by default in debug builds of the compiler, so every ordinary compilation
//! passes `-Zcheck-analyses=false`: snapshots do not depend on the compiler's build profile.
//!
//! Tests written in EVM IR have no MIR to check and are skipped.

use super::{
    Artifact, Failure, Outcome, ResolvedCall, display_call, evm_ir, execute, parse_artifacts,
};
use alloy_primitives::{U256, hex};
use evm2::{SpecId, Version, interpreter::InstrStop};
use std::{
    collections::HashMap,
    process::Command,
    sync::{Arc, Mutex, OnceLock},
};
use ui_test::{build_manager::BuildManager, per_test_config::TestConfig};

/// The environment variable that enables the check.
const VARIABLE: &str = "SOLAR_RUN_CALL_CHECK_ANALYSES";

/// The selector of `AnalysisFactViolated(uint256,uint256,uint256)`.
const SELECTOR: [u8; 4] = [0xed, 0x57, 0x2b, 0x27];

/// The contracts a checked build compiled, or why it could not.
type Build = Arc<Result<Vec<Artifact>, String>>;

/// Returns whether the check runs for the test.
pub(super) fn applies(config: &TestConfig) -> bool {
    std::env::var_os(VARIABLE).is_some_and(|value| !value.is_empty() && value != "0")
        && !evm_ir::is_evm_ir(config)
}

/// Runs `call` on the checked build of its contract and compares it with `actual`, the unchecked
/// build's run.
pub(super) fn check(
    config: &TestConfig,
    build_manager: &BuildManager,
    directive: &str,
    call: &ResolvedCall<'_>,
    actual: &Outcome,
    spec_id: SpecId,
) -> Result<(), String> {
    let name = display_call(directive, call.function.as_deref());
    let build = build(config, build_manager);
    let artifacts = build.as_ref().as_ref().map_err(Clone::clone)?;
    let Some(artifact) = artifacts.iter().find(|artifact| artifact.name == call.artifact.name)
    else {
        return Err(format!("the -Zcheck-analyses build has no `{}`", call.artifact.name));
    };
    // The checks grow the code, which can then exceed the initcode size limit.
    if artifact.bytecode.len() + call.constructor_args.len()
        > Version::base(spec_id).max_initcode_size
    {
        return Ok(());
    }
    let checked_call = ResolvedCall {
        artifact,
        function: call.function.clone(),
        constructor_args: call.constructor_args.clone(),
        input: call.input.clone(),
        expected: call.expected.clone(),
        gas_limit: call.gas_limit,
        value: call.value,
    };
    let checked = match execute(&checked_call, spec_id, false) {
        Ok(checked) => checked,
        // Or the contract size limit.
        Err(Failure::Deploy { stop: InstrStop::CreateContractSizeLimit, .. }) => return Ok(()),
        Err(Failure::Deploy { output, .. })
            if let Some(violation) = describe_violation(&output) =>
        {
            return Err(format!("deploying `{}` violates {violation}", call.artifact.name));
        }
        Err(failure) => {
            return Err(format!("`{name}` with -Zcheck-analyses: {}", String::from(failure)));
        }
    };
    // A test may revert with a fact on purpose, from an `assume` it writes.
    if checked.success == actual.success && checked.output == actual.output {
        return Ok(());
    }
    if !checked.success
        && let Some(violation) = describe_violation(&checked.output)
    {
        return Err(format!("`{name}` violates {violation}"));
    }
    // The checks cost gas, and their values deepen each frame's stack.
    if !checked.success
        && (call.gas_limit.is_some() && is_out_of_gas(checked.stop) || is_too_deep(checked.stop))
    {
        return Ok(());
    }
    Err(format!(
        "`{name}` {} without -Zcheck-analyses and {} with it",
        describe(actual),
        describe(&checked)
    ))
}

/// Compiles the test's command again with `-Zcheck-analyses=true`, once per command.
fn build(config: &TestConfig, build_manager: &BuildManager) -> Build {
    static CACHE: Mutex<Option<HashMap<String, Arc<OnceLock<Build>>>>> = Mutex::new(None);
    let mut command = match config.build_command(build_manager) {
        Ok(command) => command,
        Err(error) => return Arc::new(Err(format!("cannot rebuild the command: {error:?}"))),
    };
    // The last `-Zcheck-analyses` wins.
    command.arg("-Zcheck-analyses=true");
    let key = format!("{command:?}");
    // The lock covers the lookup only: the compiles of different commands run in parallel.
    let once = {
        let mut cache = CACHE.lock().unwrap_or_else(|poison| poison.into_inner());
        cache.get_or_insert_with(HashMap::new).entry(key).or_default().clone()
    };
    once.get_or_init(|| Arc::new(run(command))).clone()
}

fn run(mut command: Command) -> Result<Vec<Artifact>, String> {
    let output = command.output().map_err(|error| format!("cannot run the compiler: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "the -Zcheck-analyses build failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    parse_artifacts(&output.stdout)
}

/// Whether a run stopped for lack of gas.
fn is_out_of_gas(stop: InstrStop) -> bool {
    matches!(
        stop,
        InstrStop::OutOfGas
            | InstrStop::MemoryOOG
            | InstrStop::MemoryLimitOOG
            | InstrStop::PrecompileOOG
            | InstrStop::InvalidOperandOOG
            | InstrStop::ReentrancySentryOOG
    )
}

/// Whether a run stopped for lack of stack or call depth.
fn is_too_deep(stop: InstrStop) -> bool {
    matches!(stop, InstrStop::StackOverflow | InstrStop::CallTooDeep)
}

/// Describes how a run ended.
fn describe(outcome: &Outcome) -> String {
    let data = hex::encode_prefixed(&outcome.output);
    if outcome.success {
        format!("returned {data}")
    } else {
        format!("failed with {:?} ({data})", outcome.stop)
    }
}

/// Decodes `AnalysisFactViolated(fact, value, bound)` revert data.
fn describe_violation(data: &[u8]) -> Option<String> {
    let (selector, words) = data.split_first_chunk::<4>()?;
    if *selector != SELECTOR || words.len() != 96 {
        return None;
    }
    let word = |index: usize| U256::from_be_slice(&words[index * 32..][..32]);
    let (fact, value, bound) = (word(0), word(1), word(2));
    let field = |shift: usize| ((fact >> shift) & U256::from(u32::MAX)).to::<u32>();
    let byte = |shift: usize| ((fact >> shift) & U256::from(u8::MAX)).to::<u8>();
    let raw = field(0);
    let operand = if raw == u32::MAX {
        "a value".to_owned()
    } else if raw & (1 << 31) != 0 {
        format!("arg{}", raw & !(1 << 31))
    } else {
        format!("v{raw}")
    };
    let (block, function) = (field(32), field(64));
    let site = format!("at the end of bb{block} in function #{function}");
    let hint = "(see `-Zcheck-analyses=true -Zdump=mir`)";
    // `predicate(lhs, rhs)`, reported with both words.
    let source = match byte(96) {
        1 => "sccp",
        2 => "check-elim",
        3 => "cse",
        4 => "load-pre",
        5 => "indvar-simplify",
        8 => "egraph",
        9 => "const-fold",
        _ => "an unknown pass",
    };
    let predicate = match byte(104) {
        1 => "==",
        2 => "!=",
        3 => "<",
        4 => "<=",
        _ => "?",
    };
    Some(format!(
        "a fact {source} relied on: {operand} {predicate} {bound:#x} {site} is false, \
         {operand} is {value:#x} {hint}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_violations() {
        let mut data = SELECTOR.to_vec();
        let fact = (U256::from(3) << 104)
            | (U256::from(1) << 96)
            | (U256::from(1) << 64)
            | (U256::from(2) << 32);
        for word in [fact, U256::from(20), U256::from(10)] {
            data.extend_from_slice(&word.to_be_bytes::<32>());
        }
        snapbox::assert_data_eq!(
            describe_violation(&data).unwrap(),
            snapbox::str![
                "a fact sccp relied on: v0 < 0xa at the end of bb2 in function #1 is false, v0 is 0x14 (see `-Zcheck-analyses=true -Zdump=mir`)"
            ]
        );
        let argument =
            (U256::from(4) << 104) | (U256::from(2) << 96) | U256::from((1u32 << 31) | 1);
        data.truncate(4);
        for word in [argument, U256::from(9), U256::from(8)] {
            data.extend_from_slice(&word.to_be_bytes::<32>());
        }
        snapbox::assert_data_eq!(
            describe_violation(&data).unwrap(),
            snapbox::str![
                "a fact check-elim relied on: arg1 <= 0x8 at the end of bb0 in function #0 is false, arg1 is 0x9 (see `-Zcheck-analyses=true -Zdump=mir`)"
            ]
        );
        assert!(describe_violation(&[0x4e, 0x48, 0x7b, 0x71]).is_none());
    }
}
