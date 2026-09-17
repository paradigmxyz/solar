//! Lower consecutive checked `uint256` additions with one overflow check.
//!
//! Scan each block for adjacent checked additions where each addition uses the previous result.
//! Emit the sums in order, OR their individual carry predicates, and panic once at the end of the
//! chain. Keep every carry: wrapping more than once can make the final result appear valid.
//! Any other instruction, arithmetic type, or block boundary ends the chain. Intermediate sums
//! may have other uses; replacing their SSA values preserves those uses.
//!
//! Run immediately before arithmetic lowering, after gas-mode revert outlining, so the shared
//! check stays local like ordinary arithmetic checks. The target prices the removed branches
//! against the added OR and accumulator traffic. Enable this by default only in gas mode because
//! predicate lifetimes can increase bytecode size. Each addition retains its source context;
//! the shared check intentionally has no unique source location.

use crate::{
    backend::evm::op,
    mir::{
        ArithmeticKind, CheckedOp, Function, FunctionBuilder, InstId, InstKind,
        InstructionMetadata, Module, PanicCode,
        pass::{MirPass, run_function_pass},
        transform::utils::redirect_successor_predecessors,
    },
    target::Target,
};
use solar_data_structures::map::FxHashMap;

pub(crate) struct CheckedAggregate;

impl MirPass for CheckedAggregate {
    fn name(&self) -> &'static str {
        "checked-aggregate"
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let target = Target::new(gcx);
        let before = target.opcode(op::JUMPI).plus(target.opcode(op::PUSH2));
        let after = target.opcode(op::OR).plus(target.dup());
        if target.cmp(after, before).is_ge() {
            return false;
        }
        run_function_pass(module, analyses, |func, _| {
            let mut replacements = FxHashMap::default();
            for block in func.blocks.indices() {
                if !func.blocks[block]
                    .instructions
                    .windows(2)
                    .any(|pair| chain_len(func, pair) == 2)
                {
                    continue;
                }
                let instructions = std::mem::take(&mut func.blocks[block].instructions);
                let (terminator, metadata) = func.blocks[block].take_terminator();
                let mut builder = FunctionBuilder::new(func);
                builder.switch_to_block(block);
                let mut remaining = instructions.as_slice();
                while !remaining.is_empty() {
                    let count = chain_len(builder.func(), remaining);
                    if count < 2 {
                        // continuation: original instruction
                        let current = builder.current_block();
                        builder.func_mut().blocks[current].instructions.push(remaining[0]);
                        remaining = &remaining[1..];
                        continue;
                    }
                    let mut carry = None;
                    let mut combined = InstructionMetadata::EMPTY;
                    // NOTE: An accumulated overflow cannot be attributed to one addition.
                    combined.mark_debug_info_dropped();
                    for &id in &remaining[..count] {
                        let InstKind::CheckedBinary { lhs, rhs, .. } = builder.func().inst(id).kind
                        else {
                            unreachable!();
                        };
                        let context = builder.func().inst(id).metadata.debug_context();
                        builder.set_debug_context(&context);
                        // sum = add lhs, rhs
                        // overflow = lt sum, lhs
                        let sum = builder.add(lhs, rhs);
                        let overflow = builder.lt(sum, lhs);
                        replacements.insert(builder.func().inst_result_value(id).unwrap(), sum);
                        builder.set_debug_context(&combined);
                        // carry = carry | overflow
                        carry = Some(match carry {
                            Some(previous) => builder.or(previous, overflow),
                            None => overflow,
                        });
                    }
                    // panic_if carry, 0x11
                    builder.panic_if(carry.unwrap(), PanicCode::ArithmeticOverflowUnderflow);
                    remaining = &remaining[count..];
                }
                // continuation: original terminator
                let end = builder.current_block();
                if let Some(terminator) = terminator {
                    builder.func_mut().blocks[end].set_terminator(terminator, metadata);
                }
                redirect_successor_predecessors(builder.func_mut(), block, end);
            }
            if replacements.is_empty() {
                return false;
            }
            // checked sum uses => scalar sum uses
            func.replace_uses_canonicalized(&replacements);
            true
        })
    }
}

fn chain_len(func: &Function, instructions: &[InstId]) -> usize {
    let mut previous = None;
    let mut count = 0;
    for &id in instructions {
        if let InstKind::CheckedBinary {
            op: CheckedOp::Add,
            arithmetic: ArithmeticKind::Unsigned(256),
            lhs,
            rhs,
        } = func.inst(id).kind
            && previous.is_none_or(|sum| lhs == sum || rhs == sum)
        {
            previous = func.inst_result_value(id);
            count += 1;
        } else {
            break;
        }
    }
    count
}
