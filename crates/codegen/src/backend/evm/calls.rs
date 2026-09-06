//! Physical memory protocol for internal function activations.
//!
//! The scheduler supplies a suspended caller prefix, return label, and arguments in reverse order.
//! Static entry setup consumes the arguments into their distinct fixed frame. Dynamic entry first
//! computes an allocation above both fixed memory and the active dynamic frame, then exposes an
//! overflow condition to the machine CFG builder. Only the successful continuation writes memory.
//! Its header saves the previous frame pointer and the original free-memory pointer, including when
//! an inline-assembly write forced allocation to begin above that pointer. A static function can
//! execute inside a dynamic activation; its caller therefore supplies the module's maximum dynamic
//! frame size to protect the unknown ancestor's complete region.
//!
//! Return setup preserves every physical stack word. It restores the previous frame pointer and
//! reclaims the free-memory frontier only when storage planning proves the activation cannot
//! escape. Source-level local initialization stays in MIR; these helpers initialize only
//! protocol-owned header and argument words. Return labels, result arrangement, CFG edges and panic
//! blocks belong to machine lowering rather than this module.

use super::{
    ir::{InstKind, Instruction},
    op,
    storage::{FrameAddress, FrameBase, FunctionStorage, PREVIOUS_FRAME_OFFSET, SAVED_FMP_OFFSET},
};
use crate::memory::EvmMemoryLayout;
use alloy_primitives::U256;
use solar_config::EvmVersion;

/// A guarded activation setup, with no memory writes before the success edge.
pub(crate) struct CallSetup {
    /// For a dynamic frame, leaves saved FMP, frame base, frame end, and invalid flag above args.
    pub(crate) guard: Vec<Instruction>,
    /// Consumes arguments and any guard temporaries, preserving caller prefix and return label.
    pub(crate) setup: Vec<Instruction>,
}

/// Consumes `arg0` first, leaving the scheduler-owned return label below it intact.
pub(crate) fn enter(
    callee: &FunctionStorage,
    caller: &FunctionStorage,
    argument_count: usize,
    allocation_floor: u64,
    max_dynamic_frame_size: u64,
    _version: EvmVersion,
) -> Result<CallSetup, String> {
    let expected = (callee.return_offset - callee.argument_offset) / EvmMemoryLayout::WORD_SIZE;
    if u64::try_from(argument_count).ok() != Some(expected) {
        return Err("internal-call argument count disagrees with its frame layout".into());
    }
    if callee.stack_arguments {
        return Ok(CallSetup { guard: Vec::new(), setup: Vec::new() });
    }
    let mut guard = Vec::new();
    let mut setup = Vec::new();
    if callee.base == FrameBase::Dynamic {
        // saved_fmp = mload(0x40)
        // frame = max(saved_fmp, fixed_memory_end)
        push(&mut guard, EvmMemoryLayout::FMP_SLOT);
        opcode(&mut guard, op::MLOAD);
        dup(&mut guard, 1);
        push(&mut guard, allocation_floor);
        maximum(&mut guard);
        // active_end = mload(0xa0) == 0 ? 0 : mload(0xa0) + active_frame_size
        // frame = max(frame, active_end)
        push(&mut guard, EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT);
        opcode(&mut guard, op::MLOAD);
        dup(&mut guard, 1);
        push(
            &mut guard,
            if caller.base == FrameBase::Dynamic && !caller.stack_arguments {
                caller.frame_size
            } else {
                max_dynamic_frame_size
            },
        );
        opcode(&mut guard, op::ADD);
        swap(&mut guard, 1);
        opcode(&mut guard, op::ISZERO);
        opcode(&mut guard, op::ISZERO);
        opcode(&mut guard, op::MUL);
        maximum(&mut guard);
        // end = frame + frame_size
        // invalid = frame > u64::MAX || end > u64::MAX || previous_frame > u64::MAX
        dup(&mut guard, 1);
        push(&mut guard, callee.frame_size);
        opcode(&mut guard, op::ADD);
        push(&mut guard, EvmMemoryLayout::MAX_ALLOCATION_END);
        dup(&mut guard, 3);
        opcode(&mut guard, op::GT);
        push(&mut guard, EvmMemoryLayout::MAX_ALLOCATION_END);
        dup(&mut guard, 3);
        opcode(&mut guard, op::GT);
        opcode(&mut guard, op::OR);
        push(&mut guard, EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT);
        opcode(&mut guard, op::MLOAD);
        push(&mut guard, EvmMemoryLayout::MAX_ALLOCATION_END);
        swap(&mut guard, 1);
        opcode(&mut guard, op::GT);
        opcode(&mut guard, op::OR);

        // mstore(0x40, end)
        // mstore(frame, mload(0xa0))
        // mstore(frame + 32, saved_fmp)
        // mstore(0xa0, frame)
        dup(&mut setup, 1);
        push(&mut setup, EvmMemoryLayout::FMP_SLOT);
        opcode(&mut setup, op::MSTORE);
        opcode(&mut setup, op::POP);
        push(&mut setup, EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT);
        opcode(&mut setup, op::MLOAD);
        dup(&mut setup, 2);
        opcode(&mut setup, op::MSTORE);
        dup(&mut setup, 2);
        dup(&mut setup, 2);
        push(&mut setup, SAVED_FMP_OFFSET);
        opcode(&mut setup, op::ADD);
        opcode(&mut setup, op::MSTORE);
        swap(&mut setup, 1);
        opcode(&mut setup, op::POP);
        dup(&mut setup, 1);
        push(&mut setup, EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT);
        opcode(&mut setup, op::MSTORE);
    }
    for index in 0..argument_count {
        let offset = (index as u64)
            .checked_mul(EvmMemoryLayout::WORD_SIZE)
            .and_then(|offset| callee.argument_offset.checked_add(offset))
            .ok_or("internal-call argument address overflow")?;
        match callee.base {
            FrameBase::Static(base) => {
                // mstore(frame + argument_offset, arg)
                push(
                    &mut setup,
                    base.checked_add(offset).ok_or("internal-call frame address overflow")?,
                );
                opcode(&mut setup, op::MSTORE);
            }
            FrameBase::Dynamic => {
                // swap 1; dup 2; push argument_offset; add; mstore
                // Keep the new frame above the remaining argument stack.
                swap(&mut setup, 1);
                dup(&mut setup, 2);
                push(&mut setup, offset);
                opcode(&mut setup, op::ADD);
                opcode(&mut setup, op::MSTORE);
            }
        }
    }
    if callee.base == FrameBase::Dynamic {
        // pop frame
        opcode(&mut setup, op::POP);
    }
    Ok(CallSetup { guard, setup })
}

/// Restores the suspended activation without consuming or moving return values.
pub(crate) fn leave(frame: &FunctionStorage) -> Vec<Instruction> {
    let mut output = Vec::new();
    if frame.base == FrameBase::Dynamic && !frame.stack_arguments {
        // frame = mload(0xa0)
        push(&mut output, EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT);
        opcode(&mut output, op::MLOAD);
        if !frame.retain_frame {
            // mstore(0x40, mload(frame + 32))
            dup(&mut output, 1);
            push(&mut output, SAVED_FMP_OFFSET);
            opcode(&mut output, op::ADD);
            opcode(&mut output, op::MLOAD);
            push(&mut output, EvmMemoryLayout::FMP_SLOT);
            opcode(&mut output, op::MSTORE);
        }
        // mstore(0xa0, mload(frame))
        debug_assert_eq!(PREVIOUS_FRAME_OFFSET, 0);
        opcode(&mut output, op::MLOAD);
        push(&mut output, EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT);
        opcode(&mut output, op::MSTORE);
    }
    output
}

/// Materializes a physical address without introducing MIR identities into EVM IR.
pub(crate) fn address(address: FrameAddress) -> Vec<Instruction> {
    let mut output = Vec::new();
    match address {
        FrameAddress::Absolute(value) => {
            // push address
            push(&mut output, value);
        }
        FrameAddress::Relative(offset) => {
            // mload(0xa0) + offset
            push(&mut output, EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT);
            opcode(&mut output, op::MLOAD);
            if offset != 0 {
                push(&mut output, offset);
                opcode(&mut output, op::ADD);
            }
        }
    }
    output
}

/// Replaces two unsigned words by their maximum without conditional control flow.
fn maximum(output: &mut Vec<Instruction>) {
    // a + (b - a) * (b > a)
    dup(output, 2);
    dup(output, 2);
    opcode(output, op::GT);
    swap(output, 1);
    dup(output, 3);
    swap(output, 1);
    opcode(output, op::SUB);
    opcode(output, op::MUL);
    opcode(output, op::ADD);
}

fn push(output: &mut Vec<Instruction>, value: u64) {
    // push value
    output.push(InstKind::Push(U256::from(value)).into());
}

fn opcode(output: &mut Vec<Instruction>, opcode: u8) {
    // opcode
    output.push(InstKind::Op(opcode).into());
}

fn dup(output: &mut Vec<Instruction>, depth: u16) {
    // dup depth
    output.push(InstKind::Dup(depth).into());
}

fn swap(output: &mut Vec<Instruction>, depth: u16) {
    // swap depth
    output.push(InstKind::Swap(depth).into());
}
