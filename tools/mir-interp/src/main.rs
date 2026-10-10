//! Runs lowered MIR in the compiler's MIR interpreter.
//!
//! The input is one lowered MIR module, such as a `.mir` test fixture, or the output of
//! `solar -Zdump=mir-final`. The tool runs a transaction on the module's dispatch entry, with
//! calldata given in hex or as an ABI call, or one internal function on argument words, and prints
//! how the run ended, the events it logged, and the storage it wrote. Storage and context reads
//! return what the command line sets, and zero otherwise.

use alloy_dyn_abi::{DynSolType, DynSolValue, FunctionExt, JsonAbiExt, Specifier};
use alloy_json_abi::Function;
use alloy_primitives::{U256, hex};
use clap::Parser;
use serde_json::{Map, Value as Json, json};
use solar_codegen::{
    backend::evm::op,
    interpret::{self, DumpedModule, Execution, HOST_READS, Host, Options, Outcome},
};
use solar_config::EvmVersion;
use std::{
    collections::HashMap,
    fs::File,
    io::{self, Read},
    path::PathBuf,
    process::ExitCode,
    str::FromStr,
};

#[cfg(test)]
use snapbox as _;

/// Context reads that return addresses, whose values must fit 160 bits.
const ADDRESS_READS: [&str; 4] = ["address", "caller", "origin", "coinbase"];
/// The selector of `Error(string)` reverts.
const ERROR_SELECTOR: [u8; 4] = [0x08, 0xc3, 0x79, 0xa0];
/// The selector of `Panic(uint256)` reverts.
const PANIC_SELECTOR: [u8; 4] = [0x4e, 0x48, 0x7b, 0x71];

/// Runs lowered MIR in the compiler's MIR interpreter: a transaction on a contract's dispatch
/// entry, or one internal function.
///
/// Storage and context reads return what `--storage` and `--context` set, and zero otherwise.
/// Exits with 0 when the run ends, however it ends, 1 on errors, and 2 when the interpreter cannot
/// run the code it reaches.
#[derive(Parser)]
#[command(version, about, long_about)]
struct Args {
    /// A lowered MIR module, or the output of `solar -Zdump=mir-final`; `-` reads standard input.
    file: PathBuf,
    /// The dumped contract to run, named in full, as in `src/Token.sol:Token`, or by its name.
    #[arg(long)]
    contract: Option<String>,
    /// Runs a transaction with this calldata, in hex.
    #[arg(long, conflicts_with_all = ["call", "function"])]
    calldata: Option<String>,
    /// Runs a transaction calling this function, such as `transfer(address,uint256)`, with the
    /// arguments that follow; a signature that ends in `returns (...)` also decodes the result.
    #[arg(long, num_args = 1.., value_names = ["SIGNATURE", "ARGS"], conflicts_with = "function")]
    call: Option<Vec<String>>,
    /// Runs this internal function on the `--arg` words instead of a transaction.
    #[arg(long)]
    function: Option<String>,
    /// An argument word of `--function`, in decimal or `0x` hex.
    #[arg(long = "arg", value_name = "WORD", requires = "function")]
    args: Vec<String>,
    /// A persistent storage slot's value before the run, as `SLOT=VALUE`.
    #[arg(long, value_name = "SLOT=VALUE")]
    storage: Vec<String>,
    /// A context read's value, as `NAME=VALUE`, such as
    /// `caller=0x70997970c51812dc3a010c7d01b50e0d17dc79c8`, `callvalue=5`, or
    /// `timestamp=1700000000`. A read with operands, such as `balance`, returns the same value
    /// for every operand.
    #[arg(long, value_name = "NAME=VALUE")]
    context: Vec<String>,
    /// Where the free memory pointer starts: the heap start the backend chose, which the bytecode
    /// stores at `0x40` first.
    #[arg(long, value_name = "WORD", default_value = "0x80")]
    heap_start: String,
    /// The EVM version the module targets.
    #[arg(long, value_enum, default_value_t)]
    evm_version: EvmVersion,
    /// Units of work before the run stops: one per operation and per copied, hashed, or returned
    /// word.
    #[arg(long, default_value_t = 10_000_000)]
    fuel: u64,
    /// Prints every operation to standard error as it runs, with its operand values and result.
    #[arg(long)]
    trace: bool,
    /// Prints the result as JSON.
    #[arg(long)]
    json: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(&args) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &Args) -> Result<ExitCode, String> {
    let text = read_input(&args.file)?;
    let module = select(interpret::parse_dump(&text), args.contract.as_deref())?;
    let mut host = CommandLineHost::new(args)?;
    let mut trace = |line: &str| eprintln!("{line}");
    let options = Options {
        evm_version: args.evm_version,
        fuel: args.fuel,
        trace: args.trace.then_some(&mut trace as &mut dyn FnMut(&str)),
        ..Options::default()
    };
    let (execution, function) = match &args.function {
        Some(name) => {
            let words = args.args.iter().map(|arg| word(arg)).collect::<Result<Vec<_>, _>>()?;
            (interpret::call(&module.mir, name, &words, &mut host, options)?, None)
        }
        None => {
            let (calldata, function) = calldata(args)?;
            (interpret::transact(&module.mir, &calldata, &mut host, options)?, function)
        }
    };
    let decoded = match (&execution.outcome, &function) {
        (Outcome::Success(data), Some(function)) if !function.outputs.is_empty() => Some(
            function
                .abi_decode_output(data)
                .map_err(|error| format!("cannot decode the returned data: {error}"))?,
        ),
        _ => None,
    };
    if args.json {
        println!("{:#}", report_json(&execution, decoded.as_deref()));
    } else {
        print!("{}", report(&execution, decoded.as_deref()));
    }
    Ok(if matches!(execution.outcome, Outcome::Unsupported(_)) {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    })
}

/// Reads the input file, or standard input for `-`.
fn read_input(path: &PathBuf) -> Result<String, String> {
    let mut text = String::new();
    if path.as_os_str() == "-" {
        io::stdin()
            .read_to_string(&mut text)
            .map_err(|error| format!("cannot read stdin: {error}"))?;
    } else {
        File::open(path)
            .and_then(|mut file| file.read_to_string(&mut text))
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    }
    Ok(text)
}

/// Picks the module to run: the one `contract` names, or the only one.
fn select(modules: Vec<DumpedModule>, contract: Option<&str>) -> Result<DumpedModule, String> {
    let names = modules.iter().map(|module| module.name.as_str()).collect::<Vec<_>>().join(", ");
    let Some(contract) = contract else {
        return match <[_; 1]>::try_from(modules) {
            Ok([module]) => Ok(module),
            Err(modules) if modules.is_empty() => Err("the input holds no MIR module".into()),
            Err(_) => Err(format!(
                "the input holds several modules, so choose one of {names} with `--contract`"
            )),
        };
    };
    let mut matches = modules.into_iter().filter(|module| {
        module.name == contract || module.name.rsplit(':').next() == Some(contract)
    });
    match (matches.next(), matches.next()) {
        (Some(module), None) => Ok(module),
        (None, _) => Err(format!("no module is named `{contract}`; the input holds {names}")),
        (Some(_), Some(_)) => {
            Err(format!("several modules are named `{contract}`; name one of {names} in full"))
        }
    }
}

/// Returns the calldata of the transaction to run, with the function it calls when an ABI call
/// gives it.
fn calldata(args: &Args) -> Result<(Vec<u8>, Option<Function>), String> {
    if let Some(calldata) = &args.calldata {
        let calldata =
            hex::decode(calldata).map_err(|error| format!("invalid calldata: {error}"))?;
        return Ok((calldata, None));
    }
    let Some([signature, values @ ..]) = args.call.as_deref() else {
        return Ok((Vec::new(), None));
    };
    let function = Function::parse(signature)
        .map_err(|error| format!("invalid signature `{signature}`: {error}"))?;
    if values.len() != function.inputs.len() {
        return Err(format!(
            "`{}` takes {} arguments, not {}",
            function.signature(),
            function.inputs.len(),
            values.len()
        ));
    }
    let values = function
        .inputs
        .iter()
        .zip(values)
        .map(|(param, value)| {
            param
                .resolve()
                .and_then(|ty| ty.coerce_str(value))
                .map_err(|error| format!("invalid value `{value}` for `{}`: {error}", param.ty))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let calldata = function
        .abi_encode_input(&values)
        .map_err(|error| format!("cannot encode the arguments: {error}"))?;
    Ok((calldata, Some(function)))
}

/// Parses a word in decimal or `0x` hex.
fn word(text: &str) -> Result<U256, String> {
    U256::from_str(text).map_err(|error| format!("invalid word `{text}`: {error}"))
}

/// Splits `NAME=VALUE`.
fn assignment<'a>(text: &'a str, what: &str) -> Result<(&'a str, &'a str), String> {
    text.split_once('=').ok_or_else(|| format!("expected {what} as `NAME=VALUE`, not `{text}`"))
}

/// Answers the reads of a run from what the command line sets, and zero otherwise.
struct CommandLineHost {
    storage: HashMap<U256, U256>,
    context: HashMap<u8, U256>,
    heap_start: U256,
}

impl CommandLineHost {
    fn new(args: &Args) -> Result<Self, String> {
        let mut storage = HashMap::new();
        for entry in &args.storage {
            let (slot, value) = assignment(entry, "a storage slot")?;
            storage.insert(word(slot)?, word(value)?);
        }
        let mut context = HashMap::new();
        for entry in &args.context {
            let (name, value) = assignment(entry, "a context read")?;
            let name = name.to_ascii_lowercase();
            let Some(&opcode) =
                HOST_READS.iter().find(|&&opcode| op::mnemonic(opcode) == Some(name.as_str()))
            else {
                let names = HOST_READS.iter().filter_map(|&opcode| op::mnemonic(opcode));
                return Err(format!(
                    "no context read is named `{name}`; the reads are {}",
                    names.collect::<Vec<_>>().join(", ")
                ));
            };
            let value = word(value)?;
            if ADDRESS_READS.contains(&name.as_str()) && value >> 160 != U256::ZERO {
                return Err(format!(
                    "`{name}` is an address, and {value:#x} does not fit 160 bits"
                ));
            }
            context.insert(opcode, value);
        }
        Ok(Self { storage, context, heap_start: word(&args.heap_start)? })
    }
}

impl Host for CommandLineHost {
    fn read(&mut self, opcode: u8, _operands: &[U256]) -> Option<U256> {
        Some(self.context.get(&opcode).copied().unwrap_or_default())
    }

    fn storage(&mut self, slot: U256) -> U256 {
        self.storage.get(&slot).copied().unwrap_or_default()
    }

    fn free_memory_start(&mut self) -> U256 {
        self.heap_start
    }
}

/// Describes how a run ended.
fn describe(outcome: &Outcome) -> String {
    let words = |words: &[U256]| {
        words.iter().map(|word| format!("{word:#x}")).collect::<Vec<_>>().join(", ")
    };
    match outcome {
        Outcome::Return(values) if values.is_empty() => "returned".into(),
        Outcome::Return(values) => format!("returned {}", words(values)),
        Outcome::Success(data) if data.is_empty() => "stopped".into(),
        Outcome::Success(data) => format!("returned 0x{}", hex::encode(data)),
        Outcome::Revert(data) => match revert_reason(data) {
            Some(reason) => format!("reverted with 0x{} ({reason})", hex::encode(data)),
            None => format!("reverted with 0x{}", hex::encode(data)),
        },
        Outcome::Halt => "halted".into(),
        Outcome::Unsupported(reason) => format!("could not run: {reason}"),
    }
}

/// Decodes the revert data of `Error(string)` and `Panic(uint256)`.
fn revert_reason(data: &[u8]) -> Option<String> {
    let (selector, payload) = data.split_first_chunk::<4>()?;
    let (ty, name) = match *selector {
        ERROR_SELECTOR => (DynSolType::String, "Error"),
        PANIC_SELECTOR => (DynSolType::Uint(256), "Panic"),
        _ => return None,
    };
    let value = ty.abi_decode_params(payload).ok()?;
    Some(format!("{name}({})", format_value(&value)))
}

/// Formats a decoded ABI value as Solidity writes it.
fn format_value(value: &DynSolValue) -> String {
    let list =
        |values: &[DynSolValue]| values.iter().map(format_value).collect::<Vec<_>>().join(", ");
    match value {
        DynSolValue::Bool(value) => value.to_string(),
        DynSolValue::Int(value, _) => value.to_string(),
        DynSolValue::Uint(value, _) => value.to_string(),
        DynSolValue::FixedBytes(word, size) => format!("0x{}", hex::encode(&word[..*size])),
        DynSolValue::Address(address) => address.to_checksum(None),
        DynSolValue::Bytes(bytes) => format!("0x{}", hex::encode(bytes)),
        DynSolValue::String(text) => format!("{text:?}"),
        DynSolValue::Array(values) | DynSolValue::FixedArray(values) => {
            format!("[{}]", list(values))
        }
        DynSolValue::Tuple(values) => format!("({})", list(values)),
        other => format!("{other:?}"),
    }
}

/// Reports a run as text.
fn report(execution: &Execution, decoded: Option<&[DynSolValue]>) -> String {
    let mut text = format!("{}\n", describe(&execution.outcome));
    if let Some(values) = decoded {
        let values = values.iter().map(format_value).collect::<Vec<_>>().join(", ");
        text.push_str(&format!("decoded: {values}\n"));
    }
    if !execution.logs.is_empty() {
        text.push_str("events:\n");
        for (index, log) in execution.logs.iter().enumerate() {
            let topics = log.topics.iter().map(|topic| format!("{topic:#x}")).collect::<Vec<_>>();
            text.push_str(&format!(
                "  {index}: topics [{}], data 0x{}\n",
                topics.join(", "),
                hex::encode(&log.data)
            ));
        }
    }
    for (title, slots) in
        [("storage", &execution.storage), ("transient storage", &execution.transient)]
    {
        if !slots.is_empty() {
            text.push_str(&format!("{title}:\n"));
            for (slot, value) in slots {
                text.push_str(&format!("  {slot:#x} = {value:#x}\n"));
            }
        }
    }
    text
}

/// Reports a run as JSON.
fn report_json(execution: &Execution, decoded: Option<&[DynSolValue]>) -> Json {
    let data = |data: &[u8]| json!(format!("0x{}", hex::encode(data)));
    let mut report = match &execution.outcome {
        Outcome::Return(values) => json!({
            "outcome": "return",
            "words": values.iter().map(|word| format!("{word:#x}")).collect::<Vec<_>>(),
        }),
        Outcome::Success(bytes) => json!({ "outcome": "success", "data": data(bytes) }),
        Outcome::Revert(bytes) => {
            json!({ "outcome": "revert", "data": data(bytes), "reason": revert_reason(bytes) })
        }
        Outcome::Halt => json!({ "outcome": "halt" }),
        Outcome::Unsupported(reason) => json!({ "outcome": "unsupported", "reason": reason }),
    };
    if let Some(values) = decoded {
        report["decoded"] = json!(values.iter().map(format_value).collect::<Vec<_>>());
    }
    report["logs"] = execution
        .logs
        .iter()
        .map(|log| {
            let topics = log.topics.iter().map(|topic| format!("{topic:#x}")).collect::<Vec<_>>();
            json!({ "topics": topics, "data": data(&log.data) })
        })
        .collect();
    let slots = |slots: &[(U256, U256)]| {
        let map =
            slots.iter().map(|(slot, value)| (format!("{slot:#x}"), json!(format!("{value:#x}"))));
        Json::Object(map.collect::<Map<_, _>>())
    };
    report["storage"] = slots(&execution.storage);
    report["transient"] = slots(&execution.transient);
    report
}
