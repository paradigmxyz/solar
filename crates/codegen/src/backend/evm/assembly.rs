//! Primitive physical-IR encoding and fixed-point relocation.
//!
//! Block control transfers are lowered once into a byte buffer with separate
//! labels and deferred PUSH records. Ordinary instructions and literal pushes
//! are encoded immediately; only deferred pushes participate in relocation. It performs no
//! control-flow or instruction optimization. PUSH widths start at their minimum and grow
//! monotonically until all addresses fit; every iteration recomputes label offsets, including
//! embedded data addresses. Labels and relocations are ordered by buffer position, so each
//! iteration is linear in their count and never scans ordinary instructions or data bytes.
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

/// Unresolved pushes refer to positions in the byte buffer, each occupying one
/// placeholder byte until emission. Literal instructions are already encoded.
struct Relocation {
    offset: usize,
    value: Value,
    width: usize,
}

/// The assembler stores ordinary bytes once and only revisits placement records.
#[derive(Default)]
struct Assembly {
    bytes: Vec<u8>,
    labels: Vec<(Label, usize)>,
    relocations: Vec<Relocation>,
    immutable_references: Vec<ImmutableReference>,
}

impl Assembly {
    fn label(&mut self, label: Label) {
        self.labels.push((label, self.bytes.len()));
    }

    fn immutable(&mut self, id: ImmutableId, width: u8) {
        self.immutable_references.push(ImmutableReference {
            id,
            code_offset: self.bytes.len(),
            type_size: TypeSize::new_int_bits(u16::from(width) * 8),
        });
        // push<width> <zero placeholder>
        self.bytes.push(0x5f + width);
        self.bytes.resize(self.bytes.len() + usize::from(width), 0);
    }
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
        let assembly =
            lower(module, version, width).map_err(|message| gcx.dcx().err(message).emit())?;
        match resolve(assembly, module, version) {
            Ok(encoded) => return Ok(encoded),
            Err(message) if message == "indexed target exceeds selected address width" => continue,
            Err(message) => return Err(gcx.dcx().err(message).emit()),
        }
    }
    Err(gcx.dcx().err("indexed EVM target address exceeds four bytes").emit())
}

fn push(assembly: &mut Assembly, value: Value, version: EvmVersion) {
    if let Value::Literal(value) = value {
        let width = op::push_len(version, value) - 1;
        // push <literal>
        assembly.bytes.push(0x5f + width as u8);
        assembly.bytes.extend_from_slice(&value.to_be_bytes::<32>()[32 - width..]);
    } else {
        assembly.relocations.push(Relocation {
            offset: assembly.bytes.len(),
            value,
            width: usize::from(!version.has_push0()),
        });
        // push <unresolved value>
        assembly.bytes.push(0x5f);
    }
}

fn jump(assembly: &mut Assembly, target: BlockId, opcode: u8, version: EvmVersion) {
    // push <target>
    // jump / jumpi
    push(assembly, Value::Address(Label::Block(target), 0), version);
    assembly.bytes.extend_from_slice(&[opcode]);
}

fn lower(
    module: &ir::Module,
    version: EvmVersion,
    table_width: usize,
) -> std::result::Result<Assembly, String> {
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
    let mut assembly = Assembly::default();
    for (position, &id) in order.iter().enumerate() {
        let block = &module.blocks[id];
        // <block label>:
        // jumpdest (only for addressable targets)
        assembly.label(Label::Block(id));
        if targets.contains(id) {
            assembly.bytes.extend_from_slice(&[0x5b]);
        }
        for inst in &block.insts {
            match &inst.kind {
                // opcode
                InstKind::Op(opcode) => assembly.bytes.extend_from_slice(&[*opcode]),
                // push <literal>
                InstKind::Push(value) => push(&mut assembly, Value::Literal(*value), version),
                // push <block address>
                InstKind::PushLabel(target) => {
                    push(&mut assembly, Value::Address(Label::Block(*target), 0), version)
                }
                // push <data address + offset>
                InstKind::PushData { id, offset } => {
                    push(&mut assembly, Value::Address(Label::Data(*id), *offset), version)
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
                    push(&mut assembly, value, version);
                }
                // push<width> <immutable placeholder>
                InstKind::PushImmutable { id, width } => {
                    if !(1..=32).contains(width) {
                        return Err("invalid immutable PUSH width".into());
                    }
                    assembly.immutable(*id, *width);
                }
                // dup <depth>
                InstKind::Dup(depth) => stack_op(&mut assembly.bytes, version, *depth, false)?,
                // swap <depth>
                InstKind::Swap(depth) => stack_op(&mut assembly.bytes, version, *depth, true)?,
                InstKind::Exchange(a, b) => {
                    if version.has_extended_stack_ops() {
                        let immediate = op::encode_exchange(*a, *b)
                            .ok_or_else(|| "invalid EVM exchange indices".to_string())?;
                        // exchange a, b
                        assembly.bytes.extend_from_slice(&[op::EXCHANGE, immediate]);
                    } else {
                        // swap a; swap b; swap a
                        stack_op(&mut assembly.bytes, version, *a, true)?;
                        stack_op(&mut assembly.bytes, version, *b, true)?;
                        stack_op(&mut assembly.bytes, version, *a, true)?;
                    }
                }
            }
        }
        let next = order.get(position + 1).copied();
        match &block.terminator.kind {
            // jump <target> (omit an immediate fallthrough)
            TerminatorKind::Jump(target) => {
                if Some(*target) != next {
                    jump(&mut assembly, *target, 0x56, version);
                }
            }
            // push <true>; jumpi
            // push <false>; jump (unless fallthrough)
            TerminatorKind::JumpI(a, b) => {
                jump(&mut assembly, *a, 0x57, version);
                if Some(*b) != next {
                    jump(&mut assembly, *b, 0x56, version);
                }
            }
            // jump <stack address>
            TerminatorKind::DynamicJump => assembly.bytes.extend_from_slice(&[0x56]),
            TerminatorKind::IndexedJump(targets) => {
                if targets.is_empty() {
                    return Err("indexed EVM jump has no targets".into());
                }
                if table_width == 1 {
                    // <index>; push <32 - target count>; add
                    // push <packed byte addresses>; swap1; byte; jump
                    push(&mut assembly, Value::Literal(U256::from(32 - targets.len())), version);
                    assembly.bytes.extend_from_slice(&[op::ADD]);
                    push(
                        &mut assembly,
                        Value::PackedTargets(targets.clone(), table_width),
                        version,
                    );
                    assembly.bytes.extend_from_slice(&[op::SWAP1, op::BYTE, op::JUMP]);
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
                push(&mut assembly, Value::Literal(U256::from(scale)), version);
                assembly.bytes.extend_from_slice(&[opcode]);
                if version.has_bitwise_shifting() {
                    // push <addresses with first target in low bits>; swap1; shr
                    push(
                        &mut assembly,
                        Value::PackedTargets(targets.iter().rev().copied().collect(), table_width),
                        version,
                    );
                    assembly.bytes.extend_from_slice(&[op::SWAP1, op::SHR]);
                } else {
                    // Keep the original exponent at each index: EXP charges
                    // differently for zero, so reversal could increase gas.
                    // push <highest entry shift>; sub
                    // push 2; exp; push <addresses with first target in high bits>; div
                    push(
                        &mut assembly,
                        Value::Literal(U256::from((targets.len() - 1) * bits)),
                        version,
                    );
                    assembly.bytes.extend_from_slice(&[op::SUB]);
                    push(&mut assembly, Value::Literal(U256::from(2)), version);
                    assembly.bytes.extend_from_slice(&[op::EXP]);
                    push(
                        &mut assembly,
                        Value::PackedTargets(targets.clone(), table_width),
                        version,
                    );
                    assembly.bytes.extend_from_slice(&[op::DIV]);
                }
                // push <address mask>; and; jump
                push(&mut assembly, Value::Literal((U256::ONE << bits) - U256::ONE), version);
                assembly.bytes.extend_from_slice(&[op::AND, op::JUMP]);
            }
            // Execution past the physical program ends with an implicit STOP.
            TerminatorKind::Stop
                if next.is_none() && module.data.is_empty() && module.appendix.is_empty() => {}
            // stop / return / revert / invalid / selfdestruct
            kind => assembly.bytes.extend_from_slice(&[match kind {
                TerminatorKind::Stop => 0x00,
                TerminatorKind::Return => 0xf3,
                TerminatorKind::Revert => 0xfd,
                TerminatorKind::SelfDestruct => 0xff,
                TerminatorKind::Invalid | TerminatorKind::Unreachable => 0xfe,
                _ => unreachable!(),
            }]),
        }
    }
    for (id, data) in module.data.iter_enumerated() {
        // <data label>:
        // <opaque bytes>
        assembly.label(Label::Data(id));
        assembly.bytes.extend_from_slice(&data.bytes);
    }
    // <opaque appended runtime bytes>
    assembly.bytes.extend_from_slice(&module.appendix);
    Ok(assembly)
}

fn stack_op(
    bytes: &mut Vec<u8>,
    version: EvmVersion,
    depth: u16,
    swap: bool,
) -> std::result::Result<(), String> {
    if (1..=16).contains(&depth) {
        // dup<depth> / swap<depth>
        bytes.push((if swap { 0x8f } else { 0x7f }) + depth as u8);
        return Ok(());
    }
    if version.has_extended_stack_ops()
        && let Some(immediate) = op::encode_depth(depth)
    {
        // dupn / swapn <depth immediate>
        bytes.extend_from_slice(&[if swap { op::SWAPN } else { op::DUPN }, immediate]);
        return Ok(());
    }
    Err("EVM stack access exceeds the target's supported depth".into())
}

fn resolve(
    mut assembly: Assembly,
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
        let mut relocations = assembly.relocations.iter().peekable();
        let mut growth = 0usize;
        for &(label, offset) in &assembly.labels {
            // A placeholder at this position follows the label; only earlier
            // pushes contribute immediate bytes to the label's final offset.
            while let Some(relocation) = relocations.next_if(|r| r.offset < offset) {
                growth = growth
                    .checked_add(relocation.width)
                    .ok_or_else(|| "EVM bytecode size overflow".to_string())?;
            }
            let address = offset
                .checked_add(growth)
                .ok_or_else(|| "EVM bytecode size overflow".to_string())?;
            match label {
                Label::Block(id) => blocks[id] = address,
                Label::Data(id) => data[id] = address,
            }
        }
        for relocation in relocations {
            growth = growth
                .checked_add(relocation.width)
                .ok_or_else(|| "EVM bytecode size overflow".to_string())?;
        }
        program_size = assembly
            .bytes
            .len()
            .checked_add(growth)
            .ok_or_else(|| "EVM bytecode size overflow".to_string())?;
        let mut changed = false;
        for relocation in &mut assembly.relocations {
            let required =
                op::push_len(version, value_of(&relocation.value, &blocks, &data, program_size)?)
                    - 1;
            if required > relocation.width {
                relocation.width = required;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut relocations = assembly.relocations.iter().peekable();
    let mut growth = 0;
    for reference in &mut assembly.immutable_references {
        while let Some(relocation) = relocations.next_if(|r| r.offset < reference.code_offset) {
            growth += relocation.width;
        }
        reference.code_offset += growth;
    }
    if assembly.relocations.is_empty() {
        return Ok(Encoded {
            bytes: assembly.bytes,
            immutable_references: assembly.immutable_references,
        });
    }
    let mut bytes = Vec::with_capacity(program_size);
    let mut copied = 0;
    for relocation in assembly.relocations {
        // <fixed bytes>; push<resolved width> <resolved value>
        bytes.extend_from_slice(&assembly.bytes[copied..relocation.offset]);
        bytes.push(0x5f + relocation.width as u8);
        let value = value_of(&relocation.value, &blocks, &data, program_size)?.to_be_bytes::<32>();
        bytes.extend_from_slice(&value[32 - relocation.width..]);
        copied = relocation.offset + 1;
    }
    bytes.extend_from_slice(&assembly.bytes[copied..]);
    Ok(Encoded { bytes, immutable_references: assembly.immutable_references })
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
        let mut assembly = Assembly::default();
        push(&mut assembly, Value::Address(Label::Block(a), 0), EvmVersion::Osaka);
        push(&mut assembly, Value::Address(Label::Block(b), 0), EvmVersion::Osaka);
        assembly.bytes.extend_from_slice(&[op::STOP; 251]);
        assembly.label(Label::Block(a));
        assembly.bytes.extend_from_slice(&[op::STOP; 256]);
        assembly.label(Label::Block(b));
        let output = resolve(assembly, &module, EvmVersion::Osaka).unwrap();
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
        let mut assembly = Assembly::default();
        push(&mut assembly, Value::PackedTargets(vec![b, a], 2), EvmVersion::Osaka);
        assembly.bytes.extend_from_slice(&[op::STOP; 256]);
        assembly.label(Label::Block(a));
        assembly.bytes.push(op::STOP);
        assembly.label(Label::Block(b));
        let output = resolve(assembly, &module, EvmVersion::Osaka).unwrap();
        let text = disassemble(&output.bytes, EvmVersion::Osaka).lines().next().unwrap().to_owned();
        snapbox::assert_data_eq!(text, snapbox::str![["PUSH4 0x01060105"]]);
    }

    #[test]
    fn program_end_and_fixed_width_placeholders() {
        let module = ir::Module::default();
        let mut assembly = Assembly::default();
        push(&mut assembly, Value::ProgramEnd, EvmVersion::Osaka);
        assembly.immutable(ImmutableId::new(0), 2);
        let output = resolve(assembly, &module, EvmVersion::Osaka).unwrap();
        assert_eq!(output.immutable_references[0].code_offset, 2);
        snapbox::assert_data_eq!(
            disassemble(&output.bytes, EvmVersion::Osaka),
            snapbox::str![[r#"
PUSH1 0x05
PUSH2 0x0000

"#]]
        );
    }

    #[test]
    fn labels_at_placeholder_positions_do_not_include_its_growth() {
        let mut module = ir::Module::default();
        let a = module.blocks.push(ir::Block::default());
        let b = module.blocks.push(ir::Block::default());
        let c = module.blocks.push(ir::Block::default());
        let mut text = String::new();
        for version in [EvmVersion::Osaka, EvmVersion::Byzantium] {
            let mut assembly = Assembly::default();
            // a: b: push a; push b; c: push c
            assembly.label(Label::Block(a));
            assembly.label(Label::Block(b));
            push(&mut assembly, Value::Address(Label::Block(a), 0), version);
            push(&mut assembly, Value::Address(Label::Block(b), 0), version);
            assembly.label(Label::Block(c));
            push(&mut assembly, Value::Address(Label::Block(c), 0), version);
            let output = resolve(assembly, &module, version).unwrap();
            text.push_str(&disassemble(&output.bytes, version));
        }
        snapbox::assert_data_eq!(
            text,
            snapbox::str![[r#"
PUSH0
PUSH0
PUSH1 0x02
PUSH1 0x00
PUSH1 0x00
PUSH1 0x04

"#]]
        );
    }

    #[test]
    fn immutable_sites_follow_only_preceding_push_growth() {
        let mut module = ir::Module::default();
        let target = module.blocks.push(ir::Block::default());
        let version = EvmVersion::Osaka;
        let mut assembly = Assembly::default();
        // immutable0; push target; immutable1; push program_end
        // <248 STOPs>; target: immutable2
        assembly.immutable(ImmutableId::new(0), 1);
        push(&mut assembly, Value::Address(Label::Block(target), 0), version);
        assembly.immutable(ImmutableId::new(1), 2);
        push(&mut assembly, Value::ProgramEnd, version);
        assembly.bytes.extend_from_slice(&[op::STOP; 248]);
        assembly.label(Label::Block(target));
        assembly.immutable(ImmutableId::new(2), 1);
        let output = resolve(assembly, &module, version).unwrap();
        let mut text =
            disassemble(&output.bytes, version).lines().take(4).collect::<Vec<_>>().join("\n");
        for reference in &output.immutable_references {
            text.push_str(&format!(
                "\npatch at {}: {}",
                reference.code_offset,
                disassemble(&output.bytes[reference.code_offset..], version)
                    .lines()
                    .next()
                    .unwrap(),
            ));
        }
        snapbox::assert_data_eq!(
            text,
            snapbox::str![[r#"
PUSH1 0x00
PUSH2 0x0103
PUSH2 0x0000
PUSH2 0x0105
patch at 0: PUSH1 0x00
patch at 5: PUSH2 0x0000
patch at 259: PUSH1 0x00
"#]]
        );
    }

    #[test]
    fn fixed_byte_fast_path_preserves_data_and_immutable_sites() {
        let mut module = ir::Module::default();
        // push 0; push 0x1234; immutable0; stop; <data ADD>; <appendix MUL>
        module.blocks.push(ir::Block {
            insts: vec![
                InstKind::Push(U256::ZERO).into(),
                InstKind::Push(U256::from(0x1234)).into(),
                InstKind::PushImmutable { id: ImmutableId::new(0), width: 2 }.into(),
            ],
            terminator: TerminatorKind::Stop.into(),
            ..Default::default()
        });
        module.data.push(ir::Data { bytes: vec![op::ADD], ..Default::default() });
        module.appendix.push(op::MUL);
        let version = EvmVersion::Osaka;
        let assembly = lower(&module, version, 1).unwrap();
        assert!(assembly.relocations.is_empty());
        let output = resolve(assembly, &module, version).unwrap();
        assert_eq!(output.immutable_references[0].code_offset, 4);
        snapbox::assert_data_eq!(
            disassemble(&output.bytes, version),
            snapbox::str![[r#"
PUSH0
PUSH2 0x1234
PUSH2 0x0000
STOP
ADD
MUL

"#]]
        );
    }

    #[test]
    fn indexed_retry_restarts_after_ordinary_push_growth() {
        let mut module = ir::Module::default();
        let target = module.blocks.push(ir::Block::default());
        let version = EvmVersion::Osaka;
        let make_assembly = |width| {
            let mut assembly = Assembly::default();
            // push packed(target); push target; <252 STOPs>; target: immutable0
            push(&mut assembly, Value::PackedTargets(vec![target], width), version);
            push(&mut assembly, Value::Address(Label::Block(target), 0), version);
            assembly.bytes.extend_from_slice(&[op::STOP; 252]);
            assembly.label(Label::Block(target));
            assembly.immutable(ImmutableId::new(0), 1);
            assembly
        };
        let error = resolve(make_assembly(1), &module, version).err().unwrap();
        snapbox::assert_data_eq!(
            error,
            snapbox::str![["indexed target exceeds selected address width"]]
        );
        let output = resolve(make_assembly(2), &module, version).unwrap();
        assert_eq!(output.immutable_references.len(), 1);
        assert_eq!(output.immutable_references[0].code_offset, 258);
        let text =
            disassemble(&output.bytes, version).lines().take(2).collect::<Vec<_>>().join("\n");
        snapbox::assert_data_eq!(
            text,
            snapbox::str![[r#"
PUSH2 0x0102
PUSH2 0x0102
"#]]
        );
    }
}
