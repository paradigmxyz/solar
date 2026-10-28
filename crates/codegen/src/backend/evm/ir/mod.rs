//! EVM backend IR.
//!
//! The IR itself, its text format, and its verifier live in `solar_evmir`, re-exported here. This
//! module adds what compiles it: the EVM IR passes, which run on the program stack scheduling
//! emits, and its lowering to bytecode through the assembler.

use crate::{
    backend::{
        assembler,
        evm::op::{self, StackOp},
    },
    link::{EmbeddedBytecodes, LibraryTable, RelocatableBytecode},
    target::Target,
};
use alloy_primitives::U256;
use solar_sema::Gcx;

pub use solar_evmir::ir::*;

pub(in crate::backend) mod builder;
mod passes;
pub(in crate::backend::evm) use passes::SharedSchedules;
pub(crate) use passes::compact_pushes;

pub(crate) use passes::compact_pushes::immediate_materialization_cost;
pub use passes::{
    ALL_PASSES, EvmPass, lookup_pass, pipeline_label, run_passes, run_passes_no_validate,
    run_pipeline,
};
pub(in crate::backend) use passes::{
    compact_pushes::{
        ImmediateMaterialization, ImmediateMaterializationOp, immediate_materialization_len,
    },
    data::pack_linked_data,
    legalize_revert,
};

/// The cost model of a module's code.
pub(crate) trait ModuleTarget {
    /// The cost model of this module's code: the creation model for creation code, which the
    /// runtime artifact follows.
    fn target(&self, gcx: Gcx<'_>) -> Target;
}

impl ModuleTarget for Module {
    fn target(&self, gcx: Gcx<'_>) -> Target {
        Target::new(gcx).priced_for(self.code_follows)
    }
}

/// The cost model of a block's code.
pub(crate) trait BlockTarget {
    /// The cost model of this block: the creation model for creation code, which the runtime
    /// artifact follows (`code_follows`), unless the block may run more than once.
    fn target(&self, gcx: Gcx<'_>, code_follows: bool) -> Target;
}

impl BlockTarget for Block {
    fn target(&self, gcx: Gcx<'_>, code_follows: bool) -> Target {
        Target::new(gcx).priced_for(code_follows && self.runs_once())
    }
}

/// Block-level metadata.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct BlockMetadata {
    /// Estimated block hotness for layout decisions.
    pub(crate) hotness: Hotness,
    /// Whether the block contains code from a known natural loop.
    pub(crate) in_loop: bool,
    /// Whether the block may run more than once per deployment although it is not in a loop of
    /// its own function: it belongs to creation code that a loop or recursion calls. Pricing
    /// treats such blocks as runtime code.
    pub(crate) repeats: bool,
    /// This label's address is used only for control transfer, never as an observable word.
    /// Hidden return targets and branches separated from their pushed labels by sharing carry
    /// this property. References may be redirected to an
    /// equivalent continuation, unlike ordinary address-taken labels whose identity is opaque.
    /// This is an executable-code property, independent of optional debug information.
    pub(crate) is_continuation: bool,
    /// Source function entered by this block's leading `JUMPDEST`.
    pub(crate) function_invoke: Option<DebugFunction>,
    /// Stack depth at which verification enters this block, in addition to its modeled edges.
    ///
    /// Only textual EVM IR declares it, so that a test can start a block with a deep stack; the
    /// entry block is otherwise entered at depth zero.
    pub(crate) entry_depth: Option<u16>,
    /// How the code from this block's `JUMPDEST` returns to its caller, when it is the entry of a
    /// function or of a shared body that returns through a dynamic `jump`.
    ///
    /// Direct jumps to the block are calls, and the return address is a block's address that the
    /// caller pushed before them. See [`CallEffect`].
    pub(crate) call_effect: Option<CallEffect>,
}

/// How code returns to its caller through a dynamic `jump`.
///
/// The code takes the word `ret` positions below the top of the stack on entry, counting the top
/// as zero, as its return address, and the jump to that word leaves the stack with `inputs` words
/// fewer and `outputs` words more than on entry. `inputs` counts the return address and any
/// arguments, and the words the code leaves in their place are its results. This is a property
/// of the code, not of its caller: whichever block's address a caller leaves in that slot is the
/// block that runs next, at the entry depth less `inputs` plus `outputs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CallEffect {
    /// Position of the return address from the top of the stack on entry.
    pub(crate) ret: u8,
    /// The words consumed on entry and the words left on return.
    pub(crate) effect: StackEffect,
}

impl CallEffect {
    /// Returns the effect of the code that follows `instructions` and then an operation with the
    /// effect `last`, which run first and leave the return address on the stack, or `None` when
    /// they consume or copy it.
    pub(crate) fn after(self, instructions: &[Instruction], last: StackEffect) -> Option<Self> {
        let mut ret = usize::from(self.ret);
        // The words the code pushes, less the words it consumes.
        let mut growth = 0isize;
        let swapped = |slot: usize, a: usize, b: usize| match slot {
            slot if slot == a => b,
            slot if slot == b => a,
            slot => slot,
        };
        let apply = |effect: StackEffect, ret: &mut usize, growth: &mut isize| {
            *ret = ret.checked_sub(usize::from(effect.inputs))? + usize::from(effect.outputs);
            *growth += isize::from(effect.outputs) - isize::from(effect.inputs);
            Some(())
        };
        for inst in instructions {
            match inst.as_stack_op() {
                Some(StackOp::Dup(n)) => {
                    if ret + 1 == usize::from(n) {
                        return None;
                    }
                    ret += 1;
                    growth += 1;
                }
                Some(StackOp::Swap(n)) => ret = swapped(ret, 0, usize::from(n)),
                Some(StackOp::Exchange(n, m)) => ret = swapped(ret, usize::from(n), usize::from(m)),
                _ => apply(inst.stack_effect(), &mut ret, &mut growth)?,
            }
        }
        apply(last, &mut ret, &mut growth)?;
        // The code after entered `growth` words higher, so it returns that many words lower.
        let StackEffect { inputs, outputs } = self.effect;
        let (inputs, outputs) = if growth >= 0 {
            (u8::try_from(isize::from(inputs) + growth).ok()?, outputs)
        } else {
            (inputs, u8::try_from(isize::from(outputs) - growth).ok()?)
        };
        Some(Self { ret: u8::try_from(ret).ok()?, effect: StackEffect::new(inputs, outputs) })
    }
}

/// Validates the target-independent invariants of an EVM IR module.
pub fn validate(gcx: Gcx<'_>, module: &Module) {
    verifier(gcx).verify_module(module);
}

/// Returns a verifier for the session's target that reports to its diagnostics.
pub(in crate::backend) fn verifier(gcx: Gcx<'_>) -> verify::Verifier<'_> {
    verify::Verifier::for_evm_version(gcx.dcx(), gcx.sess.opts.evm_version)
}

/// Returns whether data references in `module` can observe entry boundaries or order.
pub(in crate::backend) fn data_layout_is_observable(module: &Module) -> bool {
    passes::data::data_layout_is_observable(module)
}

/// Assembles a program, whose EVM IR pipeline already ran, as bytecode that retains its unresolved
/// library addresses.
pub fn into_bytecode(gcx: Gcx<'_>, module: Module) -> solar_interface::Result<RelocatableBytecode> {
    let assembled = assemble_legalized(gcx, module)?;
    Ok(RelocatableBytecode {
        libraries: assembled.evm_ir.expect("EVM IR capture requested").libraries,
        bytes: assembled.bytecode.into(),
        relocations: assembled.library_relocations,
    })
}

/// Creation and runtime bytecode assembled from one EVM IR program.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeployableBytecode {
    /// Creation bytecode, which returns the runtime bytecode.
    pub deployment: Vec<u8>,
    /// Runtime bytecode.
    pub runtime: Vec<u8>,
}

/// Assembles a program, whose EVM IR pipeline already ran, as runtime bytecode, together with
/// creation bytecode that deploys it unchanged.
///
/// Nothing links library addresses into the result, and the creation code sets no immutables, so
/// a program that pushes either is an error. Bytecode past the target's size limits gets the same
/// warnings as a Solidity contract's.
pub fn into_deployable_bytecode(
    gcx: Gcx<'_>,
    module: Module,
) -> solar_interface::Result<DeployableBytecode> {
    let name = module.name;
    let runtime = deployable(gcx, assemble_legalized(gcx, module)?)?;
    let mut deployment = Module::new(name);
    deployment.code_follows = true;
    let runtime_data = deployment.data.push(Data::new(runtime.clone().into(), None));
    let mut block = Block::new(0);
    // push len(runtime)
    // dup 1
    // push_data runtime
    // push 0
    // codecopy
    // push 0
    // return
    block.instructions = vec![
        Instruction::push_value(U256::from(runtime.len())),
        Instruction::stack_op(StackOp::Dup(1)),
        Instruction::push_data(DataRef::new(runtime_data, 0)),
        Instruction::push_value(U256::ZERO),
        Instruction::opcode(op::CODECOPY),
        Instruction::push_value(U256::ZERO),
    ];
    block.terminator = Some(Terminator::new(TerminatorKind::Op(op::RETURN)));
    deployment.add_block(block);
    let deployment = deployable(gcx, assemble_legalized(gcx, deployment)?)?;
    crate::contract::warn_code_size_limits(gcx, None, deployment.len(), runtime.len());
    Ok(DeployableBytecode { deployment, runtime })
}

/// Assembles `module` without running the EVM IR pipeline, only legalizing it for the target.
fn assemble_legalized(
    gcx: Gcx<'_>,
    module: Module,
) -> solar_interface::Result<assembler::AssembledCode> {
    let mut assembler = assembler::Assembler::from_evm_ir(gcx, module)?;
    assembler.legalize();
    let assembled = assembler.assemble_linked(
        &EmbeddedBytecodes::default(),
        &mut LibraryTable::default(),
        true,
        false,
    );
    gcx.dcx().has_errors()?;
    Ok(assembled)
}

/// Returns the bytecode of an assembled program that deploys as it is. Fails when it pushes a
/// library address, which nothing links, or an immutable, which nothing sets.
fn deployable(
    gcx: Gcx<'_>,
    assembled: assembler::AssembledCode,
) -> solar_interface::Result<Vec<u8>> {
    if !assembled.library_relocations.is_empty() {
        let message = "EVM IR input that pushes library addresses has no bytecode to emit";
        return Err(gcx.dcx().err(message).emit());
    }
    if !assembled.immutable_refs.is_empty() {
        let message = "EVM IR input that pushes immutables has no bytecode to emit";
        let note = "its creation code only copies the runtime, so every immutable would read zero";
        return Err(gcx.dcx().err(message).note(note).emit());
    }
    Ok(assembled.bytecode)
}
