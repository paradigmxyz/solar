//! Protects live spill homes around one source MSTORE.
//!
//! A 32-byte store intersects at most two aligned words. Instead of backing up every
//! initialized word in a sufficiently long absolute run, select those two words at
//! runtime, loading both before the source store and restoring them immediately afterward.
//! Out-of-run candidates select the first initialized home; duplicate selections therefore
//! save and restore the same original value. Modular subtraction handles arbitrary source
//! pointers without a heap-region or free-memory-pointer assumption.
//!
//! The longest contiguous run is the first candidate. On forks with SHR, a bitmap may select
//! all initialized homes in a span of at most 256 words, leaving holes untouched. It must beat
//! the full contiguous incumbent including its remaining ordinary backups. Other backups retain
//! their ordinary protocol, and relative homes are excluded. The straight-line template consumes
//! exactly the original value and destination, touches no uninitialized memory, and needs five
//! temporary words. The second selection reuses the first modular address delta,
//! adding one word width after rotating the first saved pair below it.
//! Its exact literal bytes and static gas must improve the ordinary run protection. The
//! caller retains the original stack-capacity check; at least twelve removed backups pay
//! for the template's five temporaries. Final scheduling and outlining remain measured
//! properties rather than guarantees of this local cost comparison. Size mode retains ordinary
//! backups so repeated load/store runs remain available to outlining.

use crate::backend::evm::{ir, op, storage::FrameAddress};
use alloy_primitives::U256;
use ir::InstKind::{Dup, Op, Push, Swap};
use solar_config::EvmVersion;
use std::ops::Range;

pub(super) struct Protection {
    pub(super) range: Range<usize>,
    pub(super) instructions: Vec<ir::Instruction>,
}

/// The first `spill_homes` entries are initialized live value homes, never protocol words.
pub(super) fn choose(
    addresses: &[FrameAddress],
    spill_homes: usize,
    version: EvmVersion,
) -> Option<Protection> {
    // Even zero-width literals cannot make fewer than twelve homes pay the template's gas.
    if spill_homes < 12
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
    let mut output =
        Vec::with_capacity(if matches!(selection, Selection::Bitmap(_)) { 47 } else { 39 });
    // value; destination
    // value; destination; delta; delta
    // delta = (destination & !31) - start
    output.extend(
        [
            Dup(1),
            Push(U256::from(31)),
            Op(op::NOT),
            Op(op::AND),
            Push(U256::from(start)),
            Swap(1),
            Op(op::SUB),
            Dup(1),
        ]
        .map(Into::into),
    );
    for second in [false, true] {
        if second {
            // value; destination; delta; address0; old0
            // value; destination; old0; address0; delta + 32
            output.extend([Swap(2), Push(U256::from(32)), Op(op::ADD)].map(Into::into));
        }
        // delta; delta
        output.push(Dup(1).into());
        match selection {
            Selection::Contiguous(bytes) => {
                // delta; delta < bytes
                output.extend([Push(U256::from(bytes)), Swap(1), Op(op::LT)].map(Into::into));
            }
            Selection::Bitmap(mask) => {
                // delta; index = delta >> 5
                // delta; bit = (mask >> index) & 1
                output.extend(
                    [
                        Push(U256::from(5)),
                        Op(op::SHR),
                        Push(mask),
                        Swap(1),
                        Op(op::SHR),
                        Push(U256::ONE),
                        Op(op::AND),
                    ]
                    .map(Into::into),
                );
            }
        }
        // selected = start + delta * bit
        // selected; mload(selected)
        output.extend(
            [Op(op::MUL), Push(U256::from(start)), Op(op::ADD), Dup(1), Op(op::MLOAD)]
                .map(Into::into),
        );
    }
    // value; destination; old0; address0; address1; old1
    // old0; address0; address1; old1; value; destination
    // mstore(destination, value)
    // mstore(address1, old1)
    // mstore(address0, old0)
    output.extend(
        [
            Swap(1),
            Swap(3),
            Swap(5),
            Swap(1),
            Swap(2),
            Swap(4),
            Op(op::MSTORE),
            Swap(1),
            Op(op::MSTORE),
            Op(op::MSTORE),
        ]
        .map(Into::into),
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
                op::ADD | op::SUB | op::LT | op::NOT | op::AND | op::SHR | op::MLOAD | op::MSTORE,
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
            assert_eq!(cost(version, &instructions), (51, 121));
        }
    }

    #[test]
    fn only_initialized_disjoint_profitable_runs() {
        let mut homes =
            (0..28).map(|index| FrameAddress::Absolute(480 + index * 32)).collect::<Vec<_>>();
        assert_eq!(choose(&homes, 28, EvmVersion::London).unwrap().range, 0..28);
        assert!(choose(&homes, 11, EvmVersion::London).is_none());
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
        assert_eq!(cost(EvmVersion::London, &instructions), (73, 145));
        assert_eq!(choose(&homes, homes.len(), EvmVersion::London).unwrap().range, 0..28);
        assert!(choose(&homes, homes.len(), EvmVersion::Byzantium).is_none());

        let dense =
            (0..28).map(|index| FrameAddress::Absolute(480 + index * 32)).collect::<Vec<_>>();
        let selected = choose(&dense, dense.len(), EvmVersion::London).unwrap();
        assert_eq!(selected.instructions, template(480, Selection::Contiguous(28 * 32)));
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
