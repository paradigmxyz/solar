//! Private stack identities and checked physical stack reconciliation.
//!
//! A stack is stored from bottom to top. Reconciliation first removes excess
//! occurrences, duplicates missing occurrences, then fixes the permutation from
//! bottom to top. The fixed prefix is never moved or removed, though its values
//! may be duplicated. Every emitted operation respects target reach and the
//! 1024-word execution limit. Failed plans leave the input state untouched.
//!
//! This module has no MIR or frame semantics. The caller chooses liveness,
//! materializes missing constants, performs simultaneous phi renaming and supplies
//! any suspended activation prefix. Inaccessible values require caller-owned spills.

use super::{ir, op};
use solar_config::EvmVersion;

/// A failure to produce a legal physical stack schedule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StackError {
    MissingValue,
    PrefixMismatch,
    InaccessibleDepth,
    Overflow,
}

/// The scheduler's bottom-to-top value identities.
#[derive(Clone, Debug)]
pub(crate) struct Stack<T> {
    values: Vec<T>,
}

impl<T: Copy + Eq> Stack<T> {
    pub(crate) fn new(values: Vec<T>) -> Self {
        Self { values }
    }

    pub(crate) fn values(&self) -> &[T] {
        &self.values
    }

    /// Records a value produced by a separately emitted physical instruction.
    pub(crate) fn push(&mut self, value: T) {
        self.values.push(value);
    }

    /// Records operands consumed by a separately emitted physical instruction.
    pub(crate) fn truncate(&mut self, len: usize) {
        self.values.truncate(len);
    }

    /// Applies a simultaneous identity substitution, for example along a phi edge.
    pub(crate) fn rename(&mut self, mut rename: impl FnMut(T) -> T) {
        for value in &mut self.values {
            *value = rename(*value);
        }
    }

    /// Places operands in pop order above a retained live base and fixed prefix.
    pub(crate) fn prepare(
        &mut self,
        pop_order: &[T],
        fixed_prefix: usize,
        evm_version: EvmVersion,
        mut live: impl FnMut(T) -> bool,
    ) -> Result<Vec<ir::Instruction>, StackError> {
        if fixed_prefix > self.values.len() {
            return Err(StackError::PrefixMismatch);
        }
        let mut desired = self.values[..fixed_prefix].to_vec();
        for &value in &self.values[fixed_prefix..] {
            if live(value) && !desired[fixed_prefix..].contains(&value) {
                desired.push(value);
            }
        }
        desired.extend(pop_order.iter().rev().copied());
        self.reconcile(&desired, fixed_prefix, evm_version)
    }

    /// Plans a checked permutation and duplication with no partial state on failure.
    pub(crate) fn reconcile(
        &mut self,
        desired: &[T],
        fixed_prefix: usize,
        evm_version: EvmVersion,
    ) -> Result<Vec<ir::Instruction>, StackError> {
        if self.values.len() > 1024 || desired.len() > 1024 {
            return Err(StackError::Overflow);
        }
        if fixed_prefix > self.values.len()
            || fixed_prefix > desired.len()
            || self.values[..fixed_prefix] != desired[..fixed_prefix]
        {
            return Err(StackError::PrefixMismatch);
        }
        let mut planned = self.clone();
        let mut output = Vec::new();
        planned.plan(desired, fixed_prefix, evm_version, &mut output)?;
        *self = planned;
        Ok(output)
    }

    fn plan(
        &mut self,
        desired: &[T],
        fixed_prefix: usize,
        evm_version: EvmVersion,
        output: &mut Vec<ir::Instruction>,
    ) -> Result<(), StackError> {
        // swap depth
        // pop
        // Remove excess occurrences before duplicating to avoid unnecessary peaks.
        let mut index = self.values.len();
        while index > fixed_prefix {
            index -= 1;
            let value = self.values[index];
            if self.count(value) > desired.iter().filter(|&&other| other == value).count() {
                self.swap(index, evm_version, output)?;
                self.values.pop();
                output.push(ir::InstKind::Op(op::POP).into());
            }
        }
        // dup depth
        // Each missing occurrence is copied from the nearest surviving occurrence.
        for &value in &desired[fixed_prefix..] {
            let required = desired.iter().filter(|&&other| other == value).count();
            while self.count(value) < required {
                let Some(index) = self.values.iter().rposition(|&other| other == value) else {
                    return Err(StackError::MissingValue);
                };
                let depth = self.values.len() - index;
                if depth > evm_version.reachable_stack_depth() {
                    return Err(StackError::InaccessibleDepth);
                }
                if self.values.len() == 1024 {
                    return Err(StackError::Overflow);
                }
                self.values.push(value);
                output.push(ir::InstKind::Dup(depth as u16).into());
            }
        }
        // swap source_depth
        // swap destination_depth
        // Place each final slot through the top without touching the fixed prefix.
        for (index, &value) in desired.iter().enumerate().skip(fixed_prefix) {
            if self.values[index] != value {
                let source = self.values[index + 1..]
                    .iter()
                    .position(|&other| other == value)
                    .ok_or(StackError::MissingValue)?
                    + index
                    + 1;
                self.swap(source, evm_version, output)?;
                self.swap(index, evm_version, output)?;
            }
        }
        debug_assert!(self.values == desired);
        Ok(())
    }

    fn count(&self, value: T) -> usize {
        self.values.iter().filter(|&&other| other == value).count()
    }

    fn swap(
        &mut self,
        index: usize,
        evm_version: EvmVersion,
        output: &mut Vec<ir::Instruction>,
    ) -> Result<(), StackError> {
        let top = self.values.len() - 1;
        let depth = top - index;
        if depth > evm_version.reachable_stack_depth() {
            return Err(StackError::InaccessibleDepth);
        }
        // swap depth
        if depth != 0 {
            self.values.swap(index, top);
            output.push(ir::InstKind::Swap(depth as u16).into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replay(mut values: Vec<u8>, instructions: &[ir::Instruction], prefix: &[u8]) -> Vec<u8> {
        for instruction in instructions {
            match instruction.kind {
                ir::InstKind::Dup(depth) => {
                    values.push(values[values.len() - usize::from(depth)]);
                }
                ir::InstKind::Swap(depth) => {
                    let top = values.len() - 1;
                    values.swap(top, top - usize::from(depth));
                }
                ir::InstKind::Op(op::POP) => {
                    values.pop().unwrap();
                }
                _ => panic!("unexpected non-stack instruction"),
            }
            assert!(values.len() <= 1024);
            assert_eq!(&values[..prefix.len()], prefix);
        }
        values
    }

    #[test]
    fn randomized_permutations_duplicates_and_prefixes() {
        let mut seed = 0x1234_5678_u32;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as usize
        };
        for _ in 0..4000 {
            let len = next() % 12 + 1;
            let initial = (0..len).map(|_| (next() % 5) as u8).collect::<Vec<_>>();
            let prefix = next() % (len + 1);
            let mut desired = initial[..prefix].to_vec();
            for _ in 0..next() % (17 - prefix) {
                desired.push(initial[next() % len]);
            }
            let mut stack = Stack::new(initial.clone());
            let code = stack.reconcile(&desired, prefix, EvmVersion::Osaka).unwrap();
            assert_eq!(replay(initial.clone(), &code, &initial[..prefix]), desired);
            assert_eq!(stack.values(), desired);
        }
    }

    #[test]
    fn exact_target_reach_and_transient_limits() {
        for (version, depth) in [(EvmVersion::Osaka, 16), (EvmVersion::Amsterdam, 235)] {
            let initial = (0..depth).collect::<Vec<_>>();
            let mut desired = initial.clone();
            desired.push(0);
            let mut stack = Stack::new(initial.clone());
            let code = stack.reconcile(&desired, 0, version).unwrap();
            assert_eq!(replay(initial, &code, &[]), desired);

            let initial = (0..=depth).collect::<Vec<_>>();
            let mut desired = initial.clone();
            desired.swap(0, usize::from(depth));
            let mut stack = Stack::new(initial.clone());
            let code = stack.reconcile(&desired, 0, version).unwrap();
            assert_eq!(replay(initial.clone(), &code, &[]), desired);

            let mut desired = initial.clone();
            desired.push(0);
            let mut stack = Stack::new(initial.clone());
            assert_eq!(stack.reconcile(&desired, 0, version), Err(StackError::InaccessibleDepth));
            assert_eq!(stack.values(), initial);
        }
        let mut stack = Stack::new(vec![0; 1024]);
        assert_eq!(
            stack.reconcile(&vec![0; 1025], 0, EvmVersion::Amsterdam),
            Err(StackError::Overflow)
        );
        assert_eq!(stack.values().len(), 1024);
    }

    #[test]
    fn prepare_canonicalizes_live_aliases_and_preserves_operand_order() {
        let initial = vec![9, 1, 2, 1, 3];
        let mut stack = Stack::new(initial.clone());
        let code = stack.prepare(&[1, 2, 1], 1, EvmVersion::Osaka, |value| value == 1).unwrap();
        assert_eq!(replay(initial, &code, &[9]), [9, 1, 1, 2, 1]);
        assert_eq!(stack.values(), [9, 1, 1, 2, 1]);
    }

    #[test]
    fn failed_plans_are_atomic() {
        let initial = (0..20).collect::<Vec<_>>();
        let mut stack = Stack::new(initial.clone());
        assert_eq!(
            stack.reconcile(
                &initial.iter().rev().copied().collect::<Vec<_>>(),
                0,
                EvmVersion::Osaka
            ),
            Err(StackError::InaccessibleDepth)
        );
        assert_eq!(stack.values(), initial);
        assert_eq!(stack.reconcile(&[30], 0, EvmVersion::Amsterdam), Err(StackError::MissingValue));
        assert_eq!(stack.values(), initial);
    }
}
