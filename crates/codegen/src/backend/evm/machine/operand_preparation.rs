//! Prepares resident operands before absent scalar literals during one bounded block trial.
//!
//! A nonempty scalar-immediate prefix in pop order followed only by present operands permits
//! canonical preparation of the resident suffix before one PUSH per literal occurrence. Missing
//! arguments, homes, recipes and interleaved operands stay canonical. The original leading
//! stack-only prefix remains unchanged through the first non-POP opcode, preserving the boundary
//! used by the established block chooser. Writer protection and preferred successors also retain
//! their existing preparation paths. No MIR effects or memory accesses move.
//!
//! Only the final Gas resident-operand replay invokes this helper. It emits directly without a
//! local clone or cost query; a failure discards the speculative block. The existing chooser pays
//! the complete body and exact exit against its established winner, including all older trials,
//! and checks the leading prefix, required input, net height and relative peak. This protects an
//! opaque suspended caller prefix. Later CFG sharing and layout still need corpus measurements.

use super::{Context, Slot, load_value};
use crate::{
    backend::evm::{
        ir, op,
        scheduler::{Stack, StackError},
    },
    mir,
};

pub(super) fn prepare(
    context: &Context<'_>,
    stack: &mut Stack<Slot>,
    output: &mut Vec<ir::Instruction>,
    values: &[mir::ValueId],
    live: impl Fn(mir::ValueId) -> bool,
) -> Result<(), String> {
    let Some(split) = values
        .iter()
        .position(|&value| stack.values().contains(&Slot::Value(value)))
        .filter(|&split| split != 0)
    else {
        return super::prepare(context, stack, output, values, live);
    };
    if !output.iter().rev().any(|inst| matches!(inst.kind, ir::InstKind::Op(code) if code != op::POP))
        || values[..split].iter().any(|&value| {
            !matches!(context.function.value(value), mir::Value::Immediate(v) if v.as_u256().is_some())
                || context.layout.rematerialized.contains_key(&value)
                || context.layout.spills.homes.contains_key(&value)
        })
        || values[split..].iter().any(|&value| !stack.values().contains(&Slot::Value(value)))
    {
        return super::prepare(context, stack, output, values, live);
    }

    // <canonical retained values>; <reverse resident operand pop order>
    super::prepare(context, stack, output, &values[split..], live)?;
    if stack.values().len() + split > 1024 {
        return Err(super::schedule_error(StackError::Overflow));
    }
    // push A[last]; ...; push A[first]
    for &value in values[..split].iter().rev() {
        load_value(context, value, output)?;
        stack.push(Slot::Value(value));
    }
    Ok(())
}
