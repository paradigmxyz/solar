//! Isolates native compiler storage below contract-visible EVM memory.
//!
//! Contract pointers remain logical offsets. Only operands that access memory
//! gain the aligned native-prefix size; calldata, code, storage, and pointer
//! arithmetic keep their original values. Saturating address addition prevents
//! an overflowing offset from wrapping into the private prefix. Such addresses
//! still exhaust gas for nonempty accesses, while zero-length operations retain
//! EVM behavior. MSIZE subtracts the prefix and clamps private-only expansion to
//! zero. Backends must also translate their generated ABI and deployment accesses.
//!
//! Native allocation must stay within the measured prefix. Each backend repeats
//! planning when translation increases its spill requirement, and only emits an
//! artifact once that requirement fits. Unbounded native dynamic frames cannot
//! use this fixed-prefix layout.

use crate::mir::{FunctionBuilder, InstKind, Module, Terminator, Value, ValueId};
use alloy_primitives::U256;

pub(super) fn translate(module: &Module, base: u64) -> Module {
    let mut module = module.clone();
    if base == 0 {
        return module;
    }
    for f in &mut module.functions {
        for block in f.blocks.indices() {
            let instructions = std::mem::take(&mut f.blocks[block].instructions);
            let mut builder = FunctionBuilder::new(f);
            builder.switch_to_block(block);
            for id in instructions {
                let mut kind = builder.func().inst(id).kind.clone();
                match &mut kind {
                    InstKind::MLoad(a)
                    | InstKind::MStore(a, _)
                    | InstKind::MStore8(a, _)
                    | InstKind::CalldataCopy(a, _, _)
                    | InstKind::CodeCopy(a, _, _)
                    | InstKind::ReturnDataCopy(a, _, _)
                    | InstKind::DataCopy(_, a, _)
                    | InstKind::ExtCodeCopy(_, a, _, _)
                    | InstKind::Keccak256(a, _)
                    | InstKind::Log0(a, _)
                    | InstKind::Log1(a, _, _)
                    | InstKind::Log2(a, _, _, _)
                    | InstKind::Log3(a, _, _, _, _)
                    | InstKind::Log4(a, _, _, _, _, _)
                    | InstKind::Create(_, a, _)
                    | InstKind::Create2(_, a, _, _) => {
                        // memory[offset] -> physical_memory[saturating_add(offset, base)]
                        *a = address(&mut builder, *a, base);
                    }
                    InstKind::MCopy(a, b, _)
                    | InstKind::Call { args_offset: a, ret_offset: b, .. }
                    | InstKind::CallCode { args_offset: a, ret_offset: b, .. }
                    | InstKind::StaticCall { args_offset: a, ret_offset: b, .. }
                    | InstKind::DelegateCall { args_offset: a, ret_offset: b, .. } => {
                        // copy/call physical(base + input), physical(base + output)
                        *a = address(&mut builder, *a, base);
                        *b = address(&mut builder, *b, base);
                    }
                    InstKind::MSize => {
                        // physical = msize; result = physical > base ? physical - base : 0
                        let physical = builder.msize();
                        let base = builder.imm(U256::from(base));
                        let zero = builder.imm(U256::ZERO);
                        let outside = builder.gt(physical, base);
                        let size = builder.sub(physical, base);
                        kind = InstKind::Select(outside, size, zero);
                    }
                    _ => {}
                }
                builder.func_mut().inst_mut(id).kind = kind;
                builder.func_mut().blocks[block].instructions.push(id);
            }
            let mut terminator = builder.func_mut().blocks[block].terminator.take();
            if let Some(Terminator::ReturnData { offset, .. } | Terminator::Revert { offset, .. }) =
                &mut terminator
            {
                // return/revert physical(base + offset), size
                *offset = address(&mut builder, *offset, base);
            }
            builder.func_mut().blocks[block].terminator = terminator;
        }
    }
    module
}

fn address(builder: &mut FunctionBuilder<'_>, offset: ValueId, base: u64) -> ValueId {
    if let Value::Immediate(value) = builder.func().value(offset)
        && let Some(value) = value.as_u256()
    {
        return builder.imm(value.saturating_add(U256::from(base)));
    }
    // sum = offset + base; overflow = offset > MAX - base; result = overflow ? MAX : sum
    let limit = builder.imm(U256::MAX - U256::from(base));
    let max = builder.imm(U256::MAX);
    let base = builder.imm(U256::from(base));
    let overflow = builder.gt(offset, limit);
    let sum = builder.add(offset, base);
    builder.select(overflow, max, sum)
}

#[cfg(any(feature = "codegen-sonatina", feature = "codegen-llvm"))]
pub(super) fn observes_size(module: &Module) -> bool {
    module
        .functions
        .iter()
        .any(|f| f.instructions().any(|id| matches!(f.inst(id).kind, InstKind::MSize)))
}
