use alloy_consensus::{TxLegacy, transaction::Recovered};
use alloy_dyn_abi::{FunctionExt, JsonAbiExt, Specifier};
use alloy_json_abi::{Function, JsonAbi};
use alloy_primitives::{Address, Bytes, TxKind, U256, hex};
use evm2::{
    BaseEvmTypes, Evm, Precompiles, SpecId, TxResult,
    env::BlockEnv,
    ethereum::{RecoveredTxEnvelope, ethereum_tx_registry},
    evm::{AccountInfo, InMemoryDB},
};
use serde_json::Value;
use std::path::Path;
use ui_test::{
    CommentParser, Errored, Revisioned,
    build_manager::BuildManager,
    custom_flags::Flag,
    per_test_config::TestConfig,
    spanned::{Span, Spanned},
};

const CALLER: Address = Address::repeat_byte(0x22);
const GAS_LIMIT: u64 = 10_000_000;

#[derive(Debug, Clone)]
pub(crate) struct RunCall {
    call: String,
    expected: String,
}

#[derive(Debug, Clone)]
pub(crate) struct RunCallFail {
    call: String,
    expected: String,
}

#[derive(Debug)]
struct Artifact {
    name: String,
    abi: JsonAbi,
    bytecode: Vec<u8>,
}

struct Outcome {
    success: bool,
    output: Vec<u8>,
    stop: String,
}

struct ResolvedCall<'a> {
    artifact: &'a Artifact,
    function: Option<&'a Function>,
    input: Vec<u8>,
    expected: Vec<u8>,
}

pub(crate) fn is_directive(line: &str) -> bool {
    let Some(mut directive) = line.trim_start().strip_prefix("//@") else {
        return false;
    };
    directive = directive.trim_start();
    if let Some(revision) = directive.strip_prefix('[')
        && let Some((_, rest)) = revision.split_once(']')
    {
        directive = rest.trim_start();
    }
    directive.starts_with("run-call:") || directive.starts_with("run-call-fail:")
}

impl RunCall {
    pub(crate) const NAME: &'static str = "run-call";
    pub(crate) const DEFAULT: Option<Self> = None;

    pub(crate) fn parse(
        parser: &mut CommentParser<&mut Revisioned>,
        args: Spanned<&str>,
        span: Span,
    ) {
        match parse_call(*args) {
            Ok((call, expected)) => parser.add_custom_spanned(
                Self::NAME,
                Self { call: call.to_owned(), expected: expected.to_owned() },
                span,
            ),
            Err(err) => parser.error(args.span(), err),
        }
    }

    fn run(&self, output: &[u8], test_path: &Path, spec_id: SpecId) -> Result<(), String> {
        let artifacts = parse_artifacts(output)?;
        let call = resolve_call(&artifacts, test_path, &self.call, &self.expected, false)?;
        let setup = setup_call(call.artifact, call.function)?;
        let actual = execute(&call.artifact.bytecode, setup, call.input, spec_id)?;
        if !actual.success {
            return Err(format!(
                "`{}` failed with {}: 0x{}",
                display_call(&self.call, call.function),
                actual.stop,
                hex::encode(actual.output)
            ));
        }
        if actual.output != call.expected {
            return Err(format!(
                "`{}` returned 0x{}, expected 0x{}",
                display_call(&self.call, call.function),
                hex::encode(actual.output),
                hex::encode(call.expected)
            ));
        }
        Ok(())
    }
}

impl RunCallFail {
    pub(crate) const NAME: &'static str = "run-call-fail";
    pub(crate) const DEFAULT: Option<Self> = None;

    pub(crate) fn parse(
        parser: &mut CommentParser<&mut Revisioned>,
        args: Spanned<&str>,
        span: Span,
    ) {
        match parse_call(*args) {
            Ok((call, expected)) => parser.add_custom_spanned(
                Self::NAME,
                Self { call: call.to_owned(), expected: expected.to_owned() },
                span,
            ),
            Err(err) => parser.error(args.span(), err),
        }
    }

    fn run(&self, output: &[u8], test_path: &Path, spec_id: SpecId) -> Result<(), String> {
        let artifacts = parse_artifacts(output)?;
        let call = resolve_call(&artifacts, test_path, &self.call, &self.expected, true)?;
        let setup = setup_call(call.artifact, call.function)?;
        let actual = execute(&call.artifact.bytecode, setup, call.input, spec_id)?;
        if actual.success {
            return Err(format!(
                "`{}` succeeded with 0x{}, expected failure",
                display_call(&self.call, call.function),
                hex::encode(actual.output)
            ));
        }
        if actual.output != call.expected {
            return Err(format!(
                "`{}` reverted with 0x{}, expected 0x{}",
                display_call(&self.call, call.function),
                hex::encode(actual.output),
                hex::encode(call.expected)
            ));
        }
        Ok(())
    }
}

macro_rules! impl_flag {
    ($ty:ty) => {
        impl Flag for $ty {
            fn clone_inner(&self) -> Box<dyn Flag> {
                Box::new(self.clone())
            }

            fn post_test_action(
                &self,
                config: &TestConfig,
                output: &std::process::Output,
                _build_manager: &BuildManager,
            ) -> Result<(), Errored> {
                let spec_id = spec_id(config).map_err(|message| flag_error(Self::NAME, message))?;
                self.run(&output.stdout, config.status.path(), spec_id)
                    .map_err(|message| flag_error(Self::NAME, message))
            }

            fn must_be_unique(&self) -> bool {
                false
            }
        }
    };
}

impl_flag!(RunCall);
impl_flag!(RunCallFail);

fn parse_call(args: &str) -> Result<(&str, &str), &'static str> {
    let (call, expected) = args.split_once("=>").unwrap_or((args, ""));
    let call = call.trim();
    if call.is_empty() {
        return Err("call directive requires calldata or a function name");
    }
    Ok((call, expected.trim()))
}

fn resolve_call<'a>(
    artifacts: &'a [Artifact],
    test_path: &Path,
    call: &str,
    expected: &str,
    failure: bool,
) -> Result<ResolvedCall<'a>, String> {
    if call.starts_with("0x") {
        let artifact = only_artifact(artifacts, test_path)?;
        let input = decode_hex(call, "calldata")?;
        let expected = decode_hex(expected, "expected result")?;
        return Ok(ResolvedCall { artifact, function: None, input, expected });
    }

    let (function_name, args) = call.split_once(char::is_whitespace).unwrap_or((call, ""));
    let (artifact, function) = find_function(artifacts, function_name)?;
    let input = encode_values(function, args, false)?;
    let expected = if failure {
        decode_hex(expected, "expected revert data")?
    } else {
        encode_values(function, expected, true)?
    };
    Ok(ResolvedCall { artifact, function: Some(function), input, expected })
}

fn flag_error(command: &str, message: String) -> Errored {
    Errored {
        command: command.into(),
        errors: vec![ui_test::Error::ConfigError(message)],
        stderr: vec![],
        stdout: vec![],
    }
}

fn display_call(call: &str, function: Option<&Function>) -> String {
    function.map_or_else(|| call.to_owned(), Function::signature)
}

fn parse_artifacts(output: &[u8]) -> Result<Vec<Artifact>, String> {
    let output: Value = serde_json::from_slice(output)
        .map_err(|err| format!("failed to parse compiler output: {err}"))?;
    let contracts = output
        .get("contracts")
        .and_then(Value::as_object)
        .ok_or_else(|| "compiler output does not contain contracts".to_owned())?;
    contracts
        .iter()
        .filter_map(|(name, value)| {
            let bytecode = value.get("bin")?.as_str()?;
            Some((name, value, bytecode))
        })
        .map(|(name, value, bytecode)| {
            let abi = serde_json::from_value(value.get("abi").cloned().unwrap_or_default())
                .map_err(|err| format!("failed to parse ABI for `{name}`: {err}"))?;
            let bytecode = hex::decode(bytecode)
                .map_err(|err| format!("invalid bytecode for `{name}`: {err}"))?;
            Ok(Artifact { name: name.clone(), abi, bytecode })
        })
        .collect()
}

fn only_artifact<'a>(artifacts: &'a [Artifact], test_path: &Path) -> Result<&'a Artifact, String> {
    let primary = artifacts
        .iter()
        .filter(|artifact| {
            artifact.name.rsplit_once(':').is_some_and(|(source, _)| Path::new(source) == test_path)
        })
        .collect::<Vec<_>>();
    let candidates =
        if primary.is_empty() { artifacts.iter().collect::<Vec<_>>() } else { primary };
    match candidates.as_slice() {
        [artifact] => Ok(artifact),
        [] => Err("compiler output does not contain deployable contracts".to_owned()),
        _ => {
            Err("raw calldata is ambiguous because the source contains multiple contracts"
                .to_owned())
        }
    }
}

fn find_function<'a>(
    artifacts: &'a [Artifact],
    name: &str,
) -> Result<(&'a Artifact, &'a Function), String> {
    let (contract_name, function_name) = name
        .split_once("::")
        .map_or((None, name), |(contract, function)| (Some(contract), function));
    let mut matches = artifacts.iter().flat_map(|artifact| {
        let contract_matches =
            contract_name.is_none_or(|name| artifact.name.ends_with(&format!(":{name}")));
        artifact
            .abi
            .functions()
            .filter(move |function| {
                contract_matches
                    && (function.signature() == function_name
                        || (!function_name.contains('(') && function.name == function_name))
            })
            .map(move |function| (artifact, function))
    });
    let Some(found) = matches.next() else {
        return Err(format!("function `{name}` was not found in compiler output"));
    };
    if matches.next().is_some() {
        return Err(format!(
            "function `{name}` is ambiguous; use its full signature or qualify it as \
             `Contract::{function_name}`"
        ));
    }
    Ok(found)
}

fn encode_values(function: &Function, values: &str, output: bool) -> Result<Vec<u8>, String> {
    let params = if output { &function.outputs } else { &function.inputs };
    let values = split_values(values, params.len())?;
    let values = params
        .iter()
        .zip(values)
        .map(|(param, value)| {
            param
                .resolve()
                .and_then(|ty| ty.coerce_str(value))
                .map_err(|err| format!("invalid value `{value}` for `{}`: {err}", param.ty))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if output { function.abi_encode_output(&values) } else { function.abi_encode_input(&values) }
        .map_err(|err| format!("failed to encode values for `{}`: {err}", function.signature()))
}

fn split_values(values: &str, expected: usize) -> Result<Vec<&str>, String> {
    let values = values.trim();
    if expected == 0 {
        return if values.is_empty() || values == "0x" {
            Ok(Vec::new())
        } else {
            Err(format!("expected no values, found `{values}`"))
        };
    }

    let mut result = Vec::with_capacity(expected);
    let mut start = 0;
    let mut depth = 0_u32;
    let mut quote = None;
    for (offset, ch) in values.char_indices() {
        if let Some(active) = quote {
            if ch == active && !is_escaped(values, offset) {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '[' | '(' => depth += 1,
            ']' | ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                result.push(values[start..offset].trim());
                start = offset + ch.len_utf8();
            }
            _ => {}
        }
    }
    result.push(values[start..].trim());
    if result.len() != expected || result.iter().any(|value| value.is_empty()) {
        return Err(format!(
            "expected {expected} comma-separated values, found {}",
            result.iter().filter(|value| !value.is_empty()).count()
        ));
    }
    Ok(result)
}

fn is_escaped(value: &str, offset: usize) -> bool {
    value[..offset].bytes().rev().take_while(|byte| *byte == b'\\').count() % 2 == 1
}

fn decode_hex(value: &str, description: &str) -> Result<Vec<u8>, String> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    if value.is_empty() {
        Ok(Vec::new())
    } else {
        hex::decode(value).map_err(|err| format!("invalid {description}: {err}"))
    }
}

fn setup_call(artifact: &Artifact, function: Option<&Function>) -> Result<Option<Vec<u8>>, String> {
    function
        .filter(|function| function.name.starts_with("test"))
        .and_then(|_| artifact.abi.functions().find(|function| function.signature() == "setUp()"))
        .map(|function| {
            function
                .abi_encode_input(&[])
                .map_err(|err| format!("failed to encode `setUp()`: {err}"))
        })
        .transpose()
}

fn execute(
    initcode: &[u8],
    setup: Option<Vec<u8>>,
    input: Vec<u8>,
    spec_id: SpecId,
) -> Result<Outcome, String> {
    let mut database = InMemoryDB::default();
    database.insert_account_info(&CALLER, AccountInfo::default().with_balance(U256::MAX));
    let mut evm = Evm::<BaseEvmTypes>::new(
        spec_id,
        BlockEnv::default(),
        ethereum_tx_registry(spec_id),
        database,
        Precompiles::base(spec_id),
    );
    let result = transact(&mut evm, 0, TxKind::Create, Bytes::copy_from_slice(initcode))?;
    if !result.status {
        return Err(format!(
            "contract deployment failed with {:?}: 0x{}",
            result.stop,
            hex::encode(result.output)
        ));
    }
    let contract = result
        .created_address
        .ok_or_else(|| "contract deployment did not return an address".to_owned())?;

    let mut nonce = 1;
    if let Some(setup) = setup {
        let result =
            outcome(transact(&mut evm, nonce, TxKind::Call(contract), Bytes::from(setup))?);
        nonce += 1;
        if !result.success {
            return Err(format!(
                "`setUp()` failed with {}: 0x{}",
                result.stop,
                hex::encode(result.output)
            ));
        }
    }
    transact(&mut evm, nonce, TxKind::Call(contract), Bytes::from(input)).map(outcome)
}

fn transact(
    evm: &mut Evm<'_, BaseEvmTypes>,
    nonce: u64,
    to: TxKind,
    input: Bytes,
) -> Result<TxResult, String> {
    let tx = RecoveredTxEnvelope::Legacy(Recovered::new_unchecked(
        TxLegacy {
            nonce,
            to,
            input,
            gas_price: 0,
            value: U256::ZERO,
            chain_id: None,
            gas_limit: GAS_LIMIT,
        },
        CALLER,
    ));
    evm.transact(&tx)
        .map(evm2::ExecutedTx::commit)
        .map_err(|err| format!("transaction rejected: {err}"))
}

fn outcome(result: TxResult) -> Outcome {
    Outcome {
        success: result.status,
        output: result.output.into(),
        stop: format!("{:?}", result.stop),
    }
}

fn spec_id(config: &TestConfig) -> Result<SpecId, String> {
    let flags = config.comments().flat_map(|comments| &comments.compile_flags);
    let mut version = None;
    let mut expects_value = false;
    for flag in flags {
        if expects_value {
            version = Some(flag.as_str());
            expects_value = false;
        } else if flag == "--evm-version" {
            expects_value = true;
        } else if let Some(value) = flag.strip_prefix("--evm-version=") {
            version = Some(value);
        }
    }
    if expects_value {
        return Err("`--evm-version` requires a value".to_owned());
    }
    match version.unwrap_or("osaka") {
        "homestead" => Ok(SpecId::HOMESTEAD),
        "tangerineWhistle" => Ok(SpecId::TANGERINE),
        "spuriousDragon" => Ok(SpecId::SPURIOUS_DRAGON),
        "byzantium" => Ok(SpecId::BYZANTIUM),
        "constantinople" | "petersburg" => Ok(SpecId::PETERSBURG),
        "istanbul" => Ok(SpecId::ISTANBUL),
        "berlin" => Ok(SpecId::BERLIN),
        "london" => Ok(SpecId::LONDON),
        "paris" => Ok(SpecId::MERGE),
        "shanghai" => Ok(SpecId::SHANGHAI),
        "cancun" => Ok(SpecId::CANCUN),
        "prague" => Ok(SpecId::PRAGUE),
        "osaka" => Ok(SpecId::OSAKA),
        "amsterdam" => Ok(SpecId::AMSTERDAM),
        version => Err(format!("unsupported EVM version `{version}`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_empty_expected_output() {
        assert_eq!(parse_call("f()"), Ok(("f()", "")));
        assert_eq!(parse_call("f() =>"), Ok(("f()", "")));
    }

    #[test]
    fn rejects_empty_call() {
        assert_eq!(parse_call(""), Err("call directive requires calldata or a function name"));
        assert_eq!(parse_call(" => 1"), Err("call directive requires calldata or a function name"));
    }

    #[test]
    fn splits_only_top_level_commas() {
        assert_eq!(split_values("[1, 2], (true, false)", 2), Ok(vec!["[1, 2]", "(true, false)"]));
        assert!(split_values("1 true", 2).is_err());
    }
}
