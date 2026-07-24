use alloy_consensus::{TxLegacy, transaction::Recovered};
use alloy_dyn_abi::{DynSolValue, FunctionExt, JsonAbiExt, Specifier};
use alloy_json_abi::{Function, JsonAbi, Param};
use alloy_primitives::{Address, Bytes, TxKind, U256, hex};
use evm2::{
    BaseEvmTypes, Evm, Precompiles, SpecId, TxResult,
    env::BlockEnv,
    ethereum::{TxEnvelope, ethereum_tx_registry},
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
const DEFAULT_GAS_LIMIT: u64 = 10_000_000;

#[derive(Debug, Clone)]
pub(crate) struct RunCall {
    call: String,
    expected: String,
    settings: CallSettings,
}

#[derive(Debug, Clone)]
pub(crate) struct RunCallFail {
    call: String,
    expected: String,
    settings: CallSettings,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CallSettings {
    constructor: Option<String>,
    gas: Option<u64>,
    value: Option<U256>,
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
    constructor_args: Vec<u8>,
    input: Vec<u8>,
    expected: Vec<u8>,
    gas_limit: u64,
    value: U256,
}

#[derive(Debug, PartialEq, Eq)]
struct ParsedCall<'a> {
    call: &'a str,
    expected: &'a str,
    settings: CallSettings,
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
            Ok(parsed) => parser.add_custom_spanned(
                Self::NAME,
                Self {
                    call: parsed.call.to_owned(),
                    expected: parsed.expected.to_owned(),
                    settings: parsed.settings,
                },
                span,
            ),
            Err(err) => parser.error(args.span(), err),
        }
    }

    fn run(&self, output: &[u8], test_path: &Path, spec_id: SpecId) -> Result<(), String> {
        let artifacts = parse_artifacts(output)?;
        let call =
            resolve_call(&artifacts, test_path, &self.call, &self.expected, &self.settings, false)?;
        let setup = setup_call(call.artifact, call.function)?;
        let actual = execute(
            &call.artifact.bytecode,
            &call.constructor_args,
            setup,
            call.input,
            call.gas_limit,
            call.value,
            spec_id,
        )?;
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
            Ok(parsed) => parser.add_custom_spanned(
                Self::NAME,
                Self {
                    call: parsed.call.to_owned(),
                    expected: parsed.expected.to_owned(),
                    settings: parsed.settings,
                },
                span,
            ),
            Err(err) => parser.error(args.span(), err),
        }
    }

    fn run(&self, output: &[u8], test_path: &Path, spec_id: SpecId) -> Result<(), String> {
        let artifacts = parse_artifacts(output)?;
        let call =
            resolve_call(&artifacts, test_path, &self.call, &self.expected, &self.settings, true)?;
        let setup = setup_call(call.artifact, call.function)?;
        let actual = execute(
            &call.artifact.bytecode,
            &call.constructor_args,
            setup,
            call.input,
            call.gas_limit,
            call.value,
            spec_id,
        )?;
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

fn parse_call(args: &str) -> Result<ParsedCall<'_>, String> {
    let (call_and_settings, expected) = split_expected(args).unwrap_or((args, ""));
    let (call, settings) =
        split_top_level_once(call_and_settings, ';').unwrap_or((call_and_settings, ""));
    let call = call.trim();
    if call.is_empty() {
        return Err("call directive requires calldata or a function name".to_owned());
    }
    Ok(ParsedCall { call, expected: expected.trim(), settings: parse_settings(settings)? })
}

fn split_expected(value: &str) -> Option<(&str, &str)> {
    let mut depth = 0_u32;
    let mut quote = None;
    for (offset, ch) in value.char_indices() {
        if let Some(active) = quote {
            if ch == active && !is_escaped(value, offset) {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '[' | '(' => depth += 1,
            ']' | ')' => depth = depth.saturating_sub(1),
            '=' if depth == 0 && value[offset..].starts_with("=>") => {
                return Some((&value[..offset], &value[offset + 2..]));
            }
            _ => {}
        }
    }
    None
}

fn parse_settings(settings: &str) -> Result<CallSettings, String> {
    let settings = settings.trim();
    if settings.is_empty() {
        return Ok(CallSettings::default());
    }

    let mut parsed = CallSettings::default();
    for setting in split_top_level(settings, ',') {
        let setting = setting.trim();
        let (key, value) = setting
            .split_once('=')
            .map(|(key, value)| (key.trim(), value.trim()))
            .ok_or_else(|| format!("setting `{setting}` requires a value"))?;
        if key.is_empty() || value.is_empty() {
            return Err(format!("setting `{setting}` requires a value"));
        }
        match key {
            "constructor" => {
                if parsed.constructor.replace(value.to_owned()).is_some() {
                    return Err("duplicate `constructor` setting".to_owned());
                }
            }
            "gas" => {
                if parsed.gas.is_some() {
                    return Err("duplicate `gas` setting".to_owned());
                }
                let gas = parse_integer(value, "gas")?;
                parsed.gas = Some(
                    gas.try_into()
                        .map_err(|_| format!("`gas` value `{value}` does not fit in a u64"))?,
                );
            }
            "value" => {
                if parsed.value.is_some() {
                    return Err("duplicate `value` setting".to_owned());
                }
                parsed.value = Some(parse_integer(value, "value")?);
            }
            _ => return Err(format!("unknown run-call setting `{key}`")),
        }
    }
    Ok(parsed)
}

fn parse_integer(value: &str, setting: &str) -> Result<U256, String> {
    let (digits, radix) = value.strip_prefix("0x").map_or((value, 10), |digits| (digits, 16));
    U256::from_str_radix(digits, radix)
        .map_err(|err| format!("invalid `{setting}` value `{value}`: {err}"))
}

fn resolve_call<'a>(
    artifacts: &'a [Artifact],
    test_path: &Path,
    call: &str,
    expected: &str,
    settings: &CallSettings,
    failure: bool,
) -> Result<ResolvedCall<'a>, String> {
    if call.starts_with("0x") {
        let artifact = only_artifact(artifacts, test_path)?;
        let constructor_args = encode_constructor(artifact, settings.constructor.as_deref())?;
        let input = decode_hex(call, "calldata")?;
        let expected = decode_hex(expected, "expected result")?;
        return Ok(ResolvedCall {
            artifact,
            function: None,
            constructor_args,
            input,
            expected,
            gas_limit: settings.gas.unwrap_or(DEFAULT_GAS_LIMIT),
            value: settings.value.unwrap_or_default(),
        });
    }

    let (function_name, args) = call.split_once(char::is_whitespace).unwrap_or((call, ""));
    let (artifact, function) = find_function(artifacts, function_name)?;
    let constructor_args = encode_constructor(artifact, settings.constructor.as_deref())?;
    let input = encode_values(function, args, false)?;
    let expected = if failure {
        decode_hex(expected, "expected revert data")?
    } else {
        encode_values(function, expected, true)?
    };
    Ok(ResolvedCall {
        artifact,
        function: Some(function),
        constructor_args,
        input,
        expected,
        gas_limit: settings.gas.unwrap_or(DEFAULT_GAS_LIMIT),
        value: settings.value.unwrap_or_default(),
    })
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
    let values = coerce_values(params, values)?;
    if output { function.abi_encode_output(&values) } else { function.abi_encode_input(&values) }
        .map_err(|err| format!("failed to encode values for `{}`: {err}", function.signature()))
}

fn encode_constructor(artifact: &Artifact, values: Option<&str>) -> Result<Vec<u8>, String> {
    let Some(values) = values else {
        if let Some(constructor) = &artifact.abi.constructor
            && !constructor.inputs.is_empty()
        {
            return Err(format!(
                "constructor for `{}` expects {} arguments; add `constructor=[...]`",
                artifact.name,
                constructor.inputs.len()
            ));
        }
        return Ok(Vec::new());
    };
    let Some(values) = values.strip_prefix('[').and_then(|values| values.strip_suffix(']')) else {
        return Err("`constructor` must be a bracketed argument list".to_owned());
    };
    let Some(constructor) = &artifact.abi.constructor else {
        return if values.trim().is_empty() {
            Ok(Vec::new())
        } else {
            Err(format!("contract `{}` has no constructor arguments", artifact.name))
        };
    };
    let values = coerce_values(&constructor.inputs, values)?;
    constructor
        .abi_encode_input(&values)
        .map_err(|err| format!("failed to encode constructor arguments: {err}"))
}

fn coerce_values(params: &[Param], values: &str) -> Result<Vec<DynSolValue>, String> {
    let values = split_values(values, params.len())?;
    params
        .iter()
        .zip(values)
        .map(|(param, value)| {
            param
                .resolve()
                .and_then(|ty| ty.coerce_str(value))
                .map_err(|err| format!("invalid value `{value}` for `{}`: {err}", param.ty))
        })
        .collect()
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

    let result = split_top_level(values, ',');
    if result.len() != expected || result.iter().any(|value| value.is_empty()) {
        return Err(format!(
            "expected {expected} comma-separated values, found {}",
            result.iter().filter(|value| !value.is_empty()).count()
        ));
    }
    Ok(result)
}

fn split_top_level(value: &str, separator: char) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut depth = 0_u32;
    let mut quote = None;
    for (offset, ch) in value.char_indices() {
        if let Some(active) = quote {
            if ch == active && !is_escaped(value, offset) {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '[' | '(' => depth += 1,
            ']' | ')' => depth = depth.saturating_sub(1),
            ch if ch == separator && depth == 0 => {
                result.push(value[start..offset].trim());
                start = offset + ch.len_utf8();
            }
            _ => {}
        }
    }
    result.push(value[start..].trim());
    result
}

fn split_top_level_once(value: &str, separator: char) -> Option<(&str, &str)> {
    let mut depth = 0_u32;
    let mut quote = None;
    for (offset, ch) in value.char_indices() {
        if let Some(active) = quote {
            if ch == active && !is_escaped(value, offset) {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '[' | '(' => depth += 1,
            ']' | ')' => depth = depth.saturating_sub(1),
            ch if ch == separator && depth == 0 => {
                return Some((&value[..offset], &value[offset + ch.len_utf8()..]));
            }
            _ => {}
        }
    }
    None
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
    constructor_args: &[u8],
    setup: Option<Vec<u8>>,
    input: Vec<u8>,
    gas_limit: u64,
    value: U256,
    spec_id: SpecId,
) -> Result<Outcome, String> {
    let mut database = InMemoryDB::default();
    database.insert_account_info(&CALLER, AccountInfo::default().with_balance(U256::MAX));
    let mut evm = Evm::<BaseEvmTypes>::new(
        spec_id,
        BlockEnv::<BaseEvmTypes>::default(),
        ethereum_tx_registry(spec_id),
        database,
        Precompiles::base(spec_id),
    );
    let initcode = Bytes::from_iter(initcode.iter().chain(constructor_args).copied());
    let result = transact(&mut evm, 0, TxKind::Create, initcode, DEFAULT_GAS_LIMIT, U256::ZERO)?;
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
        let result = outcome(transact(
            &mut evm,
            nonce,
            TxKind::Call(contract),
            Bytes::from(setup),
            DEFAULT_GAS_LIMIT,
            U256::ZERO,
        )?);
        nonce += 1;
        if !result.success {
            return Err(format!(
                "`setUp()` failed with {}: 0x{}",
                result.stop,
                hex::encode(result.output)
            ));
        }
    }
    transact(&mut evm, nonce, TxKind::Call(contract), Bytes::from(input), gas_limit, value)
        .map(outcome)
}

fn transact(
    evm: &mut Evm<'_, BaseEvmTypes>,
    nonce: u64,
    to: TxKind,
    input: Bytes,
    gas_limit: u64,
    value: U256,
) -> Result<TxResult, String> {
    let tx = Recovered::new_unchecked(
        TxEnvelope::Legacy(TxLegacy {
            nonce,
            to,
            input,
            gas_price: 0,
            value,
            chain_id: None,
            gas_limit,
        }),
        CALLER,
    );
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
        assert_eq!(
            parse_call("f()"),
            Ok(ParsedCall { call: "f()", expected: "", settings: CallSettings::default() })
        );
        assert_eq!(
            parse_call("f() =>"),
            Ok(ParsedCall { call: "f()", expected: "", settings: CallSettings::default() })
        );
    }

    #[test]
    fn rejects_empty_call() {
        let error = "call directive requires calldata or a function name".to_owned();
        assert_eq!(parse_call(""), Err(error.clone()));
        assert_eq!(parse_call(" => 1"), Err(error));
    }

    #[test]
    fn splits_only_top_level_commas() {
        assert_eq!(split_values("[1, 2], (true, false)", 2), Ok(vec!["[1, 2]", "(true, false)"]));
        assert!(split_values("1 true", 2).is_err());
    }

    #[test]
    fn parses_call_settings() {
        assert_eq!(
            parse_call("f 1; constructor=[\"=>\", [3, 4]], gas=0x100, value=5 => 6"),
            Ok(ParsedCall {
                call: "f 1",
                expected: "6",
                settings: CallSettings {
                    constructor: Some("[\"=>\", [3, 4]]".to_owned()),
                    gas: Some(256),
                    value: Some(U256::from(5)),
                },
            })
        );
    }

    #[test]
    fn rejects_invalid_call_settings() {
        assert_eq!(
            parse_call("f; unknown=1"),
            Err("unknown run-call setting `unknown`".to_owned())
        );
        assert_eq!(parse_call("f; gas=1, gas=2"), Err("duplicate `gas` setting".to_owned()));
        assert_eq!(parse_call("f; value"), Err("setting `value` requires a value".to_owned()));
    }
}
