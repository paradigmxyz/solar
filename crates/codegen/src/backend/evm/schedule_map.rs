//! Where stack scheduling puts each MIR value: the data `-Zvalidate-backend` relates MIR to the
//! EVM IR that scheduling emits.
//!
//! The stack scheduler keeps value identities and virtual layouts to itself. With the flag, and
//! only then, it also writes this side table while it emits a body: for every MIR block, where its
//! code starts in the EVM IR and which value each stack word holds there, from the bottom of the
//! function's stack to the top; the EVM IR position of every push of a spill slot's address, of
//! every address the backend resolves for an instruction (a frame object, an allocation, the heap
//! floor), and the function's entry label and whether it is entered with a return address. The
//! map changes nothing the scheduler decides or emits: it is written after each decision, from the
//! plan and the assembler's insertion point.
//!
//! Positions name the EVM IR as [`Assembler::finish_evm_ir`] leaves it, before the EVM IR
//! pipeline: [`ScheduleMap::resolve`] moves them past the code that resolved allocations
//! expand to.
//!
//! [`Assembler::finish_evm_ir`]: crate::backend::assembler::Assembler

use super::ir;
use crate::{
    backend::assembler::{Assembler, DeferredAllocResolution, Label},
    mir::{BlockId, Function, FunctionId, InstId, ValueId},
};
use solar_data_structures::{index::IndexVec, map::FxHashSet};

/// A position in the EVM IR: a block and the index of an instruction in it.
pub(crate) type Position = (ir::BlockId, usize);

/// What one word of a block's entry stack holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Location {
    /// A MIR value.
    Value(ValueId),
    /// The caller's return address.
    Ret,
    /// A word no later code reads.
    Junk,
}

/// Where one MIR block's code starts and what the stack holds there.
#[derive(Clone, Debug)]
pub(crate) struct BlockMap {
    /// The first instruction of the block's code, or `None` when the block emits no code and
    /// control continues at `continues`.
    pub(crate) start: Option<Position>,
    /// The stack on entry, from the bottom of the function's words to the top.
    pub(crate) layout: Vec<Location>,
    /// Whether the layout names only the top of the stack, leaving the words below it open.
    pub(crate) floating: bool,
    /// The block whose code runs instead, for a block that only jumps on.
    pub(crate) continues: Option<BlockId>,
}

/// Where scheduling put one function's values.
#[derive(Clone, Debug)]
pub(crate) struct FunctionMap {
    pub(crate) function: FunctionId,
    /// The copy of the function the plan was made for, when the scheduler rebuilt cheap values at
    /// their uses; the layouts name its values.
    pub(crate) rebuilt: Option<Box<Function>>,
    /// The label that calls jump to, and after [`ScheduleMap::resolve`] its block.
    pub(crate) label: Option<Label>,
    pub(crate) entry: Option<ir::BlockId>,
    /// Whether the function is entered with a return address above its arguments.
    pub(crate) returns: bool,
    pub(crate) blocks: IndexVec<BlockId, Option<BlockMap>>,
    /// Every push of a spill slot's address, with the word whose slot it is.
    pub(crate) slots: Vec<(Location, Position)>,
    /// The first instruction of the code each backend-resolved instruction emits: the push of a
    /// frame object's or the heap floor's address, or a deferred allocation.
    pub(crate) placements: Vec<(InstId, Position)>,
    /// The instructions after which the function's spill slots may hold other words: writes that
    /// may reach the spill area and calls that make one or reenter the function.
    pub(crate) clobbers: FxHashSet<InstId>,
    /// Why the map cannot describe the function, such as code it emits outside the MIR.
    pub(crate) unsupported: Option<&'static str>,
}

/// The maps of the functions of one artifact, in emission order.
#[derive(Clone, Debug, Default)]
pub(crate) struct ScheduleMap {
    pub(crate) functions: Vec<FunctionMap>,
}

impl ScheduleMap {
    /// Resolves entry labels to blocks and moves every position past the instructions that
    /// resolved deferred allocations add before it, as [`Assembler::finish_evm_ir`] will expand
    /// them. Call it once allocations are resolved and before the program is finished.
    ///
    /// [`Assembler::finish_evm_ir`]: crate::backend::assembler::Assembler
    pub(crate) fn resolve(&mut self, asm: &Assembler<'_>) {
        // Every allocation in the program, with the instructions its resolution adds.
        let growth: Vec<Position> = asm
            .alloc_relocations
            .iter()
            .filter_map(|&(block, index, alloc)| match asm.deferred_allocations.get(&alloc)? {
                DeferredAllocResolution::Static(_) => None,
                DeferredAllocResolution::Dynamic(_) => Some((block, index)),
            })
            .collect();
        // A dynamic allocation expands one placeholder to seven instructions.
        let moved = |(block, index): Position| {
            let before = growth.iter().filter(|&&(other, at)| other == block && at < index).count();
            (block, index + before * (DYNAMIC_ALLOCATION_LEN - 1))
        };
        for function in &mut self.functions {
            function.entry = function.label.and_then(|label| asm.label_block(label));
            for block in function.blocks.iter_mut().flatten() {
                block.start = block.start.map(moved);
            }
            for (_, position) in &mut function.slots {
                *position = moved(*position);
            }
            for (_, position) in &mut function.placements {
                *position = moved(*position);
            }
        }
    }
}

/// The instructions a dynamically placed allocation expands to: see
/// [`Assembler::finish_evm_ir`](crate::backend::assembler::Assembler).
const DYNAMIC_ALLOCATION_LEN: usize = 7;
