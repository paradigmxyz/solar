//! Final rereading of cheap stable environment words instead of physical copies.
//!
//! Within each block, a known suffix of the operand stack records which words
//! came from a stable two-gas environment opcode. DUP/SWAP/EXCHANGE move those
//! facts; other instructions consume their inputs and produce unknown words.
//! An accessible legacy DUP of a known word becomes the same environment read,
//! preserving every original word, the net height and the peak. Both encodings
//! are one byte. The source read must be available on the selected fork.
//!
//! This runs after literal orientation, leaving earlier CSE and stack identities
//! intact for normalization and outlining. Code/gas observations and external
//! calls use the existing module-wide exclusion. NUMBER and mutable reads are
//! deliberately excluded. Explicit stack overrides are not rewritten; unknown
//! operations or inaccessible stack accesses forget facts. No CFG or height
//! analysis, value scheduling, or assembler optimization is introduced.

use super::{EvmPass, InstKind, Instruction, Module, canonical, literal_observers_allow, verify};
use crate::backend::evm::op;
use solar_config::EvmVersion;
use solar_sema::Gcx;

pub(crate) struct EnvironmentCopies;

impl EvmPass for EnvironmentCopies {
    fn name(&self) -> &'static str {
        "environment-copies"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        if !literal_observers_allow(module) {
            return false;
        }
        let mut changed = false;
        for id in module.block_ids().collect::<Vec<_>>() {
            let insts = &mut module.blocks[id].insts;
            if insts.iter().any(|inst| matches!(inst.kind, InstKind::Op(code) if cheap(code))) {
                changed |= reread(insts, gcx.sess.opts.evm_version);
            }
        }
        changed
    }
}

fn reread(insts: &mut [Instruction], version: EvmVersion) -> bool {
    let mut stack = Vec::new();
    let mut changed = false;
    for inst in insts {
        if !canonical(inst) {
            stack.clear();
            continue;
        }
        let len = stack.len();
        match inst.kind {
            InstKind::Dup(depth) if depth > 0 && usize::from(depth) <= len => {
                let value = stack[len - usize::from(depth)];
                // <resident environment word>; dupN -> <resident word>; <same read>
                if depth <= 16
                    && inst.stack_effect.is_none()
                    && let Some(code) = value
                {
                    inst.kind = InstKind::Op(code);
                    changed = true;
                }
                stack.push(value);
            }
            InstKind::Swap(depth) if depth > 0 && usize::from(depth) < len => {
                stack.swap(len - 1, len - 1 - usize::from(depth));
            }
            InstKind::Exchange(a, b) if usize::from(a.max(b)) < len => {
                stack.swap(len - 1 - usize::from(a), len - 1 - usize::from(b));
            }
            InstKind::Dup(_) | InstKind::Swap(_) | InstKind::Exchange(..) => {
                // NOTE: The untracked incoming prefix has no environment identity.
                // Forget the suffix rather than infer facts through a deep access.
                stack.clear();
            }
            _ => {
                if let Some((inputs, outputs)) = verify::effect(&inst.kind) {
                    stack.truncate(len.saturating_sub(usize::from(inputs)));
                    let value = match inst.kind {
                        InstKind::Op(code) if cheap(code) && op::available(code, version) => {
                            Some(code)
                        }
                        _ => None,
                    };
                    stack.extend(std::iter::repeat_n(value, usize::from(outputs)));
                } else {
                    stack.clear();
                }
            }
        }
    }
    changed
}

fn cheap(opcode: u8) -> bool {
    matches!(
        opcode,
        op::ADDRESS
            | op::ORIGIN
            | op::CALLER
            | op::CALLVALUE
            | op::CALLDATASIZE
            | op::GASPRICE
            | op::COINBASE
            | op::TIMESTAMP
            | op::PREVRANDAO
            | op::GASLIMIT
            | op::CHAINID
            | op::BASEFEE
            | op::BLOBBASEFEE
    )
}
