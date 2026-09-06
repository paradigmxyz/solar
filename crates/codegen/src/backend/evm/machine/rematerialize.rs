//! Immutable recipes at the physical stack boundary.
//!
//! For calldata recipes, ordinary stack-pressure planning runs first, including any protocol
//! recheck. Only its actual
//! memory-home candidates are considered here. Gas optimization requires exclusively eligible
//! homes, so selection cannot fragment a mixed bank and disable its compact writer protection.
//! Other modes do not use that protection and select individual homes. Stack-resident calldata
//! reads keep their existing definitions and schedules. Selected homes become cached literal-offset
//! CALLDATALOAD recipes. Reserved words, Phi scratch and spill-protocol decisions remain unchanged,
//! deliberately leaving unused storage rather than adding another allocation or scheduling
//! analysis.
//!
//! Offsets are immediate words or one ADD of two immediate words, evaluated with full EVM wrapping.
//! Noncanonical effects, Phi values, arguments and other reads decline. Selection follows at most
//! one arithmetic producer, scans only existing homes, and performs no MIR rewrite. Calldata is
//! immutable within the EVM activation, including across returning calls. Constructor argument
//! memory/code reads are not recipes. Other values and activation protocol storage still require
//! their existing protection; this is not a general source-memory interference solution.
//!
//! In unoptimized mode, fourteen stable two-gas nullary reads need neither a resident value nor
//! a spill home. Optimized modes retain ordinary residency and scheduling. The
//! constant-time classifier is shared by ordinary planning, definition suppression and consumer
//! emission. Only canonical environment reads available on the target qualify. NUMBER retains
//! its evaluated value for instrumented EVMs; mutable or more expensive reads also remain stored.

use crate::{
    backend::evm::op,
    mir::{self, EffectKind},
    utils::eval,
};
use alloy_primitives::U256;
use solar_config::{EvmVersion, OptimizationMode};
use solar_data_structures::map::FxHashMap;

/// Replaces eligible home identities while leaving their reserved offsets and counts untouched.
pub(super) fn select(
    function: &mir::Function,
    homes: &mut FxHashMap<mir::ValueId, usize>,
    preserve_banks: bool,
) -> FxHashMap<mir::ValueId, U256> {
    let mut recipes = FxHashMap::default();
    for &value in homes.keys() {
        if let Some(offset) = calldata(function, value) {
            recipes.insert(value, offset);
        } else if preserve_banks {
            return FxHashMap::default();
        }
    }
    // <selected immutable homes> -> <cached recipes>, preserving mixed gas-mode banks
    for value in recipes.keys() {
        homes.remove(value);
    }
    recipes
}

/// Returns an available, stable two-gas read for unoptimized instruction selection.
pub(super) fn nullary(
    function: &mir::Function,
    value: mir::ValueId,
    version: EvmVersion,
    optimization: OptimizationMode,
) -> Option<u8> {
    if !matches!(optimization, OptimizationMode::None) {
        return None;
    }
    let mir::Value::Inst(id) = function.value(value) else { return None };
    let instruction = function.inst(*id);
    let opcode = instruction.kind.evm_opcode()?;
    (matches!(
        opcode,
        op::CALLDATASIZE
            | op::CODESIZE
            | op::CALLER
            | op::CALLVALUE
            | op::ADDRESS
            | op::ORIGIN
            | op::GASPRICE
            | op::COINBASE
            | op::TIMESTAMP
            | op::PREVRANDAO
            | op::GASLIMIT
            | op::CHAINID
            | op::BASEFEE
            | op::BLOBBASEFEE
    ) && function.inst_result_value(*id) == Some(value)
        && instruction.metadata.effect().is_none_or(|effect| effect == EffectKind::EnvironmentRead)
        && op::available(opcode, version))
    .then_some(opcode)
}

/// Returns the exact offset of an eligible immutable calldata result.
fn calldata(function: &mir::Function, value: mir::ValueId) -> Option<U256> {
    let mir::Value::Inst(id) = function.value(value) else { return None };
    let instruction = function.inst(*id);
    if let mir::InstKind::CalldataLoad(offset) = instruction.kind
        && function.inst_result_value(*id) == Some(value)
        && instruction.metadata.effect().is_none_or(|effect| effect == EffectKind::EnvironmentRead)
    {
        return function.value_u256(offset).or_else(|| {
            let mir::Value::Inst(id) = function.value(offset) else { return None };
            let instruction = function.inst(*id);
            if let mir::InstKind::Add(first, second) = instruction.kind
                && function.inst_result_value(*id) == Some(offset)
                && instruction.metadata.effect().is_none_or(|effect| effect == EffectKind::Pure)
                && let Some(first) = function.value_u256(first)
                && let Some(second) = function.value_u256(second)
            {
                return eval::eval_opcode(op::ADD, &[first, second]);
            }
            None
        });
    }
    None
}
