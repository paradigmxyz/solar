//! Independent precompile boundary replay through evm2's real provider.
//!
//! Batches compare the Lean cryptographic functions directly with the engine, including gas
//! boundaries and malformed input. This protocol is a test/replay facility, not proof evidence:
//! world-entry verification continues to require a Lean kernel proof and fresh IR execution.

use alloy_primitives::{Address, Bytes};
use evm2::{
    BaseEvmTypes, Evm, Precompiles, TxResult,
    env::BlockEnv,
    evm::{InMemoryDB, precompile::PrecompileProvider},
    interpreter::{GasTracker, Message},
    precompiles::PrecompileHalt,
    registry::TxRegistry,
};
use serde::Deserialize;
use serde_json::{Value, json};
use solar_config::EvmVersion;
use std::io::{self, Read};
use strum::IntoEnumIterator;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Batch {
    version: u32,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    evm_version: String,
    address: u64,
    input: Vec<u8>,
    gas: u64,
}

pub(super) fn replay() -> Result<Vec<Value>, String> {
    let mut bytes = Vec::new();
    io::stdin()
        .lock()
        .take(8_388_609)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > 8_388_608 {
        return Err("precompile request exceeds 8 MiB".into());
    }
    let request: Batch = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if request.version != 1 || request.cases.len() > 512 {
        return Err("unsupported precompile batch".into());
    }
    request
        .cases
        .into_iter()
        .map(|case| {
            if case.input.len() > 65536 || case.gas > 30_000_000 {
                return Err("precompile replay limit".into());
            }
            let spec = super::world::spec(&case.evm_version)?;
            let mut evm = Evm::<BaseEvmTypes>::new(
                spec,
                BlockEnv::<BaseEvmTypes>::default(),
                TxRegistry::<BaseEvmTypes, TxResult>::new(),
                InMemoryDB::default(),
                Precompiles::base(spec),
            );
            let mut provider = Precompiles::<BaseEvmTypes>::base(spec);
            let address = Address::from_word(alloy_primitives::B256::from(
                alloy_primitives::U256::from(case.address),
            ));
            let message = Message::<BaseEvmTypes> {
                destination: address,
                code_address: address,
                input: Bytes::from(case.input),
                gas_limit: case.gas,
                ..Default::default()
            };
            let mut gas = GasTracker::new(case.gas);
            match provider.execute(&mut evm, &message, &mut gas) {
                None => Ok(json!(["inactive", [], case.gas])),
                Some(Ok(output)) => Ok(json!(["success", output.bytes(), gas.remaining()])),
                Some(Err(error)) => {
                    let Some(halt) = error.as_halt() else {
                        return Err(error.to_string());
                    };
                    Ok(json!([
                        if *halt == PrecompileHalt::OutOfGas { "out-of-gas" } else { "exception" },
                        [],
                        0
                    ]))
                }
            }
        })
        .collect()
}

pub(super) fn catalog() -> Value {
    EvmVersion::iter()
        .filter_map(|version| {
            let name = version.to_string();
            let spec = super::world::spec(&name).ok()?;
            let mut addresses = Precompiles::<BaseEvmTypes>::base(spec).addresses();
            addresses.sort_unstable();
            Some((
                name,
                json!(
                    addresses
                        .into_iter()
                        .map(|address| alloy_primitives::U256::from_be_slice(address.as_slice())
                            .to_string())
                        .collect::<Vec<_>>()
                ),
            ))
        })
        .collect::<serde_json::Map<_, _>>()
        .into()
}
