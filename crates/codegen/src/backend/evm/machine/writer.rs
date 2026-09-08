//! Protects live spill homes around one source MSTORE.
//!
//! A 32-byte store intersects at most two aligned words. Instead of backing up every
//! initialized word in a sufficiently long absolute run, select those two words at
//! runtime, loading both before the source store and restoring them immediately afterward.
//! Out-of-run candidates select the first initialized home; duplicate selections therefore
//! save and restore the same original value. Restores may run in either order because
//! selected aligned homes are disjoint or identical. Modular subtraction handles arbitrary source
//! pointers without a heap-region or free-memory-pointer assumption.
//!
//! The longest contiguous run is the first candidate. On forks with SHR, a bitmap may select
//! all initialized homes in a span of at most 256 words, leaving holes untouched. It must beat
//! the full contiguous incumbent including its remaining ordinary backups. Other backups retain
//! their ordinary protocol, and relative homes are excluded. The straight-line template consumes
//! exactly the original value and destination, touches no uninitialized memory, and needs five
//! temporary words. The second selection reuses the first modular address delta,
//! adding one word width after rotating the first saved pair below it. Bitmap selections
//! instead keep a word offset, testing membership before shifting the selected word back
//! to a byte address. Negative offsets cannot name a bitmap bit; wrapping the last EVM
//! word to zero either selects home zero or the same initialized fallback.
//! Its exact literal bytes and static gas must improve the ordinary run protection. The
//! caller retains the original stack-capacity check; at least eleven removed backups pay
//! for the template's five temporaries. Final scheduling and outlining remain measured
//! properties rather than guarantees of this local cost comparison. Size mode retains ordinary
//! backups so repeated load/store runs remain available to outlining.

//! When the selected template covers every backup and preparation emitted no code,
//! an immediately preceding homed address can keep one copy for this writer. Its
//! home store remains unchanged; DUP1 and the existing operand SWAP1 replace the
//! reload at equal gas. Exact producer, frozen prefix and canonical tail checks keep
//! this local to one boundary. The caller shifts the debug range past the inserted
//! copy so the preceding store retains its source origin. No residence, home or
//! protection decision changes.

use super::{Context, Slot};
use crate::{
    backend::evm::{ir, op, scheduler::Stack, storage::FrameAddress},
    mir,
};
use alloy_primitives::U256;
use ir::InstKind::{Dup, Op, Push, Swap};
use solar_config::EvmVersion;
use std::ops::Range;

pub(super) struct Protection {
    pub(super) range: Range<usize>,
    pub(super) instructions: Vec<ir::Instruction>,
}

/// Keeps an immediately produced address above its unchanged absolute spill store.
/// The caller has selected a writer template with no ordinary backups and emitted no preparation.
/// Its seven-word operand/template peak covers the extra copy and bounded operand materialization.
/// Only the address is retained: DUP1 plus SWAP1 replaces one PUSH plus MLOAD at equal gas.
pub(super) fn retain_address(
    context: &Context<'_>,
    (block, position): (mir::BlockId, usize),
    stack: &mut Stack<Slot>,
    output: &mut Vec<ir::Instruction>,
) -> bool {
    let Some(previous) = position
        .checked_sub(1)
        .map(|position| context.function.blocks[block].instructions[position])
    else {
        return false;
    };
    let inst = context.function.blocks[block].instructions[position];
    let mir::InstKind::MStore(address, value) = context.function.inst(inst).kind else {
        return false;
    };
    if address == value
        || context.function.inst_result_value(previous) != Some(address)
        || context
            .function
            .inst(previous)
            .kind
            .evm_opcode()
            .and_then(op::stack_io)
            .is_none_or(|(_, outputs)| outputs != 1)
        || context.layout.suppressed.as_ref().is_some_and(|set| set.contains(previous))
        || context.layout.rematerialized.contains_key(&address)
    {
        return false;
    }
    let Some(&home) = context.layout.spills.homes.get(&address) else { return false };
    let Ok(FrameAddress::Absolute(home)) = context.storage.spill_address(home) else {
        return false;
    };
    let Some(start) = output.len().checked_sub(2) else { return false };
    if (home == 0 && context.version.has_push0())
        || output[start].kind != ir::InstKind::Push(U256::from(home))
        || output[start + 1].kind != ir::InstKind::Op(op::MSTORE)
        || !ir::split_allowed(output, start)
        || output[start..].iter().any(|inst| inst.keep_with_next || inst.stack_effect.is_some())
    {
        return false;
    }
    // address; dup1; push home; mstore
    // <materialize value>; swap1; <unchanged protected writer>
    let mut duplicate = ir::Instruction::from(ir::InstKind::Dup(1));
    duplicate.debug = output[start].debug.clone();
    output.insert(start, duplicate);
    stack.push(Slot::Value(address));
    true
}

/// The first `spill_homes` entries are initialized live value homes, never protocol words.
pub(super) fn choose(
    addresses: &[FrameAddress],
    spill_homes: usize,
    version: EvmVersion,
) -> Option<Protection> {
    // A bitmap costs at least 127 gas; ten backups and the source store pay at most 123.
    if spill_homes < 11
        || addresses.iter().any(|address| !matches!(address, FrameAddress::Absolute(_)))
    {
        return None;
    }
    let incumbent = contiguous(addresses, spill_homes, version);
    if !version.has_bitwise_shifting()
        || incumbent.as_ref().is_some_and(|plan| plan.range == (0..spill_homes))
    {
        return incumbent;
    }
    let Some((start, mask)) = membership(addresses, spill_homes) else { return incumbent };
    let instructions = template(start, Selection::Bitmap(mask));
    let (new_bytes, new_gas) = cost(version, &instructions);
    let (mut old_bytes, mut old_gas) = backup_cost(version, &addresses[..spill_homes]);
    old_bytes += 1;
    old_gas += 3;
    if let Some(plan) = &incumbent {
        let removed = backup_cost(version, &addresses[plan.range.clone()]);
        let replacement = cost(version, &plan.instructions);
        old_bytes = old_bytes - removed.0 - 1 + replacement.0;
        old_gas = old_gas - removed.1 - 3 + replacement.1;
    }
    if new_bytes < old_bytes && new_gas <= old_gas {
        Some(Protection { range: 0..spill_homes, instructions })
    } else {
        incumbent
    }
}

/// Counts the selected template, all unselected ordinary backups and one source MSTORE.
///
/// This local cost excludes surrounding scheduling and later outlining; final artifact and
/// runtime comparisons are still required. Relative addresses are outside this trial's scope.
pub(super) fn protection_cost(
    version: EvmVersion,
    addresses: &[FrameAddress],
    protection: Option<&Protection>,
) -> Option<(usize, usize)> {
    if addresses.iter().any(|address| !matches!(address, FrameAddress::Absolute(_))) {
        return None;
    }
    let (mut bytes, mut gas) = backup_cost(version, addresses);
    bytes += 1;
    gas += 3;
    if let Some(protection) = protection {
        let removed = backup_cost(version, addresses.get(protection.range.clone())?);
        let replacement = cost(version, &protection.instructions);
        bytes = bytes - removed.0 - 1 + replacement.0;
        gas = gas - removed.1 - 3 + replacement.1;
    }
    Some((bytes, gas))
}

fn contiguous(
    addresses: &[FrameAddress],
    spill_homes: usize,
    version: EvmVersion,
) -> Option<Protection> {
    let mut best = 0..0;
    let mut current = 0..0;
    let mut previous = None;
    for (index, &address) in addresses[..spill_homes].iter().enumerate() {
        let FrameAddress::Absolute(address) = address else { return None };
        if address % 32 != 0 {
            current = index + 1..index + 1;
            previous = None;
            continue;
        }
        if previous.and_then(|address: u64| address.checked_add(32)) == Some(address) {
            current.end = index + 1;
        } else {
            current = index..index + 1;
        }
        previous = Some(address);
        if current.len() > best.len() {
            best = current.clone();
        }
    }
    if best.len() < 12 {
        return None;
    }
    let FrameAddress::Absolute(start) = addresses[best.start] else { return None };
    let bytes = u64::try_from(best.len()).ok()?.checked_mul(32)?;
    let end = start.checked_add(bytes)?;
    // Unselected backups must not overlap the run, including any appended protocol words.
    if addresses.iter().enumerate().any(|(index, &address)| {
        let FrameAddress::Absolute(address) = address else { return true };
        !best.contains(&index)
            && address < end
            && address.checked_add(32).is_none_or(|end| end > start)
    }) {
        return None;
    }
    let instructions = template(start, Selection::Contiguous(bytes));
    let (new_bytes, new_gas) = cost(version, &instructions);
    let (old_bytes, old_gas) = backup_cost(version, &addresses[best.clone()]);
    if new_bytes > old_bytes || new_gas > old_gas + 3 {
        return None;
    }
    Some(Protection { range: best, instructions })
}

/// A bitmap bit names one initialized word; absent bits must never be read or restored.
fn membership(addresses: &[FrameAddress], spill_homes: usize) -> Option<(u64, U256)> {
    let start = addresses[..spill_homes]
        .iter()
        .map(|address| match address {
            FrameAddress::Absolute(address) => *address,
            FrameAddress::Relative(_) => unreachable!(),
        })
        .min()?;
    let mut mask = U256::ZERO;
    let mut end = start;
    for &address in &addresses[..spill_homes] {
        let FrameAddress::Absolute(address) = address else { return None };
        let index = (address - start) / 32;
        if address % 32 != 0 || index >= 256 {
            return None;
        }
        mask |= U256::ONE << (index as usize);
        end = end.max(address.checked_add(32)?);
    }
    // Conservatively keep protocol words outside the whole span, including bitmap holes.
    if addresses[spill_homes..].iter().any(|address| {
        let FrameAddress::Absolute(address) = *address else { return true };
        address < end && address.checked_add(32).is_none_or(|end| end > start)
    }) {
        return None;
    }
    Some((start, mask))
}

#[derive(Clone, Copy)]
enum Selection {
    Contiguous(u64),
    Bitmap(U256),
}

fn backup_cost(version: EvmVersion, addresses: &[FrameAddress]) -> (usize, usize) {
    addresses.iter().fold((0, 0), |(bytes, gas), address| {
        let FrameAddress::Absolute(address) = address else { unreachable!() };
        let push_gas = if *address == 0 && version.has_push0() { 2 } else { 3 };
        (bytes + 2 * (op::push_len(version, U256::from(*address)) + 1), gas + 2 * (push_gas + 3))
    })
}

fn template(start: u64, selection: Selection) -> Vec<ir::Instruction> {
    let bitmap = matches!(selection, Selection::Bitmap(_));
    let base = if bitmap { start / 32 } else { start };
    let mut output = Vec::with_capacity(if bitmap { 46 } else { 39 });
    // value; destination
    // value; destination; offset; offset
    // offset = (destination >> 5) - base for bitmap, otherwise aligned bytes - base
    output.push(Dup(1).into());
    if bitmap {
        // value; destination; destination >> 5
        output.extend([Push(U256::from(5)), Op(op::SHR)].map(Into::into));
    } else {
        // value; destination; destination & !31
        output.extend([Push(U256::from(31)), Op(op::NOT), Op(op::AND)].map(Into::into));
    }
    // value; destination; offset; offset
    output.extend([Push(U256::from(base)), Swap(1), Op(op::SUB), Dup(1)].map(Into::into));
    for second in [false, true] {
        if second {
            // value; destination; offset; address0; old0
            // value; destination; old0; address0; offset + step
            output.extend(
                [Swap(2), Push(U256::from(if bitmap { 1 } else { 32 })), Op(op::ADD)]
                    .map(Into::into),
            );
        }
        // offset; offset
        output.push(Dup(1).into());
        match selection {
            Selection::Contiguous(bytes) => {
                // offset; offset < bytes
                output.extend([Push(U256::from(bytes)), Swap(1), Op(op::LT)].map(Into::into));
            }
            Selection::Bitmap(mask) => {
                // offset; bit = (mask >> offset) & 1
                output.extend(
                    [Push(mask), Swap(1), Op(op::SHR), Push(U256::ONE), Op(op::AND)]
                        .map(Into::into),
                );
            }
        }
        // selected = base + offset * bit
        output.extend([Op(op::MUL), Push(U256::from(base)), Op(op::ADD)].map(Into::into));
        if bitmap {
            // selected_word; selected_word << 5
            output.extend([Push(U256::from(5)), Op(op::SHL)].map(Into::into));
        }
        // selected_address; mload(selected_address)
        output.extend([Dup(1), Op(op::MLOAD)].map(Into::into));
    }
    // value; destination; old0; address0; address1; old1
    // old1; address1; old0; address0; value; destination
    // mstore(destination, value)
    // mstore(address0, old0)
    // mstore(address1, old1)
    output.extend(
        [Swap(5), Swap(1), Swap(4), Op(op::MSTORE), Op(op::MSTORE), Op(op::MSTORE)].map(Into::into),
    );
    output
}

/// Exact static cost of this restricted template; existing memory extent pays expansion.
fn cost(version: EvmVersion, instructions: &[ir::Instruction]) -> (usize, usize) {
    instructions.iter().fold((0, 0), |(bytes, gas), instruction| {
        let (size, price) = match instruction.kind {
            ir::InstKind::Push(value) => (
                op::push_len(version, value),
                if value.is_zero() && version.has_push0() { 2 } else { 3 },
            ),
            ir::InstKind::Op(op::MUL) => (1, 5),
            ir::InstKind::Dup(_)
            | ir::InstKind::Swap(_)
            | ir::InstKind::Op(
                op::ADD
                | op::SUB
                | op::LT
                | op::NOT
                | op::AND
                | op::SHR
                | op::SHL
                | op::MLOAD
                | op::MSTORE,
            ) => (1, 3),
            _ => unreachable!("unexpected selected-home template instruction"),
        };
        (bytes + size, gas + price)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_contract_and_exact_cost() {
        for version in [EvmVersion::Byzantium, EvmVersion::London, EvmVersion::Osaka] {
            let instructions = template(480, Selection::Contiguous(28 * 32));
            assert_eq!(ir::scheduling_usage(&instructions), Some((2, -2, 5)));
            assert_eq!(cost(version, &instructions), (47, 109));
        }
    }

    #[test]
    fn only_initialized_disjoint_profitable_runs() {
        let mut homes =
            (0..28).map(|index| FrameAddress::Absolute(480 + index * 32)).collect::<Vec<_>>();
        assert_eq!(choose(&homes, 28, EvmVersion::London).unwrap().range, 0..28);
        assert!(choose(&homes, 10, EvmVersion::London).is_none());
        homes.push(FrameAddress::Absolute(481));
        assert!(choose(&homes, 28, EvmVersion::London).is_none());
        homes.pop();
        homes.push(FrameAddress::Relative(0));
        assert!(choose(&homes, 28, EvmVersion::London).is_none());
        homes.pop();
        homes.push(FrameAddress::Absolute(160));
        assert_eq!(choose(&homes, 28, EvmVersion::London).unwrap().range, 0..28);
    }
    #[test]
    fn bitmap_contract_and_incumbent_selection() {
        let homes =
            (0..28).map(|index| FrameAddress::Absolute(480 + index * 64)).collect::<Vec<_>>();
        let (start, mask) = membership(&homes, homes.len()).unwrap();
        let instructions = template(start, Selection::Bitmap(mask));
        assert_eq!(ir::scheduling_usage(&instructions), Some((2, -2, 5)));
        assert_eq!(cost(EvmVersion::London, &instructions), (65, 130));
        assert_eq!(choose(&homes, homes.len(), EvmVersion::London).unwrap().range, 0..28);
        assert!(choose(&homes, homes.len(), EvmVersion::Byzantium).is_none());

        let dense =
            (0..28).map(|index| FrameAddress::Absolute(480 + index * 32)).collect::<Vec<_>>();
        let selected = choose(&dense, dense.len(), EvmVersion::London).unwrap();
        assert_eq!(selected.instructions, template(480, Selection::Contiguous(28 * 32)));

        // Retiring the home at 7840 leaves eleven initialized homes and its bit unset.
        let eleven = [7360, 7392, 7424, 7488, 7520, 7552, 7584, 7616, 7648, 7680, 7744]
            .map(FrameAddress::Absolute);
        let (start, mask) = membership(&eleven, eleven.len()).unwrap();
        assert_eq!((start, mask), (7360, U256::from(0x17f7)));
        let selected = choose(&eleven, eleven.len(), EvmVersion::Osaka).unwrap();
        assert_eq!(selected.range, 0..11);
        assert_eq!(selected.instructions, template(start, Selection::Bitmap(mask)));
        assert_eq!(protection_cost(EvmVersion::Osaka, &eleven, Some(&selected)), Some((55, 130)));
        assert!(choose(&eleven[..10], 10, EvmVersion::Osaka).is_none());
        assert!(choose(&eleven, eleven.len(), EvmVersion::Byzantium).is_none());
    }

    #[test]
    fn bitmap_span_holes_and_protocol_boundaries() {
        let homes = [FrameAddress::Absolute(480), FrameAddress::Absolute(480 + 255 * 32)];
        let (_, mask) = membership(&homes, homes.len()).unwrap();
        assert_eq!(mask, U256::ONE | (U256::ONE << 255));
        assert!(membership(&[homes[0], FrameAddress::Absolute(480 + 256 * 32)], 2).is_none());
        assert!(membership(&[homes[0], FrameAddress::Absolute(481)], 2).is_none());
        assert!(membership(&[FrameAddress::Absolute(!31u64)], 1).is_none());
        // An appended protocol word in a hole stays on the canonical path.
        assert!(membership(&[homes[0], homes[1], FrameAddress::Absolute(512)], 2).is_none());
    }
}
