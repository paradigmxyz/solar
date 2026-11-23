//! Independent structural EVM import, using compiler instructions and terminators.
//!
//! Export reachable program points without executing them or importing a Python
//! recipe. Static label pushes are consumed only by their actual adjacent branch.
//! The continuation export retains other block-label pushes and their targets
//! for stack provenance checking. Immutable pushes can be exported as symbolic
//! deployment slots in an entry trace. Recognized code reads carry an explicit
//! stack guard and the declared data suffix. A stack-carried JUMPI is split at
//! the address pop so the checker can track both architectural operands.

use serde::Deserialize;
use serde_json::{Value, json};
use solar_config::EvmVersion;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec};
use solar_evmir::{
    ir::{BlockId, Instruction, Module, TerminatorKind, verify::Verifier},
    op::{self, StackOp},
};
use solar_interface::{ColorChoice, Session, source_map::FileName};
use std::{
    collections::BTreeMap,
    io::{self, Read},
    str::FromStr,
};

mod relocations;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    version: u32,
    text: String,
    // An omitted entry selects the first block in physical order.
    entry: Option<u32>,
    evm_version: String,
    #[serde(default)]
    window: bool,
    #[serde(default)]
    labels: bool,
}

pub(super) fn export() -> Result<Value, String> {
    let mut bytes = Vec::new();
    io::stdin().lock().take(2_097_153).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() > 2_097_152 {
        return Err("EVM model request exceeds 2 MiB".into());
    }
    let input: Input = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if input.version != 1 || input.text.len() > 1_048_576 {
        return Err("invalid EVM model request".into());
    }
    let version = EvmVersion::from_str(&input.evm_version).map_err(|e| e.to_string())?;
    let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
    sess.enter_sequential(|| {
        let file = sess
            .source_map()
            .new_source_file(FileName::Custom("model.evmir".into()), input.text)
            .map_err(|e| e.to_string())?;
        let module = Module::parse(&sess, &file).map_err(|_| "cannot parse EVM model")?;
        Verifier::for_evm_version(&sess.dcx, version).verify_module(&module);
        if sess.dcx.has_errors().is_err() {
            return Err("invalid EVM model".into());
        }
        let (entry, _) = module
            .blocks
            .iter_enumerated()
            .find(|(_, b)| input.entry.is_none_or(|label| b.label == label))
            .ok_or("missing EVM model entry")?;
        if input.window {
            window(&module, entry)
        } else {
            graph(&module, entry, input.labels, input.entry.is_none())
        }
    })
}

fn graph(module: &Module, entry: BlockId, labels: bool, from_entry: bool) -> Result<Value, String> {
    let mut bases = IndexVec::<BlockId, _>::new();
    let mut total = 0;
    for block in &module.blocks {
        bases.push(total);
        total += block.instructions.len() + 1;
    }
    if total > 65536 {
        return Err("EVM model point budget exceeded".into());
    }
    let limit = if from_entry { 8192 } else { 4096 };
    let mut visited = DenseBitSet::new_empty(total);
    let mut pending = vec![(entry, 0)];
    let mut nodes = Vec::new();
    let suffix = (labels && from_entry).then(|| relocations::suffix(module)).flatten();
    let mut reads = BTreeMap::new();
    let mut read_code = false;
    while let Some((id, offset)) = pending.pop() {
        let block = &module.blocks[id];
        if offset > block.instructions.len() {
            return Err("unsupported physical branch continuation".into());
        }
        let point = bases[id] + offset;
        if !visited.insert(point) {
            continue;
        }
        if nodes.len() >= limit {
            return Err(format!("reachable EVM model exceeds {limit} nodes"));
        }
        let mut edge = |target, position| {
            pending.push((target, position));
            bases[target] + position
        };
        let node = if let Some((name, end)) = reads.remove(&point) {
            json!({"kind":"operation","operation":name,"next":edge(id,end)})
        } else if let Some(read) = suffix
            .and_then(|suffix| relocations::recognize(&block.instructions[offset..], suffix.len()))
        {
            // A read spans only ordinary instructions in this block. The second
            // point represents its effect, after admitting its scratch stack.
            reads.insert(point + 1, (read.name, offset + read.count));
            read_code = true;
            json!({"kind":"jump","growth":read.peak,"next":edge(id,offset+1)})
        } else if let Some(inst) = block.instructions.get(offset) {
            if let Some(target) = inst.pushed_block() {
                let next = block.instructions.get(offset + 1).map(|i| i.opcode).or_else(|| {
                    match block.terminator.as_ref()?.kind {
                        TerminatorKind::Op(opcode) => Some(opcode),
                        _ => None,
                    }
                });
                match next {
                    Some(op::JUMP) => json!({"kind":"jump","growth":1,"next":edge(target,0)}),
                    Some(op::JUMPI) => {
                        json!({"kind":"branch","yes":edge(target,0),"no":edge(id,offset+2)})
                    }
                    _ if labels => {
                        json!({"kind":"push_label","target":edge(target,0),"next":edge(id,offset+1)})
                    }
                    _ => return Err("escaping label in EVM source graph".into()),
                }
            } else if labels && inst.as_evm_opcode() == Some(op::JUMPI) {
                if nodes.len() + 1 >= limit {
                    return Err(format!("reachable EVM model exceeds {limit} nodes"));
                }
                // Synthetic points have no physical entry or label relocation.
                let branch = total + point;
                nodes.push(json!({
                    "id":branch,"label":block.label,
                    "node":{"kind":"branch_after_address","no":edge(id,offset+1)}
                }));
                json!({"kind":"branch_address","next":branch})
            } else if labels
                && let Some(immutable) = inst.immutable_push()
                && let Some(size) = inst.immutable_type_size()
            {
                let operation = format!("push_immutable {}, {}", immutable.index(), size.bytes());
                json!({"kind":"operation","operation":operation,"next":edge(id,offset+1)})
            } else {
                json!({"kind":"operation","operation":instruction(inst)?,"next":edge(id,offset+1)})
            }
        } else {
            match &block.terminator.as_ref().ok_or("missing EVM model terminator")?.kind {
                TerminatorKind::Jump(target) => {
                    json!({"kind":"jump","growth":usize::from(module.next_block(id)!=Some(*target)),"next":edge(*target,0)})
                }
                TerminatorKind::JumpI { then_block, else_block } => {
                    json!({"kind":"branch","yes":edge(*then_block,0),"no":edge(*else_block,0)})
                }
                TerminatorKind::Op(opcode) => {
                    json!({"kind":"terminal","operation":op::mnemonic(*opcode).ok_or("unknown EVM terminal")?})
                }
                TerminatorKind::IndexedJump(_) => {
                    return Err("indexed EVM source graph requires layout semantics".into());
                }
            }
        };
        nodes.push(json!({"id":point,"label":block.label,"node":node}));
    }
    nodes.sort_by_key(|row| row["id"].as_u64().expect("model point"));
    let dense = nodes
        .iter()
        .enumerate()
        .map(|(index, row)| (row["id"].as_u64().expect("model point"), index))
        .collect::<BTreeMap<_, _>>();
    for (index, row) in nodes.iter_mut().enumerate() {
        row["id"] = index.into();
        for key in ["next", "yes", "no", "target"] {
            if let Some(old) = row["node"][key].as_u64() {
                row["node"][key] = (*dense.get(&old).ok_or("unresolved EVM source edge")?).into();
            }
        }
    }
    let mut graph = json!({"entry":dense[&(bases[entry] as u64)],"nodes":nodes});
    if read_code {
        graph["data_suffix"] = alloy_primitives::hex::encode(suffix.unwrap()).into();
    }
    Ok(graph)
}

fn instruction(inst: &Instruction) -> Result<String, String> {
    if let Some(op) = inst.as_stack_op() {
        return Ok(match op {
            StackOp::Dup(n) => format!("dup {n}"),
            StackOp::Swap(n) => format!("swap {n}"),
            StackOp::Exchange(n, m) => format!("exchange {n}, {m}"),
            StackOp::Pop => "pop".into(),
        });
    }
    if let Some(value) = inst.concrete_immediate() {
        return Ok(format!("push {value}"));
    }
    if inst.value.is_some() {
        return Err("unmodeled EVM source relocation".into());
    }
    Ok(inst.mnemonic().to_string())
}

/// Export every operation in a block, including its final transfer. Consumers
/// may prove a prefix only if the entire remaining continuation is identical.
fn window(module: &Module, entry: BlockId) -> Result<Value, String> {
    let block = &module.blocks[entry];
    let instructions = block
        .instructions
        .iter()
        .map(|inst| {
            if let Some(target) = inst.pushed_block() {
                Ok(format!("push bb{}", module.blocks[target].label))
            } else {
                instruction(inst)
            }
        })
        .collect::<Result<Vec<_>, String>>()?;
    let terminal = &block.terminator.as_ref().ok_or("missing EVM window terminator")?.kind;
    Ok(json!({
        "instructions": instructions,
        "terminal": format!("{terminal:?}"),
        "following": module.next_block(entry).map(|id| module.blocks[id].label),
    }))
}
