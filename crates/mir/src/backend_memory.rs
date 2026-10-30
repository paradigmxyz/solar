//! Backend memory placements carried by final MIR dumps for execution tools.
//!
//! These comments describe resolved runtime allocations and return buffers. They never
//! participate in optimization or instruction selection. Allocation ordinals follow active
//! block order rather than SSA IDs, which the textual parser may renumber.

use crate::{
    Callee, EffectKind, Function, FunctionId, InstId, Module, Semantics, Value, ValueId,
    analysis::CfgInfo, memory::EvmMemoryLayout,
};
use alloy_primitives::U256;
use solar_data_structures::map::{FxHashMap, FxHashSet};
use std::fmt;

/// The backend's chosen lowering of a deferred allocation.
#[derive(Clone, Copy, Debug)]
pub enum AllocationPlacement {
    /// The allocation lives at this fixed address.
    Static(U256),
    /// The allocation bumps the free memory pointer by this many bytes.
    Dynamic(U256),
}

/// Runtime layout decisions, captured only for final MIR execution.
#[derive(Clone, Debug, Default)]
pub struct BackendMemory {
    /// The placement of each deferred allocation.
    pub allocations: FxHashMap<(FunctionId, InstId), AllocationPlacement>,
    /// The address of the buffer each function publishes its further results to.
    pub returns: FxHashMap<FunctionId, U256>,
    /// The calls whose further results stay on the stack instead of passing through the buffer.
    pub stack_returns: FxHashMap<(FunctionId, InstId), StackReturn>,
    /// The free memory pointer each runtime entry starts with.
    pub entry_heaps: FxHashMap<FunctionId, U256>,
    /// The heap floor each function's `heap_floor` reads.
    pub heap_floors: FxHashMap<FunctionId, U256>,
    /// The end of the frames and spill slots a runtime entry reaches, which the backend writes
    /// without MIR showing it.
    pub frame_ends: FxHashMap<FunctionId, U256>,
}

/// Buffer protocol instructions replaced by stack-returned results.
#[derive(Clone, Debug)]
pub struct StackReturn {
    /// The buffer reads after the call that the backend removed, in order.
    pub elided: Vec<InstId>,
    /// The value each further result binds, if the function uses it.
    pub extras: Vec<Option<ValueId>>,
}

impl BackendMemory {
    pub(crate) fn display(&self, module: &Module) -> impl fmt::Display {
        fmt::from_fn(|f| {
            writeln!(f, "// backend-memory-v1")?;
            for (id, func) in module.functions.iter_enumerated() {
                let instructions = printed_instructions(func);
                let allocations = instructions
                    .iter()
                    .copied()
                    .filter(|&inst| func.inst(inst).metadata.deferred_alloc());
                for (ordinal, inst) in allocations.enumerate() {
                    if let Some(placement) = self.allocations.get(&(id, inst)) {
                        let (kind, word) = match placement {
                            AllocationPlacement::Static(word) => ("static", word),
                            AllocationPlacement::Dynamic(word) => ("dynamic", word),
                        };
                        writeln!(f, "// backend-alloc {} {ordinal} {kind} {word:#x}", id.index())?;
                    }
                }
                for (ordinal, &inst) in instructions.iter().enumerate() {
                    if let Some(protocol) = self.stack_returns.get(&(id, inst)) {
                        let ordinal_of = |inst| {
                            instructions
                                .iter()
                                .position(|&candidate| candidate == inst)
                                .expect("active protocol instruction")
                                .to_string()
                        };
                        let elided = protocol
                            .elided
                            .iter()
                            .map(|&inst| ordinal_of(inst))
                            .collect::<Vec<_>>()
                            .join(",");
                        let extras = protocol
                            .extras
                            .iter()
                            .map(|value| match value.map(|value| func.value(value)) {
                                Some(Value::Inst(inst)) => ordinal_of(*inst),
                                _ => "-".to_owned(),
                            })
                            .collect::<Vec<_>>()
                            .join(",");
                        writeln!(
                            f,
                            "// backend-stack {} {ordinal} {} {extras}",
                            id.index(),
                            if elided.is_empty() { "-" } else { &elided }
                        )?;
                    }
                }
                if let Some(word) = self.returns.get(&id) {
                    writeln!(f, "// backend-return {} {word:#x}", id.index())?;
                }
                if let Some(word) = self.entry_heaps.get(&id) {
                    writeln!(f, "// backend-entry {} {word:#x}", id.index())?;
                }
                if let Some(word) = self.heap_floors.get(&id) {
                    writeln!(f, "// backend-floor {} {word:#x}", id.index())?;
                }
                if let Some(word) = self.frame_ends.get(&id) {
                    writeln!(f, "// backend-frames {} {word:#x}", id.index())?;
                }
            }
            Ok(())
        })
    }

    pub(crate) fn parse(module: &Module, text: &str) -> Result<Option<Self>, String> {
        let mut layout = None;
        for line in text.lines().map(str::trim) {
            if line == "// backend-memory-v1" {
                if layout.replace(Self::default()).is_some() {
                    return Err("duplicate backend memory header".into());
                }
                continue;
            }
            let Some(rest) = line.strip_prefix("// backend-") else { continue };
            let Some(layout) = &mut layout else {
                return Err("backend memory record without a header".into());
            };
            let fields = rest.split_whitespace().collect::<Vec<_>>();
            let function = fields
                .get(1)
                .and_then(|s| s.parse::<usize>().ok())
                .filter(|&index| index < module.functions.len())
                .map(FunctionId::from_usize)
                .ok_or("invalid backend memory function")?;
            match fields.as_slice() {
                ["alloc", _, ordinal, kind, word] => {
                    let ordinal =
                        ordinal.parse::<usize>().map_err(|_| "invalid allocation ordinal")?;
                    let func = &module.functions[function];
                    let inst = printed_instructions(func)
                        .into_iter()
                        .filter(|&inst| func.inst(inst).metadata.deferred_alloc())
                        .nth(ordinal)
                        .ok_or("unknown backend allocation")?;
                    let word = word.parse::<U256>().map_err(|_| "invalid backend memory word")?;
                    let placement = match *kind {
                        "static" => AllocationPlacement::Static(word),
                        "dynamic" => AllocationPlacement::Dynamic(word),
                        _ => return Err("unknown backend allocation placement".into()),
                    };
                    if layout.allocations.insert((function, inst), placement).is_some() {
                        return Err("duplicate backend allocation".into());
                    }
                }
                ["stack", _, call, elided, extras] => {
                    let func = &module.functions[function];
                    let instructions = printed_instructions(func);
                    let instruction = |ordinal: &str| {
                        ordinal
                            .parse::<usize>()
                            .ok()
                            .and_then(|index| instructions.get(index).copied())
                            .ok_or("invalid backend protocol instruction")
                    };
                    let call = instruction(call)?;
                    let elided = if *elided == "-" {
                        Vec::new()
                    } else {
                        elided.split(',').map(instruction).collect::<Result<Vec<_>, _>>()?
                    };
                    let extras = extras
                        .split(',')
                        .map(|ordinal| {
                            if ordinal == "-" {
                                Ok(None)
                            } else {
                                func.inst_result_value(instruction(ordinal)?)
                                    .map(Some)
                                    .ok_or("backend projection without a result")
                            }
                        })
                        .collect::<Result<Vec<_>, &str>>()?;
                    let protocol = StackReturn { elided, extras };
                    protocol.validate(module, function, call)?;
                    if layout.stack_returns.insert((function, call), protocol).is_some() {
                        return Err("duplicate backend stack return".into());
                    }
                }
                [kind @ ("return" | "floor" | "entry" | "frames"), _, word] => {
                    let word = word.parse::<U256>().map_err(|_| "invalid backend memory word")?;
                    let map = match *kind {
                        "return" => &mut layout.returns,
                        "entry" => &mut layout.entry_heaps,
                        "floor" => &mut layout.heap_floors,
                        _ => &mut layout.frame_ends,
                    };
                    if map.insert(function, word).is_some() {
                        return Err("duplicate backend memory word".into());
                    }
                }
                _ => return Err("unknown backend memory record".into()),
            }
        }
        Ok(layout)
    }
}

/// Matches display_function_text: reachable blocks in RPO, then unreachable blocks.
fn printed_instructions(func: &Function) -> Vec<InstId> {
    let cfg = CfgInfo::new(func);
    cfg.rpo()
        .iter()
        .copied()
        .chain(func.blocks.indices().filter(|&block| !cfg.is_reachable(block)))
        .flat_map(|block| func.blocks[block].instructions.iter().copied())
        .collect()
}

impl StackReturn {
    /// Independently check the declared memory reads, so a bad backend projection cannot
    /// make the oracle agree with itself by rebinding unrelated SSA values.
    fn validate(&self, module: &Module, function: FunctionId, call: InstId) -> Result<(), String> {
        let func = &module.functions[function];
        let Some(Semantics::Call(Callee::Function(callee), _)) = func.inst(call).kind.semantics()
        else {
            return Err("backend stack return does not name an internal call".into());
        };
        let callee = module
            .functions
            .get(*callee)
            .ok_or("backend stack return calls an unknown function")?;
        if self.extras.len() + 1 != callee.return_components().len() {
            return Err("backend stack return has the wrong arity".into());
        }
        if !self.elided.is_empty() {
            let block = func
                .blocks
                .iter()
                .find(|block| block.instructions.contains(&call))
                .ok_or("backend call is not in an active block")?;
            let mut previous = block.instructions.iter().position(|&inst| inst == call).unwrap();
            for (index, &inst) in self.elided.iter().enumerate() {
                let position = block
                    .instructions
                    .iter()
                    .position(|&candidate| candidate == inst)
                    .filter(|&position| position > previous)
                    .ok_or("backend projection crosses a block or precedes its call")?;
                // Only pure code may run before the buffer pointer is read; reads may follow it,
                // since nothing writes the buffer before the next call.
                let allowed = |effect: EffectKind| {
                    if index == 0 { effect == EffectKind::Pure } else { effect.is_read_only() }
                };
                if block.instructions[previous + 1..position]
                    .iter()
                    .any(|&between| !allowed(func.inst(between).kind.effect_kind()))
                {
                    return Err("backend projection crosses a memory or call effect".into());
                }
                previous = position;
            }
        }
        let mut offsets = FxHashMap::default();
        let mut reads = FxHashMap::default();
        let mut elided = FxHashSet::default();
        for &inst in &self.elided {
            if !elided.insert(inst) {
                return Err("duplicate backend protocol instruction".into());
            }
            let Some(result) = func.inst_result_value(inst) else {
                return Err("backend protocol instruction has no result".into());
            };
            match func.inst(inst).kind.semantics() {
                Some(Semantics::Opcode("mload", args)) if args.len() == 1 => {
                    if func.value_u64(args[0])
                        == Some(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT)
                    {
                        offsets.insert(result, 0u64);
                    } else if let Some(&offset) = offsets.get(&args[0]) {
                        reads.insert(result, offset);
                    } else {
                        return Err("backend projection does not read the return buffer".into());
                    }
                }
                Some(Semantics::Opcode("add", args)) if args.len() == 2 => {
                    let offset = [(args[0], args[1]), (args[1], args[0])]
                        .into_iter()
                        .find_map(|(base, constant)| {
                            offsets.get(&base)?.checked_add(func.value_u64(constant)?)
                        })
                        .ok_or("backend projection has an unknown buffer offset")?;
                    offsets.insert(result, offset);
                }
                _ => return Err("backend projection elides a non-protocol instruction".into()),
            }
        }
        for (index, value) in self.extras.iter().enumerate() {
            if let Some(value) = value
                && reads.remove(value) != Some((index as u64 + 1) * EvmMemoryLayout::WORD_SIZE)
            {
                return Err("backend projection reads the wrong result word".into());
            }
        }
        if !reads.is_empty() {
            return Err("backend projection discards an undeclared result word".into());
        }
        for inst in func.instructions() {
            if !elided.contains(&inst)
                && func.inst(inst).kind.operands().iter().any(|value| offsets.contains_key(value))
            {
                return Err("backend return-buffer address escapes its protocol".into());
            }
        }
        for block in &func.blocks {
            let mut escapes = false;
            if let Some(term) = &block.terminator {
                term.visit_operands(|value| escapes |= offsets.contains_key(&value));
            }
            if escapes {
                return Err("backend return-buffer address escapes through a terminator".into());
            }
        }
        Ok(())
    }
}
