//! Per-function constant-use facts for the ISLE context.
//!
//! `unshared_mask` and `compared_mask` ask about every instruction of the
//! function, so answering them by scanning would make the pass quadratic. The
//! index counts operand uses by constant, and keeps the `And` results of each
//! `(value, mask)` pair and the constants compared with each value. It covers
//! the instructions that a scan would visit: the attached blocks and the block
//! being rewritten.
//!
//! The pass keeps it valid. A committed root is removed before its kind
//! changes and inserted after, and its `(x & m) & m` users are refreshed.
//! Producers deleted from a block leave the index, and recipe instructions
//! enter it, when the block is reattached.

use crate::mir::{Function, InstId, InstKind, Value, ValueId};
use alloy_primitives::U256;
use solar_data_structures::map::FxHashMap;

#[derive(Default)]
pub(super) struct MaskUses {
    /// Operand uses of each constant, without the repeated masks of `(x & m) & m`.
    uses: FxHashMap<U256, usize>,
    /// The instructions of `(x & m) & m`, with their mask, whose outer mask is not counted.
    repeated: FxHashMap<InstId, U256>,
    /// The instructions `y & m` by their inner instruction result `y`.
    outers: FxHashMap<ValueId, Vec<InstId>>,
    /// The results of `x & m` by `(x, m)`.
    masked: FxHashMap<(ValueId, U256), Vec<ValueId>>,
    /// The constants compared by `Eq` with each other operand.
    compared: FxHashMap<ValueId, Vec<U256>>,
}

impl MaskUses {
    pub(super) fn new(func: &Function, block: &[InstId]) -> Self {
        let mut index = Self::default();
        for inst in func.instructions().chain(block.iter().copied()) {
            index.insert(func, inst);
        }
        index
    }

    /// Whether the constant has at most one use that a repeated mask would not remove.
    pub(super) fn unshared(&self, mask: U256) -> bool {
        self.uses.get(&mask).is_none_or(|&uses| uses <= 1)
    }

    /// The constants that an equality compares with `value & mask` and the mask keeps.
    pub(super) fn compared(&self, value: ValueId, mask: U256) -> impl Iterator<Item = U256> {
        self.masked
            .get(&(value, mask))
            .into_iter()
            .flatten()
            .filter_map(|result| self.compared.get(result))
            .flatten()
            .copied()
            .filter(move |&c| c & mask == c)
    }

    /// Adds an instruction with its current kind.
    pub(super) fn insert(&mut self, func: &Function, inst: InstId) {
        let kind = &func.inst(inst).kind;
        kind.visit_operands(|operand| {
            if let Some(constant) = func.value_u256(operand) {
                *self.uses.entry(constant).or_default() += 1;
            }
        });
        match *kind {
            InstKind::And(value, mask) => {
                if let Some(mask) = func.value_u256(mask)
                    && let Some(result) = func.inst_result_value(inst)
                {
                    self.masked.entry((value, mask)).or_default().push(result);
                    if matches!(func.value(value), Value::Inst(_)) {
                        self.outers.entry(value).or_default().push(inst);
                    }
                }
            }
            InstKind::Eq(a, b) => {
                for (operand, constant) in [(a, b), (b, a)] {
                    if let Some(constant) = func.value_u256(constant) {
                        self.compared.entry(operand).or_default().push(constant);
                    }
                }
            }
            _ => {}
        }
        if let Some(mask) = repeated_mask(func, inst) {
            *self.uses.entry(mask).or_default() -= 1;
            self.repeated.insert(inst, mask);
        }
    }

    /// Removes an instruction. Call it before the instruction's kind changes.
    pub(super) fn remove(&mut self, func: &Function, inst: InstId) {
        if let Some(mask) = self.repeated.remove(&inst) {
            *self.uses.entry(mask).or_default() += 1;
        }
        let kind = &func.inst(inst).kind;
        kind.visit_operands(|operand| {
            if let Some(constant) = func.value_u256(operand) {
                *self.uses.entry(constant).or_default() -= 1;
            }
        });
        match *kind {
            InstKind::And(value, mask) => {
                if let Some(mask) = func.value_u256(mask)
                    && let Some(result) = func.inst_result_value(inst)
                {
                    swap_remove(self.masked.get_mut(&(value, mask)), &result);
                    if matches!(func.value(value), Value::Inst(_)) {
                        swap_remove(self.outers.get_mut(&value), &inst);
                    }
                }
            }
            InstKind::Eq(a, b) => {
                for (operand, constant) in [(a, b), (b, a)] {
                    if let Some(constant) = func.value_u256(constant) {
                        swap_remove(self.compared.get_mut(&operand), &constant);
                    }
                }
            }
            _ => {}
        }
    }

    /// Recounts the `(x & m) & m` users of an instruction result after its kind changed.
    pub(super) fn refresh(&mut self, func: &Function, inst: InstId) {
        let Some(result) = func.inst_result_value(inst) else { return };
        let Some(outers) = self.outers.get(&result) else { return };
        for &outer in outers {
            let mask = repeated_mask(func, outer);
            if self.repeated.get(&outer) != mask.as_ref() {
                if let Some(old) = self.repeated.remove(&outer) {
                    *self.uses.entry(old).or_default() += 1;
                }
                if let Some(mask) = mask {
                    *self.uses.entry(mask).or_default() -= 1;
                    self.repeated.insert(outer, mask);
                }
            }
        }
    }
}

/// The mask of `(x & m) & m`, whose outer mask disappears with the inner one.
fn repeated_mask(func: &Function, inst: InstId) -> Option<U256> {
    let InstKind::And(inner, outer) = func.inst(inst).kind else { return None };
    let mask = func.value_u256(outer)?;
    let Value::Inst(inner) = *func.value(inner) else { return None };
    matches!(func.inst(inner).kind, InstKind::And(_, m) if func.value_u256(m) == Some(mask))
        .then_some(mask)
}

fn swap_remove<T: PartialEq>(items: Option<&mut Vec<T>>, item: &T) {
    if let Some(items) = items
        && let Some(index) = items.iter().position(|other| other == item)
    {
        items.swap_remove(index);
    }
}
