//! Bounded JSON bridge to the Rust MIR and EVM IR interpreters.
//!
//! Requests contain original canonical IR and explicit live-in words, never a translated
//! operation recipe. Output observations are compared with Lean by the Python checker. Parse
//! failures, unsupported states and limits are reported explicitly instead of certifying a bug.

use alloy_primitives::U256;
use serde::Deserialize;
use serde_json::json;
use solar_config::EvmVersion;
use solar_evmir_interpreter::{Halt, Options as EvmOptions, replay as evm};
use solar_mir_interpreter::replay as mir;
use std::{
    collections::BTreeMap,
    io::{self, Read},
    str::FromStr,
};
use strum::IntoEnumIterator;

mod mir_model;
mod precompile;
mod world;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u32,
    kind: String,
    before: String,
    after: String,
    inputs: BTreeMap<String, String>,
    memory: bool,
    gas: bool,
    evm_version: String,
    #[serde(default)]
    selector: Option<String>,
    #[serde(default)]
    world: Option<world::Input>,
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--mir-model") {
        let output = match mir_model::export() {
            Ok(model) => json!({"version":1,"status":"ok","model":model}),
            Err(reason) => json!({"version":1,"status":"unsupported","reason":reason}),
        };
        println!("{output}");
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("--precompiles") {
        let output = match precompile::replay() {
            Ok(observations) => json!({"version": 1, "status": "ok", "observations": observations}),
            Err(reason) => json!({"version": 1, "status": "unsupported", "reason": reason}),
        };
        println!("{output}");
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("--catalog") {
        println!("{}", catalog());
        return;
    }
    let output = match read().and_then(|request| replay(&request)) {
        Ok(observations) => json!({"version": 1, "status": "ok", "observations": observations}),
        Err(reason) => json!({"version": 1, "status": "unsupported", "reason": reason}),
    };
    println!("{output}");
}

fn read() -> Result<Request, String> {
    let mut bytes = Vec::new();
    io::stdin().lock().take(8_388_609).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() > 8_388_608 {
        return Err("replay request exceeds 8 MiB".into());
    }
    let request: Request = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let limit = if request.selector.is_some() { 1_048_576 } else { 131_072 };
    if request.version != 1 || request.before.len() > limit || request.after.len() > limit {
        return Err("unsupported replay version or oversized IR".into());
    }
    Ok(request)
}

fn word(request: &Request, name: &str) -> Result<U256, String> {
    request
        .inputs
        .get(name)
        .map_or(Ok(U256::ZERO), |value| U256::from_str_radix(value, 10).map_err(|e| e.to_string()))
}

fn words(request: &Request, prefix: &str) -> Result<Vec<U256>, String> {
    (0..request.inputs.len())
        .take_while(|i| request.inputs.contains_key(&format!("{prefix}{i}")))
        .map(|i| word(request, &format!("{prefix}{i}")))
        .collect()
}

fn initial_memory(request: &Request) -> Result<(Vec<u8>, u64), String> {
    if !request.memory {
        return Ok((Vec::new(), 0));
    }
    let count = word(request, "memoryWords")?;
    if count > U256::from((1 << 24) / 32) {
        return Err("initial memory exceeds Rust replay's 16 MiB limit".into());
    }
    let size = count.to::<u64>() * 32;
    let mut bytes = Vec::new();
    for i in 0..8 {
        bytes.extend(word(request, &format!("memory{i}"))?.to_be_bytes::<32>());
    }
    bytes.truncate(size.min(256) as usize);
    Ok((bytes, size))
}

fn list(values: impl IntoIterator<Item = U256>) -> String {
    format!("[{}]", values.into_iter().map(|word| word.to_string()).collect::<Vec<_>>().join(", "))
}

fn memory_text(memory: &[u8], size: u64) -> String {
    format!(
        "msize={size}, memory={}",
        list(memory.as_chunks::<32>().0.iter().map(|word| U256::from_be_bytes(*word)))
    )
}

fn payload(kind: &str, data: &[u8]) -> String {
    if data.is_empty() { format!("{kind}(empty)") } else { format!("{kind}(data={data:?})") }
}

fn replay(request: &Request) -> Result<[String; 2], String> {
    if let Some(input) = &request.world {
        return world::replay(request, input);
    }
    let version = EvmVersion::from_str(&request.evm_version).map_err(|e| e.to_string())?;
    let run = |text: &str| -> Result<String, String> {
        let (memory, size) = initial_memory(request)?;
        match request.kind.as_str() {
            "mir" => {
                if request.gas {
                    return Err("MIR replay cannot assign EVM gas costs".into());
                }
                let execution = mir::run_function(
                    text,
                    request.selector.as_deref(),
                    &words(request, "arg")?,
                    &memory,
                    size,
                    solar_mir_interpreter::Options { evm_version: version, ..Default::default() },
                )?;
                Ok(match execution.outcome {
                    mir::Outcome::Return(values) if values.len() == 1 => {
                        if request.memory {
                            format!(
                                "return={}, {}",
                                values[0],
                                memory_text(&execution.memory, execution.memory_size)
                            )
                        } else {
                            values[0].to_string()
                        }
                    }
                    mir::Outcome::Revert(data)
                        if data.len() == 36 && data[..4] == [0x4e, 0x48, 0x7b, 0x71] =>
                    {
                        format!("revert Panic({})", U256::from_be_slice(&data[4..]))
                    }
                    mir::Outcome::Revert(data) => payload("revert", &data),
                    mir::Outcome::Invalid => "invalid".into(),
                    mir::Outcome::Unsupported(reason) => return Err(reason),
                    _ => return Err("non-scalar MIR replay result".into()),
                })
            }
            "evmir" => {
                let input = evm::Input {
                    stack: words(request, "stack")?,
                    memory,
                    memory_size: size as usize,
                    calldata_size: word(request, "calldatasize")?,
                    returndata_size: word(request, "returndatasize")?,
                    gas: if request.gas { Some(word(request, "gas")?) } else { None },
                };
                let execution = evm::run_window(
                    text,
                    request.selector.as_deref(),
                    input,
                    EvmOptions { evm_version: version, ..Default::default() },
                )?;
                let mut outcome = match execution.outcome {
                    evm::Outcome::Edge(target) => {
                        let target = target.map_or("fallthrough".into(), |id| format!("bb{id}"));
                        let mut value = format!("{target}, stack={}", list(execution.stack));
                        if request.memory {
                            value.push_str(&format!(
                                ", {}",
                                memory_text(&execution.memory, execution.memory_size as u64)
                            ));
                        }
                        value
                    }
                    evm::Outcome::Success(data) => payload("success", &data),
                    evm::Outcome::Revert(data) => payload("revert", &data),
                    evm::Outcome::Halt(Halt::OutOfGas) => "out-of-gas".into(),
                    evm::Outcome::Halt(Halt::Invalid) => "exception(invalid-instruction)".into(),
                    evm::Outcome::Halt(Halt::StackUnderflow) => "exception(stack-underflow)".into(),
                    evm::Outcome::Halt(Halt::StackOverflow) => "exception(stack-overflow)".into(),
                    evm::Outcome::Halt(reason) => {
                        return Err(format!("unmodeled replay halt: {reason}"));
                    }
                    evm::Outcome::Unsupported(reason) => return Err(reason),
                };
                if let Some(gas) = execution.gas {
                    outcome.push_str(&format!(", gas={gas}"));
                }
                Ok(outcome)
            }
            _ => Err("unknown replay layer".into()),
        }
    };
    Ok([run(&request.before)?, run(&request.after)?])
}

/// Reflects both IR schemas without maintaining a second instruction registry.
fn catalog() -> serde_json::Value {
    let versions = EvmVersion::iter().collect::<Vec<_>>();
    let mir = solar_mir::InstKind::DECLARED_SEMANTICS
        .iter()
        .map(|(name, semantics)| json!({"name": name, "semantics": semantics}))
        .collect::<Vec<_>>();
    let evmir = (0..=u8::MAX)
        .filter_map(solar_evmir::op::definition)
        .map(|definition| {
            let forks = versions
                .iter()
                .map(|&version| {
                    (
                        version.to_string(),
                        json!({
                            "available": definition.is_available(version),
                            "base_gas": definition.gas.gas(version),
                            "dynamic_gas": definition.gas.dynamic_gas(version),
                        }),
                    )
                })
                .collect::<serde_json::Map<_, _>>();
            json!({
                "name": definition.mnemonic,
                "opcode": definition.opcode,
                "stack": definition.stack_io,
                "pure": definition.traits.contains(solar_evmir::op::OpcodeTraits::PURE),
                "gas": format!("{:?}", definition.gas),
                "availability": format!("{:?}", definition.availability),
                "forks": forks,
            })
        })
        .collect::<Vec<_>>();
    json!({"version": 1, "mir": mir, "evmir": evmir, "precompiles": precompile::catalog()})
}
