//! Constructor argument copying, completion, and immutable runtime patching.
//!
//! Constructor MIR and runtime MIR are scheduled independently. Appended constructor
//! arguments are copied above compiler-owned storage before constructor execution;
//! their bytecode source offset is a whole-program relocation, so it participates
//! in the assembler's fixed point. Constructor STOP transfers to one postlude.
//!
//! The postlude copies the encoded runtime above immutable staging when patching is
//! needed. Every patch modifies exactly its declared immediate bytes, preserving the
//! following runtime bytes through a masked word write. With no immutables the
//! completed constructor's memory can be reused from offset zero. No constructor
//! values or frame contents are observed after the final RETURN.

use super::{ImmutableReference, ir, machine, op, storage::ModulePlan};
use crate::{immutable::immutable_staging_addr, mir};
use alloy_primitives::U256;
use solar_config::{EvmVersion, OptimizationMode};

/// Relocation reserved within generated deployment modules, never parsed inputs.
pub(crate) const PROGRAM_SIZE_ID: u32 = 0x0fff_ffff;
const RUNTIME_START_ID: u32 = PROGRAM_SIZE_ID - 1;

pub(crate) fn lower(
    module: &mir::Module,
    runtime: Vec<u8>,
    references: &[ImmutableReference],
    version: EvmVersion,
    optimization: OptimizationMode,
    switches: &mut super::switches::Planner,
) -> Result<ir::Module, String> {
    let constructor = module
        .iter_functions()
        .find(|(_, function)| function.attributes.is_constructor)
        .map(|(id, _)| id);
    let (mut output, plan) = if let Some(constructor) = constructor {
        let generated = machine::lower(module, constructor, version, optimization, switches)?;
        (generated.ir, generated.plan)
    } else {
        let mut output = ir::Module {
            name: module.name.name,
            private_control_labels: true,
            ..Default::default()
        };
        // callvalue; jumpi <revert>, <deployment completion>
        output.blocks.push(ir::Block {
            insts: vec![ir::InstKind::Op(op::CALLVALUE).into()],
            ..Default::default()
        });
        output.blocks.push(ir::Block {
            insts: vec![
                ir::InstKind::Push(U256::ZERO).into(),
                ir::InstKind::Push(U256::ZERO).into(),
            ],
            terminator: ir::TerminatorKind::Revert.into(),
            ..Default::default()
        });
        (output, ModulePlan::new(module, true)?)
    };
    output.debug_info_tracked = module.debug_info_is_tracked();
    output.name = solar_interface::Symbol::intern(&format!("{}_deployment", module.name));
    if let Some(constructor) = constructor {
        let function = module.function(constructor);
        let needs_args = !function.params.is_empty()
            || function.instructions().any(|id| {
                matches!(
                    function.inst(id).kind,
                    mir::InstKind::ConstructorArgsBase | mir::InstKind::ConstructorArgsEnd
                )
            });
        if needs_args {
            output.program_size_id = Some(PROGRAM_SIZE_ID);
            let prologue = output.block_ids().next().ok_or("constructor has no physical entry")?;
            let insts = &mut output.blocks[prologue].insts;
            // push <program end>; codesize; sub
            // dup1; push <program end>; push <argument base>; codecopy
            // push <argument base + 31>; add; push ~31; and; push 0x40; mstore
            insts.extend([
                ir::InstKind::PushDeferred(PROGRAM_SIZE_ID).into(),
                ir::InstKind::Op(op::CODESIZE).into(),
                ir::InstKind::Op(op::SUB).into(),
                ir::InstKind::Dup(1).into(),
                ir::InstKind::PushDeferred(PROGRAM_SIZE_ID).into(),
                ir::InstKind::Push(U256::from(plan.constructor_arg_base)).into(),
                ir::InstKind::Op(op::CODECOPY).into(),
                ir::InstKind::Push(U256::from(plan.constructor_arg_base) + U256::from(31)).into(),
                ir::InstKind::Op(op::ADD).into(),
                ir::InstKind::Push(!U256::from(31)).into(),
                ir::InstKind::Op(op::AND).into(),
                ir::InstKind::Push(U256::from(0x40)).into(),
                ir::InstKind::Op(op::MSTORE).into(),
            ]);
        }
    }
    let size = U256::from(runtime.len());
    output.appendix = runtime;
    output.appendix_start_id = Some(RUNTIME_START_ID);
    let end = output.blocks.next_idx();
    let ids: Vec<_> = output.block_ids().collect();
    for id in ids {
        let block = &mut output.blocks[id];
        if matches!(block.terminator.kind, ir::TerminatorKind::Stop) {
            // jump <deployment postlude>
            block.terminator = ir::TerminatorKind::Jump(end).into();
        }
    }
    if constructor.is_none() {
        // callvalue; jumpi <revert>, <deployment postlude>
        output.blocks[ir::BlockId::new(0)].terminator =
            ir::TerminatorKind::JumpI(ir::BlockId::new(1), end).into();
    }
    let buffer = if references.is_empty() { 0 } else { plan.immutable_staging_end };
    // The default deployment postlude has an empty incoming stack. Keeping a
    // nonzero size costs one DUP instead of repeating its multi-byte PUSH.
    let keep_size = constructor.is_none() && !size.is_zero();
    // push <runtime size>; [dup1]; push_data <runtime>; push <buffer>; codecopy
    let mut insts = vec![ir::InstKind::Push(size).into()];
    if keep_size {
        // dup1
        insts.push(ir::InstKind::Dup(1).into());
    }
    // push_data <runtime>; push <buffer>; codecopy
    insts.extend([
        ir::InstKind::PushDeferred(RUNTIME_START_ID).into(),
        ir::InstKind::Push(U256::from(buffer)).into(),
        ir::InstKind::Op(op::CODECOPY).into(),
    ]);
    for reference in references {
        let width = reference.type_size.bytes();
        let address = U256::from(buffer) + U256::from(reference.code_offset + 1);
        let staging = immutable_staging_addr(plan.immutable_staging_base, reference.id);
        // push <immutable staging address>; mload
        insts.push(ir::InstKind::Push(U256::from(staging)).into());
        insts.push(ir::InstKind::Op(op::MLOAD).into());
        if width == 1 {
            if matches!(
                module.immutable_type(reference.id).immutable_encoding(),
                Some(mir::ImmutableEncoding::LeftAligned(_))
            ) {
                // push 0; byte
                insts.push(ir::InstKind::Push(U256::ZERO).into());
                insts.push(ir::InstKind::Op(op::BYTE).into());
            }
            // push <patch address>; mstore8
            insts.push(ir::InstKind::Push(address).into());
            insts.push(ir::InstKind::Op(op::MSTORE8).into());
            continue;
        }
        if width < 32 {
            let shift = (32 - usize::from(width)) * 8;
            if !matches!(
                module.immutable_type(reference.id).immutable_encoding(),
                Some(mir::ImmutableEncoding::LeftAligned(_))
            ) {
                // push <left alignment shift>; shl
                insts.push(ir::InstKind::Push(U256::from(shift)).into());
                insts.push(ir::InstKind::Op(op::SHL).into());
            }
            // push <patch address>; mload; push <suffix mask>; and; or
            insts.extend([
                ir::InstKind::Push(address).into(),
                ir::InstKind::Op(op::MLOAD).into(),
                ir::InstKind::Push((U256::ONE << shift) - U256::ONE).into(),
                ir::InstKind::Op(op::AND).into(),
                ir::InstKind::Op(op::OR).into(),
            ]);
        }
        // push <patch address>; mstore
        insts.push(ir::InstKind::Push(address).into());
        insts.push(ir::InstKind::Op(op::MSTORE).into());
    }
    if !keep_size {
        // push <runtime size>
        insts.push(ir::InstKind::Push(size).into());
    }
    // push <buffer>; return
    insts.push(ir::InstKind::Push(U256::from(buffer)).into());
    output.blocks.push(ir::Block {
        insts,
        terminator: ir::TerminatorKind::Return.into(),
        ..Default::default()
    });
    if let Some(layout) = &mut output.layout {
        layout.push(end);
    }
    Ok(output)
}
