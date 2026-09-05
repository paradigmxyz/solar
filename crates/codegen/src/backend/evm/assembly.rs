//! Primitive physical-IR encoding and fixed-point relocation.
//!
//! Block control transfers are lowered once into a compact stream. The stream
//! contains only opcodes, labels, literal or relocatable pushes, and immutable
//! placeholders. It performs no control-flow or instruction optimization. PUSH
//! widths start at their minimum and grow monotonically until all addresses fit;
//! every iteration recomputes all offsets, including embedded data addresses.
//! Immutable widths are fixed by their declarations and references identify the
//! PUSH opcode. Each assembly owns its offsets and output, so failures cannot
//! expose partial bytes or leak relocation state into another module.

use super::{
    ImmutableReference,
    ir::{self, BlockId, DataId, InstKind, TerminatorKind},
    op,
};
use crate::mir::{ImmutableId, TypeSize};
use alloy_primitives::U256;
use solar_config::EvmVersion;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec};
use solar_interface::Result;
use solar_sema::Gcx;

/// Completely resolved bytecode and its immutable patch sites.
pub(crate) struct Encoded {
    pub(crate) bytes: Vec<u8>,
    pub(crate) immutable_references: Vec<ImmutableReference>,
}

/// A stream label; code and data retain separate typed index domains.
#[derive(Clone, Copy)]
enum Label {
    Block(BlockId),
    Data(DataId),
}

/// A value resolved after placement.
enum Value {
    Literal(U256),
    Address(Label, u32),
    ProgramEnd,
    AppendixStart,
    PackedTargets(Vec<BlockId>, usize),
}

/// The compact stream contains no block or MIR semantics.
enum Atom {
    Bytes(Vec<u8>),
    Label(Label),
    Push { value: Value, width: usize },
    Immutable { id: ImmutableId, width: u8 },
}

pub(crate) fn encode(gcx: Gcx<'_>, module: &ir::Module) -> Result<Vec<u8>> {
    assemble(gcx, module).map(|output| output.bytes)
}

pub(crate) fn assemble(gcx: Gcx<'_>, module: &ir::Module) -> Result<Encoded> {
    ir::validate(gcx, module);
    gcx.dcx().has_errors()?;
    let version = gcx.sess.opts.evm_version;
    for width in 1..=4 {
        let lowered = super::indexed::lower(module, width);
        let module = lowered.as_ref();
        ir::validate_encoding(gcx, module)?;
        let atoms =
            lower(module, version, width).map_err(|message| gcx.dcx().err(message).emit())?;
        match resolve(atoms, module, version) {
            Ok(encoded) => return Ok(encoded),
            Err(message) if message == "indexed target exceeds selected address width" => continue,
            Err(message) => return Err(gcx.dcx().err(message).emit()),
        }
    }
    Err(gcx.dcx().err("indexed EVM target address exceeds four bytes").emit())
}

fn push(atoms: &mut Vec<Atom>, value: Value, version: EvmVersion) {
    let width = match &value {
        Value::Literal(value) => op::push_len(version, *value) - 1,
        _ => usize::from(!version.has_push0()),
    };
    // push <unresolved value>
    atoms.push(Atom::Push { value, width });
}

fn jump(atoms: &mut Vec<Atom>, target: BlockId, opcode: u8, version: EvmVersion) {
    // push <target>
    // jump / jumpi
    push(atoms, Value::Address(Label::Block(target), 0), version);
    atoms.push(Atom::Bytes(vec![opcode]));
}

fn lower(
    module: &ir::Module,
    version: EvmVersion,
    table_width: usize,
) -> std::result::Result<Vec<Atom>, String> {
    let mut targets = DenseBitSet::new_empty(module.blocks.len());
    let order: Vec<_> = module.block_ids().collect();
    for (position, &id) in order.iter().enumerate() {
        let next = order.get(position + 1).copied();
        let block = &module.blocks[id];
        for inst in &block.insts {
            if let InstKind::PushLabel(target) = inst.kind {
                targets.insert(target);
            }
        }
        match &block.terminator.kind {
            TerminatorKind::Jump(target) => {
                if Some(*target) != next {
                    targets.insert(*target);
                }
            }
            TerminatorKind::JumpI(a, b) => {
                targets.insert(*a);
                if Some(*b) != next {
                    targets.insert(*b);
                }
            }
            TerminatorKind::IndexedJump(cases) => {
                for &target in cases {
                    targets.insert(target);
                }
            }
            _ => {}
        }
    }
    let mut atoms = Vec::new();
    for (position, &id) in order.iter().enumerate() {
        let block = &module.blocks[id];
        // <block label>:
        // jumpdest (only for addressable targets)
        atoms.push(Atom::Label(Label::Block(id)));
        if targets.contains(id) {
            atoms.push(Atom::Bytes(vec![0x5b]));
        }
        for inst in &block.insts {
            match &inst.kind {
                // opcode
                InstKind::Op(opcode) => atoms.push(Atom::Bytes(vec![*opcode])),
                // push <literal>
                InstKind::Push(value) => push(&mut atoms, Value::Literal(*value), version),
                // push <block address>
                InstKind::PushLabel(target) => {
                    push(&mut atoms, Value::Address(Label::Block(*target), 0), version)
                }
                // push <data address + offset>
                InstKind::PushData { id, offset } => {
                    push(&mut atoms, Value::Address(Label::Data(*id), *offset), version)
                }
                // push <resolved deferred value>
                InstKind::PushDeferred(id) => {
                    let value = if module.program_size_id == Some(*id) {
                        Value::ProgramEnd
                    } else if module.appendix_start_id == Some(*id) {
                        Value::AppendixStart
                    } else {
                        Value::Literal(*module.deferred.get(id).ok_or_else(|| {
                            "cannot assemble unresolved `push_deferred` instruction".to_string()
                        })?)
                    };
                    push(&mut atoms, value, version);
                }
                // push<width> <immutable placeholder>
                InstKind::PushImmutable { id, width } => {
                    if !(1..=32).contains(width) {
                        return Err("invalid immutable PUSH width".into());
                    }
                    atoms.push(Atom::Immutable { id: *id, width: *width });
                }
                // dup <depth>
                InstKind::Dup(depth) => atoms.push(Atom::Bytes(stack_op(version, *depth, false)?)),
                // swap <depth>
                InstKind::Swap(depth) => atoms.push(Atom::Bytes(stack_op(version, *depth, true)?)),
                InstKind::Exchange(a, b) => {
                    if version.has_extended_stack_ops() {
                        let immediate = op::encode_exchange(*a, *b)
                            .ok_or_else(|| "invalid EVM exchange indices".to_string())?;
                        // exchange a, b
                        atoms.push(Atom::Bytes(vec![op::EXCHANGE, immediate]));
                    } else {
                        // swap a; swap b; swap a
                        atoms.push(Atom::Bytes(stack_op(version, *a, true)?));
                        atoms.push(Atom::Bytes(stack_op(version, *b, true)?));
                        atoms.push(Atom::Bytes(stack_op(version, *a, true)?));
                    }
                }
            }
        }
        let next = order.get(position + 1).copied();
        match &block.terminator.kind {
            // jump <target> (omit an immediate fallthrough)
            TerminatorKind::Jump(target) => {
                if Some(*target) != next {
                    jump(&mut atoms, *target, 0x56, version);
                }
            }
            // push <true>; jumpi
            // push <false>; jump (unless fallthrough)
            TerminatorKind::JumpI(a, b) => {
                jump(&mut atoms, *a, 0x57, version);
                if Some(*b) != next {
                    jump(&mut atoms, *b, 0x56, version);
                }
            }
            // jump <stack address>
            TerminatorKind::DynamicJump => atoms.push(Atom::Bytes(vec![0x56])),
            TerminatorKind::IndexedJump(targets) => {
                if targets.is_empty() {
                    return Err("indexed EVM jump has no targets".into());
                }
                if table_width == 1 {
                    // <index>; push <32 - target count>; add
                    // push <packed byte addresses>; swap1; byte; jump
                    push(&mut atoms, Value::Literal(U256::from(32 - targets.len())), version);
                    atoms.push(Atom::Bytes(vec![op::ADD]));
                    push(&mut atoms, Value::PackedTargets(targets.clone(), table_width), version);
                    atoms.push(Atom::Bytes(vec![op::SWAP1, op::BYTE, op::JUMP]));
                    continue;
                }
                let bits = table_width * 8;
                let (scale, opcode) = if version.has_bitwise_shifting() && bits.is_power_of_two() {
                    (bits.ilog2() as usize, op::SHL)
                } else {
                    (bits, op::MUL)
                };
                // <index>; push <log2(entry bits)>; shl
                // or: <index>; push <entry bits>; mul
                push(&mut atoms, Value::Literal(U256::from(scale)), version);
                atoms.push(Atom::Bytes(vec![opcode]));
                if version.has_bitwise_shifting() {
                    // push <addresses with first target in low bits>; swap1; shr
                    push(
                        &mut atoms,
                        Value::PackedTargets(targets.iter().rev().copied().collect(), table_width),
                        version,
                    );
                    atoms.push(Atom::Bytes(vec![op::SWAP1, op::SHR]));
                } else {
                    // Keep the original exponent at each index: EXP charges
                    // differently for zero, so reversal could increase gas.
                    // push <highest entry shift>; sub
                    // push 2; exp; push <addresses with first target in high bits>; div
                    push(
                        &mut atoms,
                        Value::Literal(U256::from((targets.len() - 1) * bits)),
                        version,
                    );
                    atoms.push(Atom::Bytes(vec![op::SUB]));
                    push(&mut atoms, Value::Literal(U256::from(2)), version);
                    atoms.push(Atom::Bytes(vec![op::EXP]));
                    push(&mut atoms, Value::PackedTargets(targets.clone(), table_width), version);
                    atoms.push(Atom::Bytes(vec![op::DIV]));
                }
                // push <address mask>; and; jump
                push(&mut atoms, Value::Literal((U256::ONE << bits) - U256::ONE), version);
                atoms.push(Atom::Bytes(vec![op::AND, op::JUMP]));
            }
            // Execution past the physical program ends with an implicit STOP.
            TerminatorKind::Stop
                if next.is_none() && module.data.is_empty() && module.appendix.is_empty() => {}
            // stop / return / revert / invalid / selfdestruct
            kind => atoms.push(Atom::Bytes(vec![match kind {
                TerminatorKind::Stop => 0x00,
                TerminatorKind::Return => 0xf3,
                TerminatorKind::Revert => 0xfd,
                TerminatorKind::SelfDestruct => 0xff,
                TerminatorKind::Invalid | TerminatorKind::Unreachable => 0xfe,
                _ => unreachable!(),
            }])),
        }
    }
    for (id, data) in module.data.iter_enumerated() {
        // <data label>:
        // <opaque bytes>
        atoms.push(Atom::Label(Label::Data(id)));
        atoms.push(Atom::Bytes(data.bytes.clone()));
    }
    // <opaque appended runtime bytes>
    atoms.push(Atom::Bytes(module.appendix.clone()));
    Ok(atoms)
}

fn stack_op(version: EvmVersion, depth: u16, swap: bool) -> std::result::Result<Vec<u8>, String> {
    if (1..=16).contains(&depth) {
        return Ok(vec![(if swap { 0x8f } else { 0x7f }) + depth as u8]);
    }
    if version.has_extended_stack_ops()
        && let Some(immediate) = op::encode_depth(depth)
    {
        return Ok(vec![if swap { op::SWAPN } else { op::DUPN }, immediate]);
    }
    Err("EVM stack access exceeds the target's supported depth".into())
}

fn resolve(
    mut atoms: Vec<Atom>,
    module: &ir::Module,
    version: EvmVersion,
) -> std::result::Result<Encoded, String> {
    let mut blocks = IndexVec::<BlockId, usize>::from_vec(vec![0; module.blocks.len()]);
    let mut data = IndexVec::<DataId, usize>::from_vec(vec![0; module.data.len()]);
    let value_of = |value: &Value,
                    blocks: &IndexVec<BlockId, usize>,
                    data: &IndexVec<DataId, usize>,
                    program_size: usize|
     -> std::result::Result<U256, String> {
        match value {
            Value::Literal(value) => Ok(*value),
            Value::ProgramEnd => Ok(U256::from(program_size)),
            Value::AppendixStart => Ok(U256::from(program_size - module.appendix.len())),
            Value::PackedTargets(targets, width) => {
                let mut packed = U256::ZERO;
                for &id in targets {
                    let address = *blocks
                        .get(id)
                        .ok_or_else(|| "unallocated indexed EVM target".to_string())?;
                    if U256::from(address).bit_len().div_ceil(8) > *width {
                        return Err("indexed target exceeds selected address width".into());
                    }
                    packed = (packed << (width * 8)) | U256::from(address);
                }
                Ok(packed)
            }
            Value::Address(label, offset) => {
                let base = match label {
                    Label::Block(id) => blocks.get(*id),
                    Label::Data(id) => data.get(*id),
                }
                .ok_or_else(|| "unallocated EVM relocation target".to_string())?;
                let address = base
                    .checked_add(*offset as usize)
                    .ok_or_else(|| "EVM relocation overflow".to_string())?;
                Ok(U256::from(address))
            }
        }
    };
    let mut program_size;
    loop {
        let mut offset = 0usize;
        for atom in &atoms {
            let size = match atom {
                Atom::Label(label) => {
                    match label {
                        Label::Block(id) => blocks[*id] = offset,
                        Label::Data(id) => data[*id] = offset,
                    }
                    0
                }
                Atom::Bytes(bytes) => bytes.len(),
                Atom::Push { width, .. } => 1 + width,
                Atom::Immutable { width, .. } => 1 + usize::from(*width),
            };
            offset =
                offset.checked_add(size).ok_or_else(|| "EVM bytecode size overflow".to_string())?;
        }
        program_size = offset;
        let mut changed = false;
        for atom in &mut atoms {
            if let Atom::Push { value, width } = atom {
                let required =
                    op::push_len(version, value_of(value, &blocks, &data, program_size)?) - 1;
                if required > *width {
                    *width = required;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let mut output = Encoded { bytes: Vec::new(), immutable_references: Vec::new() };
    for atom in atoms {
        match atom {
            Atom::Label(_) => {}
            Atom::Bytes(bytes) => output.bytes.extend(bytes),
            Atom::Push { value, width } => {
                let bytes = value_of(&value, &blocks, &data, program_size)?.to_be_bytes::<32>();
                output.bytes.push(0x5f + width as u8);
                output.bytes.extend_from_slice(&bytes[32 - width..]);
            }
            Atom::Immutable { id, width } => {
                output.immutable_references.push(ImmutableReference {
                    id,
                    code_offset: output.bytes.len(),
                    type_size: TypeSize::new_int_bits(u16::from(width) * 8),
                });
                output.bytes.push(0x5f + width);
                output.bytes.resize(output.bytes.len() + usize::from(width), 0);
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::evm::disassemble;

    #[test]
    fn interacting_forward_pushes_reach_fixed_point() {
        let mut module = ir::Module::default();
        let a = module.blocks.push(ir::Block::default());
        let b = module.blocks.push(ir::Block::default());
        let output = resolve(
            vec![
                Atom::Push { value: Value::Address(Label::Block(a), 0), width: 0 },
                Atom::Push { value: Value::Address(Label::Block(b), 0), width: 0 },
                Atom::Bytes(vec![op::STOP; 251]),
                Atom::Label(Label::Block(a)),
                Atom::Bytes(vec![op::STOP; 256]),
                Atom::Label(Label::Block(b)),
            ],
            &module,
            EvmVersion::Osaka,
        )
        .unwrap();
        let text = disassemble(&output.bytes, EvmVersion::Osaka)
            .lines()
            .take(2)
            .collect::<Vec<_>>()
            .join("\n");
        snapbox::assert_data_eq!(
            text,
            snapbox::str![[r#"
PUSH2 0x0101
PUSH2 0x0201
"#]]
        );
    }

    #[test]
    fn indexed_scaling_respects_slot_width_and_fork() {
        let mut module = ir::Module::default();
        let entry = module.blocks.push(ir::Block::default());
        let target = module.blocks.push(ir::Block::default());
        // push 0; indexed_jump <target>
        module.blocks[entry].insts.push(InstKind::Push(U256::ZERO).into());
        module.blocks[entry].terminator = TerminatorKind::IndexedJump(vec![target]).into();
        let mut text = String::new();
        for (version, width) in [
            (EvmVersion::Osaka, 2),
            (EvmVersion::Osaka, 3),
            (EvmVersion::Osaka, 4),
            (EvmVersion::Byzantium, 2),
        ] {
            let output =
                resolve(lower(&module, version, width).unwrap(), &module, version).unwrap();
            let instructions = disassemble(&output.bytes, version);
            text.push_str(&instructions.lines().take(3).collect::<Vec<_>>().join("\n"));
            text.push('\n');
        }
        snapbox::assert_data_eq!(
            text,
            snapbox::str![[r#"
PUSH0
PUSH1 0x04
SHL
PUSH0
PUSH1 0x18
MUL
PUSH0
PUSH1 0x05
SHL
PUSH1 0x00
PUSH1 0x10
MUL

"#]]
        );
    }

    #[test]
    fn low_first_targets_resolve_after_push_growth() {
        let mut module = ir::Module::default();
        let a = module.blocks.push(ir::Block::default());
        let b = module.blocks.push(ir::Block::default());
        let output = resolve(
            vec![
                Atom::Push { value: Value::PackedTargets(vec![b, a], 2), width: 0 },
                Atom::Bytes(vec![op::STOP; 256]),
                Atom::Label(Label::Block(a)),
                Atom::Bytes(vec![op::STOP]),
                Atom::Label(Label::Block(b)),
            ],
            &module,
            EvmVersion::Osaka,
        )
        .unwrap();
        let text = disassemble(&output.bytes, EvmVersion::Osaka).lines().next().unwrap().to_owned();
        snapbox::assert_data_eq!(text, snapbox::str![["PUSH4 0x01060105"]]);
    }

    #[test]
    fn program_end_and_fixed_width_placeholders() {
        let module = ir::Module::default();
        let output = resolve(
            vec![
                Atom::Push { value: Value::ProgramEnd, width: 0 },
                Atom::Immutable { id: ImmutableId::new(0), width: 2 },
            ],
            &module,
            EvmVersion::Osaka,
        )
        .unwrap();
        snapbox::assert_data_eq!(
            disassemble(&output.bytes, EvmVersion::Osaka),
            snapbox::str![[r#"
PUSH1 0x05
PUSH2 0x0000

"#]]
        );
    }
}
