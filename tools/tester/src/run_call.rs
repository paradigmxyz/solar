use alloy_consensus::{TxLegacy, transaction::Recovered};
use alloy_dyn_abi::{DynSolValue, FunctionExt, JsonAbiExt, Specifier};
use alloy_json_abi::{Error, Function, JsonAbi, Param};
use alloy_primitives::{Address, Bytes, TxKind, U256, hex};
use evm2::{
    BaseEvmTypes, Evm, Precompiles, SpecId, TxResult,
    env::BlockEnv,
    ethereum::{TxEnvelope, ethereum_tx_registry},
    evm::{AccountInfo, InMemoryDB},
};
use serde_json::Value;
use std::{borrow::Cow, cell::Cell, path::Path, rc::Rc};
use ui_test::{
    CommentParser, Errored, Revisioned,
    build_manager::BuildManager,
    custom_flags::Flag,
    per_test_config::TestConfig,
    spanned::{Span, Spanned},
};

mod oracle;

const CALLER: Address = Address::repeat_byte(0x22);

/// Amsterdam splits execution gas from a reservoir for more expensive state
/// creation (EIP-8037). Keep a generous state budget for correctness fixtures;
/// the interpreter still enforces the fork's execution-gas cap.
fn default_gas_limit(spec: SpecId) -> u64 {
    if spec >= SpecId::AMSTERDAM { 100_000_000 } else { 10_000_000 }
}

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
    /// What the call did, when the MIR interpreter checks it.
    trace: Option<oracle::Trace>,
}

struct ResolvedCall<'a> {
    artifact: &'a Artifact,
    function: Option<Cow<'a, Function>>,
    constructor_args: Vec<u8>,
    input: Vec<u8>,
    expected: Vec<u8>,
    gas_limit: Option<u64>,
    value: U256,
}

#[derive(Debug, PartialEq, Eq)]
struct ParsedCall<'a> {
    call: &'a str,
    expected: &'a str,
    settings: CallSettings,
}

pub(crate) fn parse_directive(line: &str) -> Option<(&str, Option<&str>)> {
    let mut directive = line.trim_start().strip_prefix("//@")?.trim_start();
    let revisions = if let Some(scoped) = directive.strip_prefix('[') {
        let (revisions, rest) = scoped.split_once(']')?;
        directive = rest.trim_start();
        Some(revisions)
    } else {
        None
    };
    Some((directive, revisions))
}

pub(crate) fn is_directive(line: &str) -> bool {
    parse_directive(line).is_some_and(|(directive, _)| {
        directive.starts_with("run-call:") || directive.starts_with("run-call-fail:")
    })
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

    fn run(
        &self,
        output: &[u8],
        config: &TestConfig,
        build_manager: &BuildManager,
        spec_id: SpecId,
    ) -> Result<(), String> {
        let artifacts = parse_artifacts(output)?;
        let test_path = config.status.path();
        let call =
            resolve_call(&artifacts, test_path, &self.call, &self.expected, &self.settings, false)?;
        let actual = execute(&call, spec_id, oracle::applies(config))?;
        if !actual.success {
            return Err(format!(
                "`{}` failed with {}: 0x{}",
                display_call(&self.call, call.function.as_deref()),
                actual.stop,
                hex::display(actual.output)
            ));
        }
        if actual.output != call.expected {
            return Err(format!(
                "`{}` returned 0x{}, expected 0x{}",
                display_call(&self.call, call.function.as_deref()),
                hex::display(actual.output),
                hex::display(call.expected)
            ));
        }
        check_mir(config, build_manager, &self.call, &call, &actual)
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

    fn run(
        &self,
        output: &[u8],
        config: &TestConfig,
        build_manager: &BuildManager,
        spec_id: SpecId,
    ) -> Result<(), String> {
        let artifacts = parse_artifacts(output)?;
        let test_path = config.status.path();
        let call =
            resolve_call(&artifacts, test_path, &self.call, &self.expected, &self.settings, true)?;
        let actual = execute(&call, spec_id, oracle::applies(config))?;
        if actual.success {
            return Err(format!(
                "`{}` succeeded with 0x{}, expected failure",
                display_call(&self.call, call.function.as_deref()),
                hex::display(actual.output)
            ));
        }
        if actual.output != call.expected {
            return Err(format!(
                "`{}` reverted with 0x{}, expected 0x{}",
                display_call(&self.call, call.function.as_deref()),
                hex::display(actual.output),
                hex::display(call.expected)
            ));
        }
        check_mir(config, build_manager, &self.call, &call, &actual)
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
                build_manager: &BuildManager,
            ) -> Result<(), Errored> {
                let spec_id = spec_id(config).map_err(|message| flag_error(Self::NAME, message))?;
                self.run(&output.stdout, config, build_manager, spec_id)
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

/// Checks the call against the MIR interpreter when that check is enabled.
fn check_mir(
    config: &TestConfig,
    build_manager: &BuildManager,
    directive: &str,
    call: &ResolvedCall<'_>,
    actual: &Outcome,
) -> Result<(), String> {
    let Some(trace) = &actual.trace else { return Ok(()) };
    let name = display_call(directive, call.function.as_deref());
    let evm_version = evm_version(config)?
        .parse()
        .map_err(|error| format!("unsupported EVM version for the MIR interpreter: {error}"))?;
    let call = oracle::Call {
        contract: &call.artifact.name,
        name: &name,
        input: &call.input,
        value: call.value,
        evm_version,
    };
    oracle::check(config, build_manager, &call, trace)
}

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
        let expected = if failure {
            encode_revert_data(expected)?
        } else {
            decode_hex(expected, "expected result")?
        };
        return Ok(ResolvedCall {
            artifact,
            function: None,
            constructor_args,
            input,
            expected,
            gas_limit: settings.gas,
            value: settings.value.unwrap_or_default(),
        });
    }

    let (function_name, args) = call.split_once(char::is_whitespace).unwrap_or((call, ""));
    let mir = test_path.extension().is_some_and(|extension| extension == "mir");
    let (artifact, function) = find_function(artifacts, function_name, mir)?;
    let constructor_args = encode_constructor(artifact, settings.constructor.as_deref())?;
    let input = encode_values(&function, args, false)?;
    let expected = if failure {
        encode_revert_data(expected)?
    } else {
        encode_values(&function, expected, true)?
    };
    Ok(ResolvedCall {
        artifact,
        function: Some(function),
        constructor_args,
        input,
        expected,
        gas_limit: settings.gas,
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
    let output: Value = match serde_json::from_slice(output) {
        Ok(output) => output,
        Err(err) => {
            let marker = br#""contracts""#;
            let Some(contracts) = output.windows(marker.len()).rposition(|window| window == marker)
            else {
                return Err(format!("failed to parse compiler output: {err}"));
            };
            let Some(start) = output[..contracts].iter().rposition(|&byte| byte == b'{') else {
                return Err(format!("failed to parse compiler output: {err}"));
            };
            serde_json::Deserializer::from_slice(&output[start..])
                .into_iter()
                .next()
                .transpose()
                .map_err(|_| format!("failed to parse compiler output: {err}"))?
                .ok_or_else(|| format!("failed to parse compiler output: {err}"))?
        }
    };
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

/// Finds the function `name` names in `artifacts`. In a test of MIR input, whose modules have no
/// ABI, a full signature names a function without one.
fn find_function<'a>(
    artifacts: &'a [Artifact],
    name: &str,
    mir: bool,
) -> Result<(&'a Artifact, Cow<'a, Function>), String> {
    let (contract_name, function_name) = name
        .split_once("::")
        .map_or((None, name), |(contract, function)| (Some(contract), function));
    let contract_matches = |artifact: &Artifact| {
        contract_name.is_none_or(|name| artifact.name.ends_with(&format!(":{name}")))
    };
    let mut matches =
        artifacts.iter().filter(|artifact| contract_matches(artifact)).flat_map(|artifact| {
            artifact
                .abi
                .functions()
                .filter(move |function| {
                    function.signature() == function_name
                        || (!function_name.contains('(') && function.name == function_name)
                })
                .map(move |function| (artifact, function))
        });
    let Some((artifact, function)) = matches.next() else {
        // A MIR module has no ABI, so a call to it gives the function's full signature. A
        // Solidity contract without functions only has a fallback, which a call naming a missing
        // function would reach.
        if !mir {
            return Err(format!("function `{name}` was not found in compiler output"));
        }
        let abi_less = artifacts
            .iter()
            .filter(|artifact| {
                contract_matches(artifact) && artifact.abi.functions().next().is_none()
            })
            .collect::<Vec<_>>();
        if function_name.contains('(')
            && let [artifact] = abi_less[..]
        {
            return Ok((artifact, Cow::Owned(parse_signature(function_name)?)));
        }
        return Err(format!("function `{name}` was not found in compiler output"));
    };
    if matches.next().is_some() {
        return Err(format!(
            "function `{name}` is ambiguous; use its full signature or qualify it as \
             `Contract::{function_name}`"
        ));
    }
    Ok((artifact, Cow::Borrowed(function)))
}

/// Parses the signature `name(inputs)` of a function, or `name(inputs)(outputs)` when it returns
/// values, for a contract without an ABI.
fn parse_signature(signature: &str) -> Result<Function, String> {
    let invalid = |detail: String| format!("invalid function signature `{signature}`: {detail}");
    let open = signature.find('(').ok_or_else(|| invalid("expected `(`".to_owned()))?;
    let (name, rest) = signature.split_at(open);
    let (inputs, rest) = split_parenthesized(rest).map_err(invalid)?;
    let text = if rest.is_empty() {
        format!("function {name}({inputs})")
    } else {
        let (outputs, rest) = split_parenthesized(rest).map_err(invalid)?;
        if !rest.is_empty() {
            return Err(invalid(format!("unexpected `{rest}` after the outputs")));
        }
        format!("function {name}({inputs}) returns ({outputs})")
    };
    Function::parse(&text).map_err(|error| invalid(error.to_string()))
}

fn encode_values(function: &Function, values: &str, output: bool) -> Result<Vec<u8>, String> {
    let params = if output { &function.outputs } else { &function.inputs };
    let values = coerce_values(params, values)?;
    if output { function.abi_encode_output(&values) } else { function.abi_encode_input(&values) }
        .map_err(|err| format!("failed to encode values for `{}`: {err}", function.signature()))
}

fn encode_revert_data(value: &str) -> Result<Vec<u8>, String> {
    let value = value.trim();
    if value.is_empty() || value.starts_with("0x") {
        return decode_hex(value, "expected revert data");
    }

    let Some(offset) = value.find('(') else {
        return Err("expected revert data must be hex or an error invocation".to_owned());
    };
    let name = value[..offset].trim();
    let (types_or_values, rest) = split_parenthesized(&value[offset..])?;
    let (error, values) = match name {
        "Panic" | "Error" => {
            if !rest.trim().is_empty() {
                return Err(format!("unexpected data after error invocation `{value}`"));
            }
            let signature = if name == "Panic" { "Panic(uint256)" } else { "Error(string)" };
            let error = Error::parse(signature)
                .map_err(|err| format!("invalid builtin error `{name}`: {err}"))?;
            (error, types_or_values)
        }
        _ => {
            let (values, rest) = split_parenthesized(rest.trim_start())?;
            if !rest.trim().is_empty() {
                return Err(format!("unexpected data after error invocation `{value}`"));
            }
            let error = Error::parse(&format!("{name}({types_or_values})"))
                .map_err(|err| format!("invalid error `{name}`: {err}"))?;
            (error, values)
        }
    };

    let values = coerce_values(&error.inputs, values)?;
    error
        .abi_encode_input(&values)
        .map_err(|err| format!("failed to encode error `{}`: {err}", error.signature()))
}

fn split_parenthesized(value: &str) -> Result<(&str, &str), String> {
    let Some(value) = value.strip_prefix('(') else {
        return Err("expected parenthesized error arguments".to_owned());
    };

    let mut depth = 1_u32;
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
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Ok((&value[..offset], &value[offset + ch.len_utf8()..]));
                }
            }
            _ => {}
        }
    }
    Err("unterminated error arguments".to_owned())
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

/// Deploys the called contract, runs its `setUp()` when the call needs one, and executes the call,
/// recording what it did for the MIR interpreter to check when `check_mir` is set.
fn execute(call: &ResolvedCall<'_>, spec_id: SpecId, check_mir: bool) -> Result<Outcome, String> {
    let setup = setup_call(call.artifact, call.function.as_deref())?;
    let mut database = InMemoryDB::default();
    database.insert_account_info(&CALLER, AccountInfo::default().with_balance(U256::MAX));
    let mut evm = Evm::<BaseEvmTypes>::new(
        spec_id,
        BlockEnv::<BaseEvmTypes>::default(),
        ethereum_tx_registry(spec_id),
        database,
        Precompiles::base(spec_id),
    );
    let initcode =
        Bytes::from_iter(call.artifact.bytecode.iter().chain(&call.constructor_args).copied());
    let result =
        transact(&mut evm, 0, TxKind::Create, initcode, default_gas_limit(spec_id), U256::ZERO)?;
    if !result.status {
        return Err(format!(
            "contract deployment failed with {:?}: 0x{}",
            result.stop,
            hex::display(result.output)
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
            default_gas_limit(spec_id),
            U256::ZERO,
        )?);
        nonce += 1;
        if !result.success {
            return Err(format!(
                "`setUp()` failed with {}: 0x{}",
                result.stop,
                hex::display(result.output)
            ));
        }
    }
    let before = check_mir.then(|| oracle::Chain::capture(&evm, contract));
    let heap_start = Rc::new(Cell::new(None));
    if before.is_some() {
        evm.set_inspector(oracle::HeapStart(heap_start.clone()));
    }
    let input = Bytes::copy_from_slice(&call.input);
    let gas_limit = call.gas_limit.unwrap_or_else(|| default_gas_limit(spec_id));
    let result = transact(&mut evm, nonce, TxKind::Call(contract), input, gas_limit, call.value)?;
    let trace = before.map(|before| {
        evm.clear_inspector();
        let (stop, output, logs) = (result.stop, &result.output, &result.logs);
        oracle::Trace::new(before, &evm, stop, output, logs, heap_start.get())
    });
    Ok(Outcome { trace, ..outcome(result) })
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
        trace: None,
    }
}

/// Returns the EVM version the test compiles for.
fn evm_version(config: &TestConfig) -> Result<&str, String> {
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
    Ok(version.unwrap_or("osaka"))
}

fn spec_id(config: &TestConfig) -> Result<SpecId, String> {
    match evm_version(config)? {
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
    fn parses_artifacts_after_mir_dump() {
        let output = br#"@module Test

{"contracts":{"source.sol:Test":{"abi":[],"bin":"00"}}}"#;
        let artifacts = parse_artifacts(output).unwrap();
        assert_eq!(artifacts[0].name, "source.sol:Test");
        assert_eq!(artifacts[0].bytecode, [0]);
    }

    #[test]
    fn parses_artifacts_before_evm_ir_dump() {
        let output = br#"{
  "contracts": {"source.sol:Test": {"abi": [], "bin": "00"}}
}

@module runtime"#;
        let artifacts = parse_artifacts(output).unwrap();
        assert_eq!(artifacts[0].name, "source.sol:Test");
        assert_eq!(artifacts[0].bytecode, [0]);
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

    #[test]
    fn encodes_error_invocations() {
        let panic = encode_revert_data("Panic(0x11)").unwrap();
        assert_eq!(&panic[..4], [0x4e, 0x48, 0x7b, 0x71]);
        assert_eq!(panic.len(), 36);

        let string = encode_revert_data("Error(\"x (y)\")").unwrap();
        assert_eq!(&string[..4], [0x08, 0xc3, 0x79, 0xa0]);

        let custom = encode_revert_data(
            "Stuff(uint256,bytes32)(1, 0x1234000000000000000000000000000000000000000000000000000000000000)",
        )
        .unwrap();
        assert_eq!(&custom[..4], Error::parse("Stuff(uint256,bytes32)").unwrap().selector());

        assert_eq!(encode_revert_data("Empty()()").unwrap().len(), 4);
    }

    #[test]
    fn rejects_invalid_error_invocations() {
        assert!(encode_revert_data("Panic(1) extra").is_err());
        assert!(encode_revert_data("Error()").is_err());
        assert!(encode_revert_data("E(uint256)").is_err());
        assert!(encode_revert_data("E(uint256)(1) extra").is_err());
        assert!(encode_revert_data("E(uint256)(1").is_err());
    }

    #[test]
    fn signatures_name_functions_only_in_mir() {
        let artifact = |abi: &str| Artifact {
            name: "source:Test".into(),
            abi: serde_json::from_str(abi).unwrap(),
            bytecode: vec![0],
        };
        let fallback_only = [artifact(r#"[{"type":"fallback","stateMutability":"nonpayable"}]"#)];
        let signature = "missing(uint256)(uint256)";
        let error = find_function(&fallback_only, signature, false).unwrap_err();
        assert_eq!(error, "function `missing(uint256)(uint256)` was not found in compiler output");
        let module = [artifact("[]")];
        let (_, function) = find_function(&module, signature, true).unwrap();
        assert_eq!(function.signature(), "missing(uint256)");
    }

    #[test]
    fn parses_abi_less_signatures() {
        let add = parse_signature("add(uint256,uint256)(uint256)").unwrap();
        assert_eq!(add.signature(), "add(uint256,uint256)");
        assert_eq!(
            add.outputs.iter().map(|output| output.ty.as_str()).collect::<Vec<_>>(),
            ["uint256"]
        );
        let increment = parse_signature("increment()").unwrap();
        assert_eq!(increment.signature(), "increment()");
        assert!(increment.outputs.is_empty());
        assert_eq!(
            parse_signature("f()(uint256)x").unwrap_err(),
            "invalid function signature `f()(uint256)x`: unexpected `x` after the outputs"
        );
        assert!(parse_signature("f(uint256").is_err());
    }
}
