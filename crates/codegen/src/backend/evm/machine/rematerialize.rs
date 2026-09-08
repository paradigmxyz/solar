//! Immutable recipes at the physical stack boundary.
//!
//! For calldata recipes, ordinary stack-pressure planning runs first, including any protocol
//! recheck. Only its actual memory-home candidates are considered here. Gas optimization requires
//! exclusively eligible homes, so selection cannot fragment a mixed bank and disable its compact
//! writer protection.
//! Other modes do not use that protection and select individual homes. Stack-resident calldata
//! reads keep their existing definitions and schedules. Selected homes become cached literal-offset
//! CALLDATALOAD recipes. Reserved words, Phi scratch and spill-protocol decisions remain unchanged,
//! deliberately leaving unused storage rather than adding another allocation or scheduling
//! analysis.
//!
//! Computed recipes additionally admit a whole immutable bank containing one ADD, SUB, AND, OR
//! or XOR over a fixed-offset calldata read and a literal. Each computed result has exactly one
//! ordinary same-block use. Canonical ADDs of two immediate words may complete such a bank
//! (for example, a static ABI output cursor); their cached literals may have multiple ordinary
//! same-block uses. A bank must still contain a computed calldata recipe. One bounded use scan
//! suppresses the original definitions and only those calldata/constant-offset dependencies whose
//! uses all remain inside the same-block
//! suppressed closure. Shared producers keep their definitions. This avoids charging profitability
//! to a particular writer or leaving compute-and-discard work. Failed admission retains exactly
//! the existing calldata selection policy.
//!
//! The caller excludes construction, internal-call artifacts, returning owners and dynamic frames:
//! a callee's tracked stack omits suspended ancestor words. This helper admits only native opcodes
//! with at most three operands, no outgoing tail call, at most 256 value IDs and 512 instruction
//! IDs. The ID bounds keep analysis small. A nonempty home bank comes from the spill-planning path,
//! which retains at most eight overlapping resident values. After clearing that bank, at most
//! three prepared operands and one binary-recipe temporary bring the stack peak to twelve words,
//! with no suspended caller prefix. Cleanup at suppressed definitions removes dead dependencies;
//! ordinary scheduling still checks reachability. Original reservations and protocol choices stay
//! fixed. Local
//! expression equivalence does not prove final gas/size profitability: literal sharing, cleanup,
//! memory expansion and later target passes remain measured admission gates.
//!
//! Offsets are immediate words or one ADD of two immediate words, evaluated with full EVM wrapping.
//! Noncanonical effects, Phi values, arguments and other reads decline. Selection follows at most
//! one arithmetic producer for an offset and performs no MIR rewrite. Calldata is immutable within
//! the EVM activation, including across returning calls. Constructor argument memory/code reads are
//! not recipes. Other values and activation protocol storage still require
//! their existing protection; this is not a general source-memory interference solution.
//!
//! In unoptimized mode, fourteen stable two-gas nullary reads need neither a resident value nor
//! a spill home. Optimized modes retain ordinary residency and scheduling. The
//! constant-time classifier is shared by ordinary planning, definition suppression and consumer
//! emission. Only canonical environment reads available on the target qualify. NUMBER retains
//! its evaluated value for instrumented EVMs; mutable or more expensive reads also remain stored.

use crate::{
    backend::evm::{ir, op},
    mir::{self, EffectKind, utils::eval},
};
use alloy_primitives::U256;
use solar_config::{EvmVersion, OptimizationMode};
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};

/// An immutable expression whose emitted result occupies one stack word.
#[derive(Clone, Copy)]
pub(super) enum Recipe {
    Calldata(U256),
    Literal(U256),
    Binary { offset: U256, literal: U256, opcode: u8, calldata_first: bool },
}

/// Emits one cached immutable recipe with its original operand order.
pub(super) fn emit(recipe: &Recipe, output: &mut Vec<ir::Instruction>) {
    match *recipe {
        Recipe::Literal(value) => {
            // push <folded constant word>
            output.push(ir::InstKind::Push(value).into());
        }
        Recipe::Calldata(offset) => {
            // push <constant calldata offset>
            // calldataload
            output.push(ir::InstKind::Push(offset).into());
            output.push(ir::InstKind::Op(op::CALLDATALOAD).into());
        }
        Recipe::Binary { offset, literal, opcode, calldata_first } => {
            // push <literal> when it is the second operand
            // push <constant calldata offset>; calldataload
            // push <literal> when it is the first operand
            // opcode
            if calldata_first {
                output.push(ir::InstKind::Push(literal).into());
            }
            output.push(ir::InstKind::Push(offset).into());
            output.push(ir::InstKind::Op(op::CALLDATALOAD).into());
            if !calldata_first {
                output.push(ir::InstKind::Push(literal).into());
            }
            output.push(ir::InstKind::Op(opcode).into());
        }
    }
}

/// Keeps unchanged functions free of an instruction-domain suppression allocation.
#[derive(Default)]
pub(super) struct Selection {
    pub(super) recipes: FxHashMap<mir::ValueId, Recipe>,
    pub(super) suppressed: Option<DenseBitSet<mir::InstId>>,
}

/// Replaces eligible home identities while leaving their reserved offsets and counts untouched.
pub(super) fn select(
    function: &mir::Function,
    homes: &mut FxHashMap<mir::ValueId, usize>,
    preserve_banks: bool,
    allow_computed: bool,
) -> Selection {
    let mut selection = Selection::default();
    let mut complete = true;
    for &value in homes.keys() {
        if let Some(offset) = calldata(function, value) {
            selection.recipes.insert(value, Recipe::Calldata(offset));
        } else {
            complete = false;
            if preserve_banks && !allow_computed {
                return Selection::default();
            }
        }
    }
    if !complete
        && allow_computed
        && let Some(computed) = computed(function, homes)
    {
        // <whole immutable bank and closed producers> -> <recipes at their consumers>
        homes.clear();
        return computed;
    }
    if !complete && preserve_banks {
        return Selection::default();
    }
    // <selected calldata homes> -> <cached recipes>, preserving mixed gas-mode banks
    for value in selection.recipes.keys() {
        homes.remove(value);
    }
    selection
}

/// Records only candidate identities, rather than allocating use lists for every MIR value.
#[derive(Default)]
struct Uses {
    definition: Option<mir::BlockId>,
    users: Vec<(mir::BlockId, Option<mir::InstId>)>,
}

/// Selects a closed bank with bounded expression depth and a single same-block computed use.
fn computed(function: &mir::Function, homes: &FxHashMap<mir::ValueId, usize>) -> Option<Selection> {
    // Bound the analysis and leave ample room for two-word recipes above ordinary operands.
    // The caller separately excludes suspended internal-call prefixes and dynamic protocols.
    if function.num_values() > 256 || function.num_insts() > 512 {
        return None;
    }
    let mut recipes = FxHashMap::default();
    let mut uses = FxHashMap::<_, Uses>::default();
    for &value in homes.keys() {
        let (recipe, read) = if let Some(offset) = calldata(function, value) {
            (Recipe::Calldata(offset), Some(value))
        } else if let Some(literal) = constant_add(function, value) {
            (Recipe::Literal(literal), None)
        } else {
            let (recipe, read) = binary(function, value)?;
            (recipe, Some(read))
        };
        recipes.insert(value, recipe);
        uses.entry(value).or_default();
        if let Some(read) = read {
            uses.entry(read).or_default();
            if let mir::Value::Inst(id) = function.value(read)
                && let mir::InstKind::CalldataLoad(offset) = function.inst(*id).kind
                && matches!(function.value(offset), mir::Value::Inst(_))
            {
                uses.entry(offset).or_default();
            }
        }
    }
    if !recipes.values().any(|recipe| matches!(recipe, Recipe::Binary { .. })) {
        return None;
    }
    // Count occurrences, including repeated operands, terminators and unreachable blocks.
    // An auxiliary producer is removable only when every use is in this same-block closure.
    for (block_id, block) in function.blocks.iter_enumerated() {
        for &id in &block.instructions {
            let instruction = function.inst(id);
            let operands = instruction.kind.operands();
            if instruction.kind.evm_opcode().is_none() || operands.len() > 3 {
                return None;
            }
            if let Some(value) = function.inst_result_value(id)
                && let Some(entry) = uses.get_mut(&value)
            {
                entry.definition = Some(block_id);
            }
            for value in operands {
                if let Some(entry) = uses.get_mut(&value) {
                    entry.users.push((block_id, Some(id)));
                }
            }
        }
        if let Some(term) = &block.terminator {
            if matches!(term, mir::Terminator::TailCall { .. }) {
                return None;
            }
            for value in term.operands() {
                if let Some(entry) = uses.get_mut(&value) {
                    entry.users.push((block_id, None));
                }
            }
        }
    }
    let mut suppressed = DenseBitSet::new_empty(function.num_insts());
    for (&value, recipe) in &recipes {
        let entry = &uses[&value];
        let defined = entry.definition?;
        if matches!(recipe, Recipe::Binary { .. })
            && !matches!(entry.users.as_slice(), [(block, Some(_))] if *block == defined)
        {
            return None;
        }
        if matches!(recipe, Recipe::Literal(_))
            && entry.users.iter().any(|(block, user)| *block != defined || user.is_none())
        {
            return None;
        }
        let mir::Value::Inst(id) = function.value(value) else { return None };
        suppressed.insert(*id);
    }
    // <binary root>; <calldata dependency>; <optional constant-offset ADD>
    // Remove only closed same-block dependencies, in at most two dependency layers.
    for _ in 0..2 {
        for (&value, entry) in &uses {
            if let Some(defined) = entry.definition
                && !entry.users.is_empty()
                && entry.users.iter().all(|(block, user)| {
                    *block == defined && user.is_some_and(|id| suppressed.contains(id))
                })
                && let mir::Value::Inst(id) = function.value(value)
            {
                suppressed.insert(*id);
            }
        }
    }
    Some(Selection { recipes, suppressed: Some(suppressed) })
}

/// Recognizes one legacy three-gas word operation, preserving its operand order.
fn binary(function: &mir::Function, value: mir::ValueId) -> Option<(Recipe, mir::ValueId)> {
    let mir::Value::Inst(id) = function.value(value) else { return None };
    let instruction = function.inst(*id);
    if function.inst_result_value(*id) != Some(value)
        || instruction.metadata.effect().is_some_and(|effect| effect != EffectKind::Pure)
    {
        return None;
    }
    let (opcode, first, second) = match instruction.kind {
        mir::InstKind::Add(a, b) => (op::ADD, a, b),
        mir::InstKind::Sub(a, b) => (op::SUB, a, b),
        mir::InstKind::And(a, b) => (op::AND, a, b),
        mir::InstKind::Or(a, b) => (op::OR, a, b),
        mir::InstKind::Xor(a, b) => (op::XOR, a, b),
        _ => return None,
    };
    for (read, constant, calldata_first) in [(first, second, true), (second, first, false)] {
        if let Some(literal) = function.value_u256(constant)
            && let Some(offset) = calldata(function, read)
        {
            return Some((Recipe::Binary { offset, literal, opcode, calldata_first }, read));
        }
    }
    None
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
        return function.value_u256(offset).or_else(|| constant_add(function, offset));
    }
    None
}

/// Evaluates exactly one canonical ADD of two immediate words, with EVM wrapping.
fn constant_add(function: &mir::Function, value: mir::ValueId) -> Option<U256> {
    let mir::Value::Inst(id) = function.value(value) else { return None };
    let instruction = function.inst(*id);
    if let mir::InstKind::Add(first, second) = instruction.kind
        && function.inst_result_value(*id) == Some(value)
        && instruction.metadata.effect().is_none_or(|effect| effect == EffectKind::Pure)
        && let Some(first) = function.value_u256(first)
        && let Some(second) = function.value_u256(second)
    {
        return eval::eval_opcode(op::ADD, &[first, second]);
    }
    None
}
