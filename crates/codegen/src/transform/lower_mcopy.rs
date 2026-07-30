//! Lower `mcopy` for EVM versions that predate Cancun.

use crate::{
    analysis::CallGraphInfo,
    mir::{
        BlockId, EffectKind, Function, FunctionBuilder, FunctionId, InstKind, MirType, Module,
        ValueId,
    },
    pass::MirPass,
    transform::utils::redirect_successor_predecessors,
};
use solar_config::OptimizationMode;
use solar_interface::{Ident, sym};
use solar_sema::Gcx;

// Smallest measured site count where outlining improved every benchmarked gas path.
const MIN_GAS_SHARED_SITES: usize = 21;

/// Lowers `mcopy` to an overlap-safe memory loop when the target has no
/// `MCOPY` opcode.
pub(crate) struct LowerMCopy;

impl MirPass for LowerMCopy {
    fn name(&self) -> &'static str {
        "lower-mcopy"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        gcx: Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        if gcx.sess.opts.evm_version.has_mcopy() {
            return false;
        }

        let functions = module.functions.indices().collect::<Vec<_>>();
        let sites =
            functions.iter().map(|&func| count_mcopies(&module.functions[func])).sum::<usize>();
        if sites == 0 {
            return false;
        }
        // Outlining changes the memory high-water mark, which is observable
        // through `msize`.
        if sites == 1
            || functions.iter().any(|&func| {
                module.functions[func]
                    .instructions()
                    .any(|inst| matches!(module.functions[func].inst(inst).kind, InstKind::MSize))
            })
        {
            return lower_functions(module, &functions);
        }

        let optimization = gcx.sess.opts.optimization;
        if !should_share_mcopies(optimization, sites) {
            return lower_functions(module, &functions);
        }

        let call_graph = CallGraphInfo::new(module);
        let constructors = functions
            .iter()
            .copied()
            .filter(|&func| module.functions[func].attributes.is_constructor)
            .collect::<Vec<_>>();
        let mut constructor_reachable =
            call_graph.reachable_callees_from(constructors.iter().copied());
        for constructor in constructors {
            constructor_reachable.insert(constructor);
        }
        // Constructor calls use dynamic frames at the free-memory pointer,
        // which compiler-generated copies may also use as unreserved scratch
        // space. Keep those copies inline so they cannot overwrite their frame.
        let shared_sites = functions
            .iter()
            .filter(|&&func| !constructor_reachable.contains(func))
            .map(|&func| count_mcopies(&module.functions[func]))
            .sum::<usize>();
        // Two sites already save bytecode, but their per-call gas results are
        // mixed. Keep gas-oriented outlining to the measured threshold.
        if !should_share_mcopies(optimization, shared_sites) {
            return lower_functions(module, &functions);
        }

        let helper = mcopy_helper(module);
        for func in functions {
            if constructor_reachable.contains(func) {
                lower_function(&mut module.functions[func]);
            } else {
                replace_mcopies(&mut module.functions[func], helper);
            }
        }
        lower_function(&mut module.functions[helper]);
        true
    }
}

fn lower_functions(module: &mut Module, functions: &[FunctionId]) -> bool {
    let mut changed = false;
    for &func in functions {
        changed |= lower_function(&mut module.functions[func]);
    }
    changed
}

fn should_share_mcopies(optimization: OptimizationMode, sites: usize) -> bool {
    if optimization.is_size() {
        sites >= 2
    } else {
        optimization.is_gas() && sites >= MIN_GAS_SHARED_SITES
    }
}

fn count_mcopies(func: &Function) -> usize {
    func.instructions()
        .filter(|&inst| matches!(func.inst(inst).kind, InstKind::MCopy(_, _, _)))
        .count()
}

fn replace_mcopies(func: &mut Function, helper: FunctionId) {
    let instructions = func.instructions().collect::<Vec<_>>();
    for inst in instructions {
        if let InstKind::MCopy(dest, src, len) = func.inst(inst).kind {
            let instruction = func.inst_mut(inst);
            instruction.kind = InstKind::InternalCall {
                function: helper,
                args: vec![dest, src, len].into_boxed_slice(),
                returns: 0,
            };
            instruction.metadata.set_effect(Some(EffectKind::InternalCall));
            instruction.metadata.set_memory_region(None);
        }
    }
}

fn mcopy_helper(module: &mut Module) -> FunctionId {
    let mut func = Function::new(Ident::with_dummy_span(sym::__mcopy));
    func.attributes.no_inline = true;
    {
        let mut builder = FunctionBuilder::new(&mut func);
        let dest = builder.add_param(MirType::MemPtr);
        let src = builder.add_param(MirType::MemPtr);
        let len = builder.add_param(MirType::uint256());
        builder.mcopy(dest, src, len);
        builder.ret([]);
    }
    module.add_function(func)
}

fn lower_function(func: &mut Function) -> bool {
    let mut changed = false;
    let mut block_index = 0;
    while block_index < func.blocks.len() {
        let block = BlockId::from_usize(block_index);
        let mcopy = func.blocks[block]
            .instructions
            .iter()
            .copied()
            .enumerate()
            .find(|(_, inst)| matches!(func.inst(*inst).kind, InstKind::MCopy(_, _, _)));
        if let Some((position, inst)) = mcopy {
            lower_mcopy(func, block, position, inst);
            changed = true;
        }
        block_index += 1;
    }
    changed
}

fn lower_mcopy(func: &mut Function, block: BlockId, position: usize, inst: crate::mir::InstId) {
    let InstKind::MCopy(dest, src, len) = func.inst(inst).kind else { unreachable!() };

    let mut instructions = std::mem::take(&mut func.blocks[block].instructions);
    let tail = instructions.split_off(position + 1);
    let removed = instructions.pop();
    debug_assert_eq!(removed, Some(inst));
    func.blocks[block].instructions = instructions;
    let old_terminator = func.blocks[block].terminator.take();

    let continuation = func.alloc_block();
    func.blocks[continuation].instructions = tail;
    func.blocks[continuation].terminator = old_terminator;
    redirect_successor_predecessors(func, block, continuation);

    let forward_word_cond = func.alloc_block();
    let forward_word_body = func.alloc_block();
    let forward_byte_cond = func.alloc_block();
    let forward_byte_body = func.alloc_block();
    let backward_word_cond = func.alloc_block();
    let backward_word_body = func.alloc_block();
    let backward_byte_cond = func.alloc_block();
    let backward_byte_body = func.alloc_block();

    let mut builder = FunctionBuilder::new(func);
    builder.switch_to_block(block);
    let zero = builder.imm_u64(0);
    let src_end = builder.add(src, len);
    let dest_after_src = builder.gt(dest, src);
    let dest_before_end = builder.lt(dest, src_end);
    let overlaps_after = builder.and(dest_after_src, dest_before_end);
    builder.branch(overlaps_after, backward_word_cond, forward_word_cond);

    builder.switch_to_block(forward_word_cond);
    let forward_offset = builder.phi(vec![(block, zero)]);
    let forward_remaining = builder.sub(len, forward_offset);
    let word_size = builder.imm_u64(32);
    let byte_mask = builder.imm_u64(31);
    let forward_short = builder.lt(forward_remaining, word_size);
    builder.branch(forward_short, forward_byte_cond, forward_word_body);

    builder.switch_to_block(forward_word_body);
    let forward_src = builder.add(src, forward_offset);
    let word = builder.mload(forward_src);
    let forward_dest = builder.add(dest, forward_offset);
    builder.mstore(forward_dest, word);
    let next_forward_offset = builder.add(forward_offset, word_size);
    let forward_word_latch = builder.current_block();
    builder.jump(forward_word_cond);
    builder.add_phi_incoming(forward_offset, forward_word_latch, next_forward_offset);

    builder.switch_to_block(forward_byte_cond);
    let forward_byte_offset = builder.phi(vec![(forward_word_cond, forward_offset)]);
    let has_forward_byte = builder.lt(forward_byte_offset, len);
    builder.branch(has_forward_byte, forward_byte_body, continuation);

    builder.switch_to_block(forward_byte_body);
    let forward_src = builder.add(src, forward_byte_offset);
    let byte = load_byte(&mut builder, forward_src, byte_mask);
    let forward_dest = builder.add(dest, forward_byte_offset);
    builder.mstore8(forward_dest, byte);
    let one = builder.imm_u64(1);
    let next_forward_byte = builder.add(forward_byte_offset, one);
    let forward_byte_latch = builder.current_block();
    builder.jump(forward_byte_cond);
    builder.add_phi_incoming(forward_byte_offset, forward_byte_latch, next_forward_byte);

    builder.switch_to_block(backward_word_cond);
    let backward_remaining = builder.phi(vec![(block, len)]);
    let backward_short = builder.lt(backward_remaining, word_size);
    builder.branch(backward_short, backward_byte_cond, backward_word_body);

    builder.switch_to_block(backward_word_body);
    let next_backward_remaining = builder.sub(backward_remaining, word_size);
    let backward_src = builder.add(src, next_backward_remaining);
    let word = builder.mload(backward_src);
    let backward_dest = builder.add(dest, next_backward_remaining);
    builder.mstore(backward_dest, word);
    let backward_word_latch = builder.current_block();
    builder.jump(backward_word_cond);
    builder.add_phi_incoming(backward_remaining, backward_word_latch, next_backward_remaining);

    builder.switch_to_block(backward_byte_cond);
    let backward_byte_remaining = builder.phi(vec![(backward_word_cond, backward_remaining)]);
    let has_backward_byte = builder.gt(backward_byte_remaining, zero);
    builder.branch(has_backward_byte, backward_byte_body, continuation);

    builder.switch_to_block(backward_byte_body);
    let one = builder.imm_u64(1);
    let next_backward_byte = builder.sub(backward_byte_remaining, one);
    let backward_src = builder.add(src, next_backward_byte);
    let byte = load_byte(&mut builder, backward_src, byte_mask);
    let backward_dest = builder.add(dest, next_backward_byte);
    builder.mstore8(backward_dest, byte);
    let backward_byte_latch = builder.current_block();
    builder.jump(backward_byte_cond);
    builder.add_phi_incoming(backward_byte_remaining, backward_byte_latch, next_backward_byte);
}

fn load_byte(builder: &mut FunctionBuilder<'_>, address: ValueId, byte_mask: ValueId) -> ValueId {
    // Avoid expanding memory past the source byte's containing word.
    let byte_index = builder.and(address, byte_mask);
    let aligned_address = builder.sub(address, byte_index);
    let word = builder.mload(aligned_address);
    builder.byte(byte_index, word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcopy_sharing_policy() {
        assert!(!should_share_mcopies(OptimizationMode::None, usize::MAX));
        assert!(!should_share_mcopies(OptimizationMode::Size, 1));
        assert!(should_share_mcopies(OptimizationMode::Size, 2));
        assert!(!should_share_mcopies(OptimizationMode::Gas, MIN_GAS_SHARED_SITES - 1));
        assert!(should_share_mcopies(OptimizationMode::Gas, MIN_GAS_SHARED_SITES));
    }
}
