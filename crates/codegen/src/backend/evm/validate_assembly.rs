//! Validation of assembly: the bytecode the assembler emitted against the assembly program it was
//! solved from, with `-Zvalidate-assembly`.
//!
//! The assembler is a primitive stream encoder (see `assembler/mod.rs`): it resolves label offsets
//! and PUSH widths to a least fixed point and emits bytes. This checker does not repeat that
//! solution. It decodes the emitted bytes in lockstep with the program, one instruction at a time,
//! and checks:
//!
//! - an opcode is its byte, and an opcode with an immediate its two bytes;
//! - a push of a constant decodes to a PUSH whose immediate is the constant, so its width suffices;
//! - a label definition is a `JUMPDEST`, at the offset where the walk finds it, which defines the
//!   label's offset independently of the assembler's layout;
//! - a push of a label decodes to a PUSH whose immediate is the label's offset (of the width the
//!   program fixes, when it fixes one), and a packed push holds each label's offset from its base;
//! - a push of program data decodes to the data's offset plus the reference's, and the data's bytes
//!   are where the walk finds them;
//! - an immutable placeholder is a PUSH of its width with zero bytes, recorded at that offset in
//!   the artifact's immutable references, in order, and a library placeholder a `PUSH20` of zeros
//!   with a relocation at its immediate;
//! - the walk ends exactly at the end of the bytecode.
//!
//! So every pushed label is the offset of a `JUMPDEST` the program defines, and the bytes decode
//! to the program's instructions with their operands resolved: the bytecode runs as the program
//! does. For deployment code, [`check_deployment`] checks that the code ends with the runtime code
//! it deploys, where the program places it. A failure is a compiler bug and stops compilation.
//!
//! # Limits
//!
//! - The checker relates bytes to the assembly program, not to EVM IR: the EVM IR validators cover
//!   the steps before it, and lowering EVM IR to the assembly program is outside both.
//! - That the deployment code's `CODECOPY` copies exactly the runtime it embeds is a property of
//!   the code's data flow: this checks that the runtime bytes end the code, and that the copy's
//!   offset push resolves to their start through the program's data reference, not the copy's
//!   operands.

use crate::backend::{
    assembler::{AsmInstKind, AssembledCode, Assembler, Label, assembly},
    evm::{ir, op},
};
use alloy_primitives::U256;
use solar_data_structures::map::FxHashMap;

/// Checks that `code` is the bytecode of `program`, with `fault` injected into the checker's copy
/// of the bytecode when given (`-Zvalidate-assembly-fault`): `label` adds one to the first label
/// push, and `width` widens the first label push by one byte.
pub(in crate::backend) fn check(
    asm: &Assembler<'_>,
    program: &assembly::Program,
    code: &AssembledCode,
    fault: Option<&str>,
) -> Result<(), String> {
    let mut bytes = code.bytecode.clone();
    if let Some(fault) = fault {
        let walk = walk(asm, program, code, &bytes)?;
        if let Some(&(pc, ..)) = walk.label_pushes.first() {
            let width = push_width(bytes[pc]);
            match fault {
                "label" if width > 0 => bytes[pc + width] = bytes[pc + width].wrapping_add(1),
                "width" if width < 32 => {
                    bytes[pc] = op::push(width as u8 + 1);
                    bytes.insert(pc + 1, 0);
                }
                _ => {}
            }
        }
    }
    let walk = walk(asm, program, code, &bytes)?;
    for &(pc, label, value) in &walk.label_pushes {
        let target =
            *walk.labels.get(&label).ok_or_else(|| format!("{label:?} is never defined"))?;
        if value != U256::from(target) {
            return Err(format!("the push at {pc} holds {value}, not {label:?} at {target}"));
        }
    }
    for &(pc, labels, value) in &walk.packed_pushes {
        let labels = &program.packed_labels[labels];
        let base = match labels.base {
            Some(base) => {
                *walk.labels.get(&base).ok_or_else(|| format!("{base:?} is never defined"))?
            }
            None => 0,
        };
        let width = usize::from(labels.label_width) * 8;
        for (index, label) in labels.labels.iter().enumerate() {
            let target =
                *walk.labels.get(label).ok_or_else(|| format!("{label:?} is never defined"))?;
            let field = (value >> (index * width)) & ((U256::ONE << width) - U256::ONE);
            if target < base || field != U256::from(target - base) {
                return Err(format!("entry {index} of the packed push at {pc} is not {label:?}"));
            }
        }
    }
    for &(pc, reference, value) in &walk.data_pushes {
        let reference = program.data_refs[reference];
        let start = *walk
            .data
            .get(&reference.id)
            .ok_or_else(|| format!("the data the push at {pc} names is never placed"))?;
        let len = program.data[reference.id].bytes.linked().len();
        if reference.offset as usize > len || value != U256::from(start + reference.offset as usize)
        {
            return Err(format!("the data push at {pc} holds {value}, not its data's offset"));
        }
    }
    if walk.immutables != code.immutable_refs.len() {
        return Err("the immutable references are not the placeholders".into());
    }
    Ok(())
}

/// Checks that deployment code `deployment` ends with the runtime code `runtime` it deploys.
pub(in crate::backend) fn check_deployment(
    deployment: &[u8],
    runtime: &[u8],
) -> Result<(), String> {
    if deployment.is_empty() || deployment.ends_with(runtime) {
        Ok(())
    } else {
        Err("the deployment code does not end with the runtime code it deploys".into())
    }
}

/// What the walk found.
#[derive(Default)]
struct Walk {
    /// Each label's offset, where its `JUMPDEST` is.
    labels: FxHashMap<Label, usize>,
    /// Each placed data entry's offset.
    data: FxHashMap<ir::DataId, usize>,
    /// Each label push: its offset, label and immediate.
    label_pushes: Vec<(usize, Label, U256)>,
    /// Each packed label push: its offset, labels and immediate.
    packed_pushes: Vec<(usize, assembly::PackedLabelsId, U256)>,
    /// Each data push: its offset, data reference and immediate.
    data_pushes: Vec<(usize, assembly::DataRefId, U256)>,
    /// How many immutable placeholders it found.
    immutables: usize,
}

/// Decodes `bytes` in lockstep with `program`.
fn walk(
    asm: &Assembler<'_>,
    program: &assembly::Program,
    code: &AssembledCode,
    bytes: &[u8],
) -> Result<Walk, String> {
    let mut walk = Walk::default();
    let mut pc = 0;
    let byte = |pc: usize| bytes.get(pc).copied().ok_or_else(|| format!("the code ends at {pc}"));
    for (index, inst) in program.instructions.iter().enumerate() {
        let at = |what: &str| format!("instruction {index} at {pc}: {what}");
        match inst.kind() {
            AsmInstKind::Op(opcode) => {
                if (op::PUSH0..=op::PUSH32).contains(&opcode) || byte(pc)? != opcode {
                    return Err(at(&format!("not opcode {opcode:#04x}")));
                }
                pc += 1;
            }
            AsmInstKind::OpImmediate(opcode, immediate) => {
                if byte(pc)? != opcode || byte(pc + 1)? != immediate {
                    return Err(at("not its opcode and immediate"));
                }
                pc += 2;
            }
            AsmInstKind::PushInline(value) => {
                let (found, next) = push(bytes, pc).ok_or_else(|| at("not a push"))?;
                if found != U256::from(value) {
                    return Err(at(&format!("pushes {found}, not {value}")));
                }
                pc = next;
            }
            AsmInstKind::Push(id) => {
                let value = asm.push_value(id);
                let (found, next) = push(bytes, pc).ok_or_else(|| at("not a push"))?;
                if found != value {
                    return Err(at(&format!("pushes {found}, not {value}")));
                }
                pc = next;
            }
            AsmInstKind::PushLabel(label) => {
                let (value, next) = push(bytes, pc).ok_or_else(|| at("not a push"))?;
                walk.label_pushes.push((pc, label, value));
                pc = next;
            }
            AsmInstKind::PushLabelFixed(label, width) => {
                let (value, next) = push(bytes, pc).ok_or_else(|| at("not a push"))?;
                let fits = next - pc - 1 == usize::from(width)
                    || (width == 0 && value.is_zero() && next - pc <= 2);
                if !fits {
                    return Err(at(&format!("not a push of {width} bytes")));
                }
                walk.label_pushes.push((pc, label, value));
                pc = next;
            }
            AsmInstKind::PushPackedLabels(labels) => {
                let packed = &program.packed_labels[labels];
                let width = packed.labels.len() * usize::from(packed.label_width);
                let (value, next) = push(bytes, pc).ok_or_else(|| at("not a push"))?;
                if next - pc - 1 != width {
                    return Err(at(&format!("not a push of {width} bytes")));
                }
                walk.packed_pushes.push((pc, labels, value));
                pc = next;
            }
            AsmInstKind::PushImmutable(id) => {
                let (immutable, type_size) = asm.immutable_placeholder(id);
                let width = usize::from(type_size.bytes());
                let (value, next) = push(bytes, pc).ok_or_else(|| at("not a push"))?;
                let recorded = code.immutable_refs.get(walk.immutables).is_some_and(|reference| {
                    reference.code_offset == pc
                        && reference.type_size == type_size
                        && reference.id == immutable
                });
                if next - pc - 1 != width || !value.is_zero() || !recorded {
                    return Err(at("not its recorded immutable placeholder"));
                }
                walk.immutables += 1;
                pc = next;
            }
            AsmInstKind::PushLibrary(id) => {
                let (value, next) = push(bytes, pc).ok_or_else(|| at("not a push"))?;
                let relocated = code
                    .library_relocations
                    .iter()
                    .any(|relocation| relocation.offset == pc + 1 && relocation.library == id);
                if next - pc != 21 || !value.is_zero() || !relocated {
                    return Err(at("not its relocated library placeholder"));
                }
                pc = next;
            }
            AsmInstKind::Label(label) => {
                if byte(pc)? != op::JUMPDEST {
                    return Err(at("not a JUMPDEST"));
                }
                if walk.labels.insert(label, pc).is_some() {
                    return Err(at(&format!("{label:?} is defined twice")));
                }
                pc += 1;
            }
            AsmInstKind::PushData(reference) => {
                let (value, next) = push(bytes, pc).ok_or_else(|| at("not a push"))?;
                walk.data_pushes.push((pc, reference, value));
                pc = next;
            }
            AsmInstKind::Data(data) => {
                let expected = program.data[data].bytes.linked();
                if bytes.get(pc..pc + expected.len()) != Some(&expected[..]) {
                    return Err(at("not its data"));
                }
                walk.data.insert(data, pc);
                pc += expected.len();
            }
        }
    }
    if pc != bytes.len() {
        return Err(format!("the program ends at {pc}, the code at {}", bytes.len()));
    }
    Ok(walk)
}

/// The immediate width of the PUSH `opcode`, or 0 for any other.
fn push_width(opcode: u8) -> usize {
    if (op::PUSH1..=op::PUSH32).contains(&opcode) { usize::from(opcode - op::PUSH1) + 1 } else { 0 }
}

/// The value the PUSH at `pc` pushes and the offset after it.
fn push(bytes: &[u8], pc: usize) -> Option<(U256, usize)> {
    let opcode = *bytes.get(pc)?;
    if opcode == op::PUSH0 {
        return Some((U256::ZERO, pc + 1));
    }
    let width = push_width(opcode);
    if width == 0 {
        return None;
    }
    let immediate = bytes.get(pc + 1..pc + 1 + width)?;
    Some((U256::from_be_slice(immediate), pc + 1 + width))
}
