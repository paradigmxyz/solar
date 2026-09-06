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

    /// Places unique last-use operands without fixing the retained values' order.
    /// Callers must allow the private retained suffix to change order. In
    /// particular, saved protocol words must continue using `prepare` instead.
    pub(crate) fn prepare_dead_operands(
        &mut self,
        pop_order: &[T],
        fixed_prefix: usize,
        evm_version: EvmVersion,
        mut retained: impl FnMut(T) -> bool,
    ) -> Option<Vec<ir::Instruction>> {
        if self.values.len() > 1024
            || fixed_prefix > self.values.len()
            || pop_order.is_empty()
            || pop_order.len() > self.values.len() - fixed_prefix
            || pop_order.iter().enumerate().any(|(index, &operand)| {
                retained(operand)
                    || self.count(operand) != 1
                    || self.values[..fixed_prefix].contains(&operand)
                    || pop_order[..index].contains(&operand)
            })
            || self.values[fixed_prefix..].iter().enumerate().any(|(index, &value)| {
                (!pop_order.contains(&value) && !retained(value))
                    || self.values[fixed_prefix..fixed_prefix + index].contains(&value)
            })
        {
            return None;
        }
        // Keep the established unary trial's shallow-operand boundary unchanged.
        if let [operand] = pop_order
            && self.values[self.values.len().saturating_sub(2)..].contains(operand)
        {
            return None;
        }
        let mut desired = self.values.clone();
        // <fixed prefix>; <retained values in unconstrained order>; <reverse pop order>
        for (depth, operand) in pop_order.iter().enumerate() {
            let source = desired.iter().position(|value| value == operand)?;
            let destination = desired.len() - 1 - depth;
            desired.swap(source, destination);
        }
        self.reconcile(&desired, fixed_prefix, evm_version).ok()
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
            // No excess remains, so equal lengths imply equal identity counts.
            if self.values.len() == desired.len() {
                break;
            }
            let required = desired.iter().filter(|&&other| other == value).count();
            // Each appended copy increases this identity count by exactly one.
            for _ in self.count(value)..required {
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

    fn replay<T: Copy + Eq + std::fmt::Debug>(
        mut values: Vec<T>,
        instructions: &[ir::Instruction],
        prefix: &[T],
    ) -> Vec<T> {
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
        for (version, depth) in [(EvmVersion::Osaka, 16u8), (EvmVersion::Amsterdam, 235)] {
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
    fn writer_operands_preserve_long_protected_prefixes() {
        for version in [EvmVersion::Osaka, EvmVersion::Amsterdam] {
            for prefix_len in [1, 8, 16, 17, 255, 1018, 1019, 1020, 1021, 1022, 1023, 1024] {
                let prefix = (0..prefix_len as u16).collect::<Vec<_>>();
                for arity in 1..=4 {
                    for repeated in [false, true] {
                        let operands = (0..arity)
                            .map(|index| 2048 + if repeated { index / 2 } else { index })
                            .collect::<Vec<u16>>();
                        let mut initial = prefix.clone();
                        for &operand in operands.iter().rev() {
                            if !initial.contains(&operand) {
                                initial.push(operand);
                            }
                        }
                        if initial.len() > 1024 {
                            continue;
                        }
                        let mut expected = prefix.clone();
                        expected.extend(operands.iter().rev());
                        let mut stack = Stack::new(initial.clone());
                        let prepared = stack.prepare(&operands, prefix_len, version, |_| false);
                        if expected.len() > 1024 {
                            assert_eq!(prepared, Err(StackError::Overflow));
                            assert_eq!(stack.values(), initial);
                        } else {
                            let mut executed = replay(initial, &prepared.unwrap(), &prefix);
                            assert_eq!(executed, expected);
                            executed.truncate(executed.len() - operands.len());
                            assert_eq!(executed, prefix);
                        }
                    }
                }
            }
            let prefix_len = version.reachable_stack_depth() + 1;
            let initial = (0..prefix_len).collect::<Vec<_>>();
            let mut stack = Stack::new(initial.clone());
            assert_eq!(
                stack.prepare(&[0], prefix_len, version, |_| false),
                Err(StackError::InaccessibleDepth)
            );
            assert_eq!(stack.values(), initial);
        }
    }

    #[test]
    fn dying_writer_operands_cross_backups_without_reordering_survivors() {
        for version in [EvmVersion::Osaka, EvmVersion::Amsterdam] {
            for prefix_len in [0, 1, 1008, 1012, 1020] {
                let prefix = (0..prefix_len).map(|value| 4096 + value).collect::<Vec<u16>>();
                for residents in 0..=8 {
                    for backups in 1..=12 {
                        for arity in 2..=4 {
                            for repeated in [false, true] {
                                let operands = (0..arity)
                                    .map(
                                        |index| {
                                            if index == 0 || repeated { 50 } else { 300 + index }
                                        },
                                    )
                                    .collect::<Vec<u16>>();
                                let mut initial = prefix.clone();
                                initial.push(50);
                                initial.extend(100..100 + residents);
                                initial.extend(200..200 + backups);
                                for &value in operands.iter().rev() {
                                    if !initial.contains(&value) {
                                        initial.push(value);
                                    }
                                }
                                let mut surviving = prefix.clone();
                                surviving.extend(100..100 + residents);
                                surviving.extend(200..200 + backups);
                                let mut expected = surviving.clone();
                                expected.extend(operands.iter().rev());
                                if initial.len().max(expected.len()) - prefix.len() > 16
                                    || initial.len() > 1024
                                {
                                    continue;
                                }
                                let mut stack = Stack::new(initial.clone());
                                let prepared =
                                    stack.prepare(&operands, prefix.len(), version, |value| {
                                        (100..300).contains(&value)
                                    });
                                if expected.len() > 1024 {
                                    assert_eq!(prepared, Err(StackError::Overflow));
                                    assert_eq!(stack.values(), initial);
                                } else {
                                    let mut executed = replay(initial, &prepared.unwrap(), &prefix);
                                    assert_eq!(executed, expected);
                                    for &operand in &operands {
                                        assert_eq!(executed.pop(), Some(operand));
                                    }
                                    assert_eq!(executed, surviving);
                                    for backup in (200..200 + backups).rev() {
                                        assert_eq!(executed.pop(), Some(backup));
                                    }
                                    assert_eq!(&executed[..prefix.len()], prefix);
                                    assert_eq!(
                                        &executed[prefix.len()..],
                                        (100..100 + residents).collect::<Vec<_>>()
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
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
    fn dead_operand_preserves_prefix_and_live_identities() {
        for len in 3..=18 {
            for prefix in 0..len {
                for operand in prefix..len {
                    let initial = (0..len as u8).collect::<Vec<_>>();
                    let mut stack = Stack::new(initial.clone());
                    let code = stack.prepare_dead_operands(
                        &[operand as u8],
                        prefix,
                        EvmVersion::Osaka,
                        |value| value != operand as u8,
                    );
                    let depth = len - 1 - operand;
                    if depth > 1 && depth <= 16 {
                        let code = code.unwrap();
                        assert_eq!(code.len(), 1);
                        let mut actual = replay(initial.clone(), &code, &initial[..prefix]);
                        assert_eq!(actual, stack.values());
                        assert_eq!(actual.pop(), Some(operand as u8));
                        actual.sort_unstable();
                        assert_eq!(
                            actual,
                            initial.into_iter().filter(|&v| v != operand as u8).collect::<Vec<_>>()
                        );
                    } else {
                        assert!(code.is_none());
                        assert_eq!(stack.values(), initial);
                    }
                }
            }
        }
        for (initial, operand, prefix) in
            [(vec![0, 1, 1, 2], 0, 0), (vec![0, 1, 2], 0, 1), (vec![0, 1, 0, 2], 0, 0)]
        {
            let mut stack = Stack::new(initial.clone());
            assert!(
                stack
                    .prepare_dead_operands(&[operand], prefix, EvmVersion::Osaka, |v| v != operand)
                    .is_none()
            );
            assert_eq!(stack.values(), initial);
        }
        let mut stack = Stack::new(vec![0, 1, 2]);
        assert!(stack.prepare_dead_operands(&[0], 0, EvmVersion::Osaka, |_| true).is_none());
        assert!(stack.prepare_dead_operands(&[0], 0, EvmVersion::Osaka, |v| v == 1).is_none());
        assert_eq!(stack.values(), [0, 1, 2]);
        for (version, depth) in [(EvmVersion::Osaka, 16), (EvmVersion::Amsterdam, 235)] {
            let operand = 1023 - depth;
            let initial = (0..1024_u16).collect::<Vec<_>>();
            let mut stack = Stack::new(initial.clone());
            assert!(
                stack.prepare_dead_operands(&[operand], 0, version, |v| v != operand).is_some()
            );
            let mut expected = initial;
            expected.swap(usize::from(operand), 1023);
            assert_eq!(stack.values(), expected);
        }
        let initial = (0..1025_u16).collect::<Vec<_>>();
        let mut stack = Stack::new(initial.clone());
        assert!(
            stack.prepare_dead_operands(&[1022], 0, EvmVersion::Osaka, |v| v != 1022).is_none()
        );
        assert_eq!(stack.values(), initial);
    }

    #[test]
    fn dead_operands_preserve_pop_order_and_retained_multiset() {
        let mut seed = 0x5132_abcd_u32;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as usize
        };
        for version in [EvmVersion::Osaka, EvmVersion::Amsterdam] {
            for arity in 2..=6 {
                for _ in 0..1000 {
                    let len = arity + next() % (18 - arity);
                    let prefix = next() % (len - arity + 1);
                    let initial = (0..len).collect::<Vec<_>>();
                    let mut operands = initial[prefix..].to_vec();
                    for index in (1..operands.len()).rev() {
                        operands.swap(index, next() % (index + 1));
                    }
                    operands.truncate(arity);
                    let mut stack = Stack::new(initial.clone());
                    let code = stack
                        .prepare_dead_operands(&operands, prefix, version, |v| {
                            !operands.contains(&v)
                        })
                        .unwrap();
                    assert!(code.iter().all(|inst| matches!(inst.kind, ir::InstKind::Swap(_))));
                    let mut actual = replay(initial.clone(), &code, &initial[..prefix]);
                    assert_eq!(actual, stack.values());
                    for &operand in &operands {
                        assert_eq!(actual.pop(), Some(operand));
                    }
                    actual.sort_unstable();
                    assert_eq!(
                        actual,
                        initial.into_iter().filter(|v| !operands.contains(v)).collect::<Vec<_>>()
                    );
                }
            }
        }
    }

    #[test]
    fn dead_operands_reject_aliases_protocols_and_invalid_capacity_atomically() {
        for (initial, operands, prefix) in [
            (vec![0, 1, 2], vec![], 0),
            (vec![0, 1, 2], vec![0, 0], 0),
            (vec![0, 1, 2], vec![0, 3], 0),
            (vec![0, 1, 2], vec![0, 1], 1),
            (vec![0, 1, 2], vec![1, 2], 4),
            (vec![0, 1, 2], vec![1, 2], 2),
            (vec![0, 1, 0, 2], vec![0, 2], 0),
            (vec![0, 1, 2, 3, 3], vec![0, 2], 0),
        ] {
            let mut stack = Stack::new(initial.clone());
            assert!(
                stack
                    .prepare_dead_operands(&operands, prefix, EvmVersion::Osaka, |v| !operands
                        .contains(&v))
                    .is_none()
            );
            assert_eq!(stack.values(), initial);
        }
        // A live operand and a retained protocol word both require canonical preparation.
        for retained in [|_: u16| true, |v: u16| v == 1] {
            let initial = vec![0, 1, 2, 3];
            let mut stack = Stack::new(initial.clone());
            assert!(stack.prepare_dead_operands(&[0, 2], 0, EvmVersion::Osaka, retained).is_none());
            assert_eq!(stack.values(), initial);
        }
        for (version, depth) in [(EvmVersion::Osaka, 16), (EvmVersion::Amsterdam, 235)] {
            for extra in 0..=1 {
                let initial = (0..1024_u16).collect::<Vec<_>>();
                let operands = [1023, 1023 - depth - extra];
                let prefix = usize::from(1023 - depth - extra);
                let mut stack = Stack::new(initial.clone());
                let code = stack
                    .prepare_dead_operands(&operands, prefix, version, |v| !operands.contains(&v));
                if extra == 0 {
                    let code = code.unwrap();
                    let actual = replay(initial.clone(), &code, &initial[..prefix]);
                    assert_eq!(actual, stack.values());
                    assert_eq!(&actual[1022..], &[operands[1], operands[0]]);
                } else {
                    assert!(code.is_none());
                    assert_eq!(stack.values(), initial);
                }
            }
        }
        let initial = (0..1025_u16).collect::<Vec<_>>();
        let mut stack = Stack::new(initial.clone());
        assert!(
            stack
                .prepare_dead_operands(&[1024, 1023], 0, EvmVersion::Osaka, |v| v < 1023)
                .is_none()
        );
        assert_eq!(stack.values(), initial);
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
