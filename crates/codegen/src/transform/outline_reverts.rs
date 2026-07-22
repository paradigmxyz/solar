//! Outline duplicate constant revert blocks into shared helpers.
//!
//! Panic checks and argless custom errors lower to small blocks of constant
//! stores followed by `revert` — `mstore(0, selector); mstore(4, code);
//! revert(0, 36)` — and the same shape repeats at every check site, across
//! functions. EVM IR also deduplicates equivalent terminal blocks and merges
//! common terminal tails, but only after backend lowering. Outlining the
//! semantic MIR shape first prevents stack scheduling and layout differences
//! from hiding those duplicates from the backend passes.
//!
//! This pass hashes every block whose instructions are all `mstore(imm, imm)`
//! and whose terminator is `revert(imm, imm)`. A shape that occurs at least
//! twice (and is big enough for the jump to pay for itself) is synthesized
//! once as a shared no-return helper, and each occurrence is rewritten into an
//! argless `tail_call` to it — a bare jump in the backend, on both the
//! MIR-dispatch and backend-dispatcher paths.

use crate::{
    mir::{Function, FunctionBuilder, InstKind, Module, Terminator, Value},
    pass::MirPass,
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_data_structures::map::FxHashMap;
use solar_interface::{Ident, Symbol};

/// Statistics from revert-block outlining.
#[derive(Clone, Debug, Default)]
pub(crate) struct OutlineRevertsStats {
    /// Number of blocks rewritten into tail calls.
    pub outlined: usize,
    /// Number of shared helpers synthesized.
    pub helpers: usize,
}

/// Revert-block outlining pass.
pub(crate) struct OutlineRevertsPass;

#[derive(Debug, Default)]
struct OutlineReverts {
    stats: OutlineRevertsStats,
}

/// A constant revert block: `mstore(offset, value)*` then `revert(offset, size)`.
type RevertShape = (SmallVec<[(U256, U256); 2]>, U256, U256);

impl OutlineReverts {
    fn run(&mut self, module: &mut Module) -> bool {
        // Collect every constant revert block, keyed by shape.
        let mut shapes: FxHashMap<RevertShape, Vec<(usize, usize)>> = FxHashMap::default();
        for (func_idx, func) in module.functions.iter().enumerate() {
            for block_idx in 0..func.blocks.len() {
                if let Some(shape) = constant_revert_shape(func, block_idx)
                    && estimated_inline_size(&shape) >= MIN_OUTLINED_SIZE
                {
                    shapes.entry(shape).or_default().push((func_idx, block_idx));
                }
            }
        }

        let mut worth_outlining: Vec<(RevertShape, Vec<(usize, usize)>)> =
            shapes.into_iter().filter(|(_, sites)| sites.len() >= 2).collect();
        if worth_outlining.is_empty() {
            return false;
        }
        // Deterministic helper numbering and output.
        worth_outlining.sort_by(|a, b| a.1.cmp(&b.1));

        for (shape, sites) in worth_outlining {
            let helper = self.synthesize_helper(module, &shape);
            for (func_idx, block_idx) in sites {
                let func = &mut module.functions[crate::mir::FunctionId::from_usize(func_idx)];
                let block = &mut func.blocks[crate::mir::BlockId::from_usize(block_idx)];
                block.instructions.clear();
                block.terminator =
                    Some(Terminator::TailCall { function: helper, args: Default::default() });
                self.stats.outlined += 1;
            }
        }

        self.stats.outlined != 0
    }

    fn synthesize_helper(
        &mut self,
        module: &mut Module,
        (stores, offset, size): &RevertShape,
    ) -> crate::mir::FunctionId {
        let name = format!("__revert_stub{}", self.stats.helpers);
        self.stats.helpers += 1;
        let mut func = Function::new(Ident::with_dummy_span(Symbol::intern(&name)));
        func.attributes.no_inline = true;
        {
            let mut builder = FunctionBuilder::new(&mut func);
            for &(store_offset, value) in stores {
                let store_offset = builder.imm_u256(store_offset);
                let value = builder.imm_u256(value);
                builder.mstore(store_offset, value);
            }
            let offset = builder.imm_u256(*offset);
            let size = builder.imm_u256(*size);
            builder.revert(offset, size);
        }
        module.add_function(func)
    }
}

impl MirPass for OutlineRevertsPass {
    fn name(&self) -> &'static str {
        "outline-reverts"
    }

    fn run_pass(&self, _gcx: solar_sema::Gcx<'_>, module: &mut Module) -> bool {
        OutlineReverts::default().run(module)
    }

    fn is_required(&self) -> bool {
        false
    }
}

/// Below this estimated inline footprint the jump to a helper saves nothing.
const MIN_OUTLINED_SIZE: usize = 12;

/// Returns the block's shape when every instruction is a fully-constant
/// `mstore` and the terminator is a fully-constant `revert`.
fn constant_revert_shape(func: &Function, block_idx: usize) -> Option<RevertShape> {
    let block = &func.blocks[crate::mir::BlockId::from_usize(block_idx)];
    let Some(Terminator::Revert { offset, size }) = block.terminator else {
        return None;
    };
    let imm = |v| match func.value(v) {
        Value::Immediate(imm) => imm.as_u256(),
        _ => None,
    };
    let mut stores = SmallVec::with_capacity(block.instructions.len());
    for &inst_id in &block.instructions {
        let InstKind::MStore(store_offset, value) = func.instructions[inst_id].kind else {
            return None;
        };
        stores.push((imm(store_offset)?, imm(value)?));
    }
    Some((stores, imm(offset)?, imm(size)?))
}

/// Rough emitted size of the shape when left inline: pushes at minimal width
/// plus one byte per operation.
fn estimated_inline_size((stores, offset, size): &RevertShape) -> usize {
    let push = |v: &U256| 1 + v.byte_len();
    stores.iter().map(|(o, v)| push(o) + push(v) + 1).sum::<usize>() + push(offset) + push(size) + 1
}
