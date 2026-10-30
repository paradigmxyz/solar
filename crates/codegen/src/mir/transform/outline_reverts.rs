//! Outline duplicate constant revert blocks into shared helpers.
//!
//! Panic checks and argless custom errors lower to small blocks of constant
//! stores followed by `revert` — `mstore(32, code); mstore(0, selector);
//! revert(28, 36)` — and the same shape repeats at every check site, across
//! functions. EVM IR also deduplicates equivalent terminal blocks and merges
//! common terminal tails, but only after backend lowering. Outlining the
//! semantic MIR shape first prevents stack scheduling and layout differences
//! from hiding those duplicates from the backend passes.
//!
//! This pass hashes every block whose instructions are all `mstore(imm, imm)`
//! and whose terminator is `revert(imm, imm)`. A shape whose copies take more
//! bytes under the target model than a jump at each site plus one landed copy
//! is synthesized once as a shared no-return helper, and each occurrence is
//! rewritten into an argless `tail_call` to it — a bare jump in the backend.
//! Only bytes are priced: the extra jump runs on the failure path alone. When
//! optimizing for size, `Target::shares_terminal_copy` leaves shapes no larger
//! than a jump to and from a helper to the backend's terminal sharing.

use crate::{
    backend::evm::op,
    mir::{
        BlockId, Function, FunctionBuilder, FunctionId, InstKind, MirModuleTarget, Module,
        PtrLocation, Terminator, Value, pass::MirPass,
    },
    target::{Cost, Target},
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_data_structures::map::FxHashMap;
use solar_interface::{Ident, sym};

/// Revert-block outlining pass.
pub(crate) struct OutlineReverts;

impl MirPass for OutlineReverts {
    fn name(&self) -> &'static str {
        "outline-reverts"
    }

    fn is_contract_wide(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let target = module.target(gcx);
        outline_reverts(module, &target)
    }
}

/// A constant revert block: `mstore(offset, value)*` then `revert(offset, size)`.
type RevertShape = (SmallVec<[(U256, U256); 2]>, U256, U256);

fn outline_reverts(module: &mut Module, target: &Target) -> bool {
    // Collect every constant revert block, keyed by shape.
    let mut shapes = FxHashMap::<RevertShape, Vec<(FunctionId, BlockId)>>::default();
    for (func_id, func) in module.functions.iter_enumerated() {
        for block_id in func.blocks.indices() {
            if let Some(shape) = constant_revert_shape(func, block_id) {
                shapes.entry(shape).or_default().push((func_id, block_id));
            }
        }
    }

    let mut worth_outlining: Vec<_> = shapes
        .into_iter()
        .filter(|(shape, sites)| saves_bytes(target, shape, sites.len()))
        .collect();
    if worth_outlining.is_empty() {
        return false;
    }
    // Deterministic helper numbering and output.
    worth_outlining.sort_by(|a, b| a.1.cmp(&b.1));

    for (shape, sites) in worth_outlining {
        let helper = create_revert(module, &shape);
        for (func_id, block_id) in sites {
            // mstore(imm, imm)*; revert(imm, imm) => tail_call @revert_stub
            let block = &mut module.functions[func_id].blocks[block_id];
            block.instructions.clear();
            block.terminator =
                Some(Terminator::TailCall { function: helper, args: Default::default() });
        }
    }
    true
}

fn create_revert(module: &mut Module, (stores, offset, size): &RevertShape) -> FunctionId {
    let mut func = Function::new(Ident::with_dummy_span(sym::revert_stub));
    {
        let mut builder = FunctionBuilder::new(&mut func);
        for &(store_offset, value) in stores {
            let store_offset = builder.imm(store_offset);
            let value = builder.imm(value);
            builder.store(store_offset, value);
        }
        let offset = builder.imm(*offset);
        let size = builder.imm(*size);
        builder.revert(offset, size);
    }
    module.add_function(func)
}

/// Returns the block's shape when every instruction is a fully-constant
/// `mstore` and the terminator is a fully-constant `revert`.
fn constant_revert_shape(func: &Function, block: BlockId) -> Option<RevertShape> {
    let block = &func.blocks[block];
    let Some(Terminator::Revert { offset, size }) = block.terminator else {
        return None;
    };
    let imm = |v| match func.value(v) {
        Value::Immediate(imm) => imm.as_u256(),
        _ => None,
    };
    let mut stores = SmallVec::with_capacity(block.instructions.len());
    for &inst_id in &block.instructions {
        let InstKind::Store(store_offset, value, PtrLocation::Memory) = func.inst(inst_id).kind
        else {
            return None;
        };
        stores.push((imm(store_offset)?, imm(value)?));
    }
    Some((stores, imm(offset)?, imm(size)?))
}

/// Whether sharing one copy of a shape among its `sites` pays under the target model.
fn saves_bytes(target: &Target, (stores, offset, size): &RevertShape, sites: usize) -> bool {
    let copy = stores
        .iter()
        .map(|&(offset, value)| {
            target.push(offset) + target.push(value) + target.opcode(op::MSTORE)
        })
        .sum::<Cost>()
        + target.push(*offset)
        + target.push(*size)
        + target.opcode(op::REVERT);
    target.shares_terminal_copy(copy, u32::try_from(sites).unwrap_or(u32::MAX))
}
