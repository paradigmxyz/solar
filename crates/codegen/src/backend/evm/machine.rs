//! MIR-to-physical-stack lowering with explicit activation and edge protocols.
//!
//! Every MIR block has a canonical stack of live values and simultaneous phi results. Internal
//! functions additionally carry one private return-label slot above an opaque suspended caller
//! prefix. Entry preambles load internal arguments once from the frame, while external ABI head
//! words remain lazy calldata loads as specified by retained ABI lowering. Calls split a physical
//! block at an explicit continuation, consume frame arguments, and return their first result on
//! the stack; additional results are published through the retained scratch-buffer convention.
//!
//! Edge reconciliation runs after successor selection and preserves each source until its final
//! simultaneous transfer. Observable instructions execute in MIR order. MIR identities and the
//! return-label marker exist only in this module and the private scheduler; emitted EVM IR contains
//! scheduled physical instructions and explicit control-flow edges.

use super::{
    calls, ir, op, parallel_copy,
    scheduler::Stack,
    spills::SpillPlan,
    storage::{FrameBase, FunctionStorage, ModulePlan},
};
use crate::{
    analysis::{AliasAnalysis, CallGraphInfo, CfgInfo, Liveness},
    immutable,
    memory::EvmMemoryLayout,
    mir,
};
use alloy_primitives::U256;
use solar_config::{EvmVersion, OptimizationMode};
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};
use std::cell::Cell;

mod call_entry;
mod call_reserve;
mod debug;
mod entry_order;
mod initialization;
mod rematerialize;
mod writer;

/// The constructor program-end relocation is resolved by primitive assembly.
const PROGRAM_END_ID: u32 = 0x0fff_ffff;

/// A generated physical program and its completed memory layout.
pub(crate) struct MachineOutput {
    pub(crate) ir: ir::Module,
    pub(crate) plan: ModulePlan,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Slot {
    ReturnAddress,
    Protected(usize),
    Value(mir::ValueId),
    CallLabel(ir::BlockId),
    Argument(mir::ArgIdx),
}

struct FunctionLayout {
    blocks: IndexVec<mir::BlockId, ir::BlockId>,
    entries: IndexVec<mir::BlockId, Vec<Slot>>,
    entry: ir::BlockId,
    returning: bool,
    live: Liveness,
    cfg: CfgInfo,
    alias: AliasAnalysis,
    spills: SpillPlan,
    rematerialized: FxHashMap<mir::ValueId, rematerialize::Recipe>,
    suppressed: Option<DenseBitSet<mir::InstId>>,
    // Definition boundaries: Phi values are available at entry, other results after their opcode.
    home_definitions: FxHashMap<mir::ValueId, (mir::BlockId, usize)>,
}

impl FunctionLayout {
    /// Preserves the ordinary plan's spill protocol even when recipes replace every home.
    fn uses_spill_protocol(&self) -> bool {
        !self.spills.homes.is_empty() || !self.rematerialized.is_empty()
    }
}

struct Context<'a> {
    module: &'a mir::Module,
    function: &'a mir::Function,
    storage: &'a FunctionStorage,
    plan: &'a ModulePlan,
    layout: &'a FunctionLayout,
    version: EvmVersion,
    optimization: OptimizationMode,
    deployment: bool,
    data_map: &'a FxHashMap<mir::DataId, ir::DataId>,
    tail_entry_scope: &'a Cell<Option<bool>>,
}

/// Generates one artifact rooted at a runtime dispatcher or constructor.
pub(crate) fn lower(
    module: &mir::Module,
    root: mir::FunctionId,
    version: EvmVersion,
    optimization: OptimizationMode,
    switches: &mut super::switches::Planner,
) -> Result<MachineOutput, String> {
    let deployment = module.function(root).attributes.is_constructor;
    let mut plan = ModulePlan::new(module, deployment).map_err(str::to_owned)?;
    let mut output = ir::Module {
        name: module.name.name,
        private_control_labels: true,
        debug_info_tracked: module.debug_info_is_tracked(),
        ..Default::default()
    };
    if deployment {
        output.program_size_id = Some(PROGRAM_END_ID);
    }
    let call_graph = CallGraphInfo::new(module);
    let mut reachable = call_graph.reachable_callees_from([root]);
    reachable.insert(root);
    let mut returnable = DenseBitSet::new_empty(module.functions.len());
    for id in reachable.iter() {
        let function = module.function(id);
        let cfg = CfgInfo::new(function);
        if function.blocks.iter_enumerated().any(|(block_id, block)| {
            cfg.is_reachable(block_id)
                && (matches!(block.terminator, Some(mir::Terminator::Return { .. }))
                    || (function.returns.is_empty()
                        && matches!(block.terminator, Some(mir::Terminator::Stop))))
        }) {
            returnable.insert(id);
        }
    }
    let mut returning = DenseBitSet::new_empty(module.functions.len());
    let mut has_internal_calls = false;
    for id in reachable.iter() {
        let function = module.function(id);
        let cfg = CfgInfo::new(function);
        for (block_id, block) in function.blocks.iter_enumerated() {
            if !cfg.is_reachable(block_id) {
                continue;
            }
            for &inst in &block.instructions {
                if let mir::InstKind::ICall { function: callee, .. } = function.inst(inst).kind {
                    has_internal_calls = true;
                    if returnable.contains(callee) {
                        returning.insert(callee);
                    }
                }
            }
        }
    }
    for (id, storage) in plan.functions.iter_mut_enumerated() {
        storage.stack_arguments &= returning.contains(id);
    }
    let possible_dynamic = plan
        .functions
        .iter()
        .any(|storage| storage.reachable && storage.base == FrameBase::Dynamic);
    let dynamic_frames = plan.functions.iter().any(|storage| {
        storage.reachable && storage.base == FrameBase::Dynamic && !storage.stack_arguments
    });
    let protocol_words = |storage: &FunctionStorage, active: bool| {
        let dynamic = storage.base == FrameBase::Dynamic;
        [
            if active { 1 + 2 * usize::from(dynamic && !storage.stack_arguments) } else { 0 },
            if dynamic { 3 } else { usize::from(possible_dynamic) },
        ]
    };
    // mstore(0x40, fixed_memory_end)
    // mstore(0xa0, 0) when dynamic activations are reachable
    let prologue = output.blocks.push(ir::Block::default());
    // Deployment often reaches only a constructor; omit unused heavyweight layouts.
    let mut layouts = FxHashMap::default();
    for id in reachable.iter() {
        let function = module.function(id);
        let live = Liveness::compute(function);
        let cfg = CfgInfo::new(function);
        let alias = AliasAnalysis::new(function);
        let spills = SpillPlan::new(
            function,
            &live,
            &cfg,
            &alias,
            (returning.contains(id), protocol_words(&plan.functions[id], dynamic_frames)),
            version,
            |value| stored(function, value, version, optimization),
        );
        plan.reserve_spills(id, spills.words).map_err(str::to_owned)?;
        let mut blocks = IndexVec::new();
        let mut entries = IndexVec::<mir::BlockId, Vec<mir::ValueId>>::new();
        for (block_id, block) in function.blocks.iter_enumerated() {
            blocks.push(if cfg.is_reachable(block_id) {
                output.blocks.push(ir::Block::default())
            } else {
                prologue
            });
            let mut values = live
                .live_in(block_id)
                .iter()
                .filter(|&value| stored(function, value, version, optimization))
                .collect::<Vec<_>>();
            for &inst in &block.instructions {
                if let Some(result) = function.inst_result_value(inst)
                    && matches!(function.inst(inst).kind, mir::InstKind::Phi(_))
                {
                    values.push(result);
                }
            }
            values.sort_unstable();
            values.dedup();
            values.retain(|value| !spills.homes.contains_key(value));
            entries.push(values);
        }
        let entries = entries
            .into_iter()
            .map(|values| {
                let mut entry = Vec::new();
                if returning.contains(id) {
                    entry.push(Slot::ReturnAddress);
                }
                entry.extend(values.into_iter().map(Slot::Value));
                entry
            })
            .collect();
        let entry = output.blocks.push(ir::Block::default());
        let home_definitions = home_definitions(function, &spills);
        layouts.insert(
            id,
            FunctionLayout {
                blocks,
                entries,
                entry,
                returning: returning.contains(id),
                live,
                cfg,
                alias,
                spills,
                rematerialized: FxHashMap::default(),
                suppressed: None,
                home_definitions,
            },
        );
    }
    plan.finalize().map_err(str::to_owned)?;
    // A spill can create the first dynamic frame. Recheck only stack-only layouts once;
    // every newly spilled layout already used its larger post-spill protocol bound.
    if !dynamic_frames && plan.max_dynamic_frame_size != 0 {
        for id in reachable.iter() {
            let function = module.function(id);
            let layout = layouts.get_mut(&id).unwrap();
            if layout.spills.homes.is_empty() {
                layout.spills = SpillPlan::new(
                    function,
                    &layout.live,
                    &layout.cfg,
                    &layout.alias,
                    (layout.returning, protocol_words(&plan.functions[id], true)),
                    version,
                    |value| stored(function, value, version, optimization),
                );
                plan.reserve_spills(id, layout.spills.words).map_err(str::to_owned)?;
                for entry in &mut layout.entries {
                    entry.retain(|slot| !matches!(slot, Slot::Value(value) if layout.spills.homes.contains_key(value)));
                }
                layout.home_definitions = home_definitions(function, &layout.spills);
            }
        }
        plan.finalize().map_err(str::to_owned)?;
    }
    // <selected immutable homes> -> <cached recipes at their consumers>
    // Keep the ordinary reserved words, Phi scratch and protocol fixed point unchanged.
    for id in reachable.iter() {
        let layout = layouts.get_mut(&id).unwrap();
        // Incoming internal calls can hide caller words below the tracked stack.
        // Computed recipes initially require a complete, bounded activation stack.
        let allow_computed = !deployment
            && !has_internal_calls
            && plan.max_dynamic_frame_size == 0
            && !layout.returning;
        let selected = rematerialize::select(
            module.function(id),
            &mut layout.spills.homes,
            optimization.is_gas(),
            allow_computed,
        );
        layout.rematerialized = selected.recipes;
        layout.suppressed = selected.suppressed;
        for value in layout.rematerialized.keys() {
            layout.home_definitions.remove(value);
        }
    }
    let reads_fmp = plan.max_dynamic_frame_size != 0
        || reachable.iter().any(|id| {
            initialization::requires_fmp(
                module.function(id),
                &layouts[&id].alias,
                &plan.functions[id],
                plan.fixed_memory_end,
            )
        });
    let fmp_entry = (reads_fmp && !deployment)
        .then(|| initialization::unique_frontier(module, root, &plan, &layouts, &call_graph))
        .flatten();
    if reads_fmp && fmp_entry.is_none() {
        // mstore(0x40, fixed_memory_end)
        output.blocks[prologue].insts.extend([
            ir::InstKind::Push(U256::from(plan.fixed_memory_end)).into(),
            ir::InstKind::Push(U256::from(EvmMemoryLayout::FMP_SLOT)).into(),
            ir::InstKind::Op(op::MSTORE).into(),
        ]);
    }
    if plan.max_dynamic_frame_size != 0 {
        output.blocks[prologue].insts.extend([
            ir::InstKind::Push(U256::ZERO).into(),
            ir::InstKind::Push(U256::from(EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT)).into(),
            ir::InstKind::Op(op::MSTORE).into(),
        ]);
    }
    output.blocks[prologue].terminator = ir::TerminatorKind::Jump(layouts[&root].entry).into();
    let mut used_data = DenseBitSet::new_empty(module.data_count());
    for function in reachable.iter().map(|id| module.function(id)) {
        for inst in function.instructions() {
            if let mir::InstKind::DataCopy(data, _, _) = function.inst(inst).kind {
                used_data.insert(data.id);
            }
        }
    }
    let mut data_map = FxHashMap::default();
    for (id, data) in module.iter_data() {
        if used_data.contains(id) {
            let output_id =
                output.data.push(ir::Data { name: module.data_name(id), bytes: data.to_vec() });
            data_map.insert(id, output_id);
        }
        if !deployment && module.data_is_emitted_in_runtime(id) {
            // <code>; <referenced constant data>; <opaque runtime trailer>
            output.appendix.extend_from_slice(data);
        }
    }
    // Returning activations can leave physical stack heights unknown. Retain
    // their argument stores so compact literal materialization keeps its budget.
    let tail_entry_scope = Cell::new((!returning.is_empty()).then_some(false));
    for id in reachable.iter() {
        let function = module.function(id);
        let context = Context {
            module,
            function,
            storage: &plan.functions[id],
            plan: &plan,
            layout: &layouts[&id],
            version,
            optimization,
            deployment,
            data_map: &data_map,
            tail_entry_scope: &tail_entry_scope,
        };
        let mut entry = Vec::new();
        if fmp_entry == Some(id) {
            // mstore(0x40, fixed_memory_end)
            // <the unique external entry materialization follows>
            entry.extend([
                ir::InstKind::Push(U256::from(plan.fixed_memory_end)).into(),
                ir::InstKind::Push(U256::from(EvmMemoryLayout::FMP_SLOT)).into(),
                ir::InstKind::Op(op::MSTORE).into(),
            ]);
        }
        if context.storage.stack_arguments {
            let (mut stack, desired) = call_entry::entry(function, context.layout)
                .ok_or("non-argument value is live at function entry")?;
            let mut copies = function
                .live_values()
                .filter_map(|value| {
                    let mir::Value::Arg(index) = function.value(value) else { return None };
                    context.layout.spills.homes.get(&value).map(|&home| (*index, home))
                })
                .collect::<Vec<_>>();
            copies.sort_unstable();
            for (position, &(index, home)) in copies.iter().enumerate() {
                // <return label>; <remaining arguments>; <argument for this home>
                // push <spill address>; mstore
                entry.extend(
                    stack
                        .prepare(&[Slot::Argument(index)], 1, version, |slot| {
                            desired.contains(&slot)
                                || copies[position + 1..]
                                    .iter()
                                    .any(|&(index, _)| slot == Slot::Argument(index))
                        })
                        .map_err(schedule_error)?,
                );
                store_spill(&context, home, &mut entry)?;
                stack.truncate(stack.values().len() - 1);
            }
            // <return label>; <canonical incoming argument values>
            entry.extend(stack.reconcile(&desired, 1, version).map_err(schedule_error)?);
        } else {
            for &slot in &context.layout.entries[mir::BlockId::ENTRY] {
                if let Slot::Value(value) = slot {
                    let mir::Value::Arg(index) = function.value(value) else {
                        return Err("non-argument value is live at function entry".into());
                    };
                    load_argument(&context, *index, &mut entry)?;
                }
            }
        }
        for value in function.live_values() {
            if !context.storage.stack_arguments
                && let mir::Value::Arg(index) = function.value(value)
                && let Some(&home) = context.layout.spills.homes.get(&value)
            {
                // mstore(argument_home, incoming_argument)
                load_argument(&context, *index, &mut entry)?;
                store_spill(&context, home, &mut entry)?;
            }
        }
        // <internal or constructor argument words>
        // jump <first MIR block>
        output.blocks[context.layout.entry] = ir::Block {
            insts: entry,
            terminator: ir::TerminatorKind::Jump(context.layout.blocks[mir::BlockId::ENTRY]).into(),
            ..Default::default()
        };
        output.blocks[context.layout.blocks[mir::BlockId::ENTRY]].function_invoke =
            debug::function(&context);
        lower_function(&context, &layouts, &mut output, switches)?;
    }
    call_entry::prune_unused(&mut output, &layouts);
    Ok(MachineOutput { ir: output, plan })
}

fn home_definitions(
    function: &mir::Function,
    spills: &SpillPlan,
) -> FxHashMap<mir::ValueId, (mir::BlockId, usize)> {
    let mut definitions = FxHashMap::default();
    if !spills.homes.is_empty() {
        for (block_id, block) in function.blocks.iter_enumerated() {
            for (position, &inst) in block.instructions.iter().enumerate() {
                if let Some(result) = function.inst_result_value(inst)
                    && spills.homes.contains_key(&result)
                {
                    let available_at = if matches!(function.inst(inst).kind, mir::InstKind::Phi(_))
                    {
                        0
                    } else {
                        position + 1
                    };
                    definitions.insert(result, (block_id, available_at));
                }
            }
        }
    }
    definitions
}

fn lower_function(
    context: &Context<'_>,
    layouts: &FxHashMap<mir::FunctionId, FunctionLayout>,
    output: &mut ir::Module,
    switches: &mut super::switches::Planner,
) -> Result<(), String> {
    let function = context.function;
    let layout = context.layout;
    for (block_id, block) in function.blocks.iter_enumerated() {
        if !layout.cfg.is_reachable(block_id) {
            continue;
        }
        let mut current = layout.blocks[block_id];
        let mut stack = Stack::new(layout.entries[block_id].clone());
        let mut insts = Vec::new();
        let mut emitted_until = 0;
        for (position, &inst_id) in block.instructions.iter().enumerate() {
            if position < emitted_until {
                continue;
            }
            let instruction = function.inst(inst_id);
            if matches!(instruction.kind, mir::InstKind::Phi(_)) {
                continue;
            }
            if matches!(instruction.kind, mir::InstKind::Gas)
                && call_reserve::lower(context, (block_id, position), &mut stack, &mut insts)?
            {
                emitted_until = position + 3;
                continue;
            }
            let origin_start = insts.len();
            let live = |value| layout.live.is_used_at_or_after(value, block_id, position + 1);
            if let mir::InstKind::ICall { function: callee, args, returns } = &instruction.kind {
                let saved = save_writer_homes(
                    context,
                    (block_id, position),
                    &mut stack,
                    &mut insts,
                    live,
                    args.len() + 5,
                    || true,
                )?;
                let continuation = output.blocks.push(ir::Block::default());
                let mut target = layouts[callee].entry;
                let caller;
                if layout.uses_spill_protocol() {
                    let base = stack.values()[..prefix(context)].to_vec();
                    insts.extend(
                        stack
                            .reconcile(&base, prefix(context), context.version)
                            .map_err(schedule_error)?,
                    );
                    caller = base;
                    // <suspended return label>; push <continuation>; <argN..arg0>
                    insts.push(ir::InstKind::PushLabel(continuation).into());
                    stack.push(Slot::CallLabel(continuation));
                    for &value in args.iter().rev() {
                        load_value(context, value, &mut insts)?;
                        stack.push(Slot::Value(value));
                    }
                } else {
                    let incoming = stack.clone();
                    let mut original = Vec::new();
                    prepare(context, &mut stack, &mut original, args, live)?;
                    caller = stack.values()[..stack.values().len() - args.len()].to_vec();
                    // push <continuation>
                    // <suspended caller>; <continuation>; <argN..arg0>
                    original.push(ir::InstKind::PushLabel(continuation).into());
                    stack.push(Slot::CallLabel(continuation));
                    let rotation_start = original.len();
                    let mut desired = caller.clone();
                    desired.push(Slot::CallLabel(continuation));
                    desired.extend(args.iter().rev().copied().map(Slot::Value));
                    original.extend(
                        stack
                            .reconcile(&desired, prefix(context), context.version)
                            .map_err(schedule_error)?,
                    );
                    if let Some((prepared, direct)) = call_entry::choose(
                        context,
                        *callee,
                        &layouts[callee],
                        args,
                        &incoming,
                        (continuation, &caller),
                        (&original, rotation_start),
                    ) {
                        // <suspended caller>; <continuation>; <canonical callee entry>
                        // jump <first callee MIR block>
                        insts.extend(prepared);
                        target = direct;
                    } else {
                        // <suspended caller>; <continuation>; <reverse argument order>
                        insts.extend(original);
                    }
                }
                debug::instructions(context, &instruction.metadata, &mut insts[origin_start..]);
                enter_call(
                    context,
                    &context.plan.functions[*callee],
                    args.len(),
                    current,
                    &mut insts,
                    target,
                    output,
                )?;
                current = continuation;
                stack = Stack::new(caller);
                stack.truncate(stack.values().len() - saved.tracked);
                restore_writer_homes(&saved, &mut insts, *returns != 0);
                if *returns != 0 {
                    if let Some(value) = function.inst_result_value(inst_id) {
                        if let Some(&home) = layout.spills.homes.get(&value) {
                            store_spill(context, home, &mut insts)?;
                        } else {
                            stack.push(Slot::Value(value));
                        }
                    } else {
                        // pop <unused first result>
                        insts.push(ir::InstKind::Op(op::POP).into());
                    }
                }
                continue;
            }
            if let Some(opcode) = instruction.kind.evm_opcode() {
                lower_opcode(
                    context,
                    (block_id, position),
                    inst_id,
                    opcode,
                    &mut stack,
                    &mut insts,
                    entry_order::OperandOrder::Canonical,
                )?;
                continue;
            }
            match instruction.kind {
                mir::InstKind::Select(condition, when_true, when_false) => {
                    prepare(
                        context,
                        &mut stack,
                        &mut insts,
                        &[condition, when_true, when_false],
                        live,
                    )?;
                    // false_value ^ ((true_value ^ false_value) * (condition != 0))
                    insts.extend([
                        ir::InstKind::Op(op::ISZERO).into(),
                        ir::InstKind::Op(op::ISZERO).into(),
                        ir::InstKind::Dup(2).into(),
                        ir::InstKind::Dup(4).into(),
                        ir::InstKind::Op(op::XOR).into(),
                        ir::InstKind::Op(op::MUL).into(),
                        ir::InstKind::Swap(1).into(),
                        ir::InstKind::Op(op::POP).into(),
                        ir::InstKind::Op(op::XOR).into(),
                    ]);
                    stack.truncate(stack.values().len() - 3);
                }
                mir::InstKind::InternalFrameAddr(offset) => {
                    // push <current frame address + offset>
                    insts.extend(calls::address(
                        context.storage.address(offset).map_err(str::to_owned)?,
                    ));
                }
                mir::InstKind::Alloc { .. } => {
                    // push <final deferred allocation address>
                    insts.push(
                        ir::InstKind::Push(U256::from(
                            context.storage.allocation_address(inst_id).map_err(str::to_owned)?,
                        ))
                        .into(),
                    );
                }
                mir::InstKind::ConstructorArgsBase => {
                    // push <copied constructor arguments base>
                    insts.push(
                        ir::InstKind::Push(U256::from(context.plan.constructor_arg_base)).into(),
                    );
                }
                mir::InstKind::ConstructorArgsEnd => {
                    // codesize - program_end + constructor_arg_base
                    insts.push(ir::InstKind::PushDeferred(PROGRAM_END_ID).into());
                    insts.push(ir::InstKind::Op(op::CODESIZE).into());
                    insts.push(ir::InstKind::Op(op::SUB).into());
                    insts.push(
                        ir::InstKind::Push(U256::from(context.plan.constructor_arg_base)).into(),
                    );
                    insts.push(ir::InstKind::Op(op::ADD).into());
                }
                mir::InstKind::LoadImmutable(id) => load_immutable(context, id, &mut insts)?,
                mir::InstKind::DataCopy(data, destination, size) => {
                    prepare(context, &mut stack, &mut insts, &[destination, size], live)?;
                    // <size>; <destination>; push_data <source>; swap 1; codecopy
                    insts.push(
                        ir::InstKind::PushData {
                            id: *context
                                .data_map
                                .get(&data.id)
                                .ok_or("missing live data relocation")?,
                            offset: data.offset,
                        }
                        .into(),
                    );
                    insts.push(ir::InstKind::Swap(1).into());
                    insts.push(ir::InstKind::Op(op::CODECOPY).into());
                    stack.truncate(stack.values().len() - 2);
                    debug::instructions(context, &instruction.metadata, &mut insts[origin_start..]);
                    continue;
                }
                _ => {
                    return Err(format!(
                        "unlowered MIR instruction `{}` reached the physical EVM boundary",
                        instruction.kind.mnemonic()
                    ));
                }
            }
            record_result(context, inst_id, &mut stack, &mut insts, true)?;
            debug::instructions(context, &instruction.metadata, &mut insts[origin_start..]);
        }
        if let Some(selected) = entry_order::choose_operands(context, block_id, &stack, &insts) {
            // <same opcode sequence>; <paid restore of original post-instruction stack>
            insts = selected;
        }
        let term = block.terminator.as_ref().ok_or("unterminated MIR block")?;
        let terminator = match term {
            mir::Terminator::Jump(target) => {
                if let Some((selected, instructions)) =
                    entry_order::choose(context, block_id, *target, &stack, &insts)
                {
                    // <paid entry permutation>; <same opcodes>; <canonical successor values>
                    stack = selected;
                    insts = instructions;
                }
                ir::TerminatorKind::Jump(edge(context, block_id, *target, &stack, output)?)
            }
            mir::Terminator::Branch { condition, then_block, else_block } => {
                prepare(context, &mut stack, &mut insts, &[*condition], |value| {
                    layout.live.live_out(block_id).contains(value)
                })?;
                stack.truncate(stack.values().len() - 1);
                ir::TerminatorKind::JumpI(
                    edge(context, block_id, *then_block, &stack, output)?,
                    edge(context, block_id, *else_block, &stack, output)?,
                )
            }
            mir::Terminator::Switch { value, default, cases } => {
                prepare(context, &mut stack, &mut insts, &[*value], |value| {
                    layout.live.live_out(block_id).contains(value)
                })?;
                stack.truncate(stack.values().len() - 1);
                let fallback = edge(context, block_id, *default, &stack, output)?;
                let mut targets = Vec::with_capacity(cases.len());
                for &(case, destination) in cases {
                    let mir::Value::Immediate(case) = function.value(case) else {
                        return Err("switch case is not an immediate".into());
                    };
                    targets.push((
                        case.as_u256().ok_or("non-scalar switch case")?,
                        edge(context, block_id, destination, &stack, output)?,
                    ));
                }
                // <live prefix>; <selector>
                // jump <physical strategy consuming selector>
                ir::TerminatorKind::Jump(switches.lower(output, &targets, fallback))
            }
            mir::Terminator::Return { values }
                if values.is_empty()
                    && (function.attributes.is_constructor || !layout.returning) =>
            {
                ir::TerminatorKind::Stop
            }
            mir::Terminator::Return { values } => {
                if !layout.returning {
                    return Err("value-returning MIR root has no internal caller".into());
                }
                return_values(context, &mut stack, &mut insts, values)?;
                ir::TerminatorKind::DynamicJump
            }
            mir::Terminator::Stop if layout.returning && function.returns.is_empty() => {
                return_values(context, &mut stack, &mut insts, &[])?;
                ir::TerminatorKind::DynamicJump
            }
            mir::Terminator::ReturnData { offset, size }
            | mir::Terminator::Revert { offset, size } => {
                prepare(context, &mut stack, &mut insts, &[*offset, *size], |_| false)?;
                if matches!(term, mir::Terminator::ReturnData { .. }) {
                    ir::TerminatorKind::Return
                } else {
                    ir::TerminatorKind::Revert
                }
            }
            mir::Terminator::Stop => ir::TerminatorKind::Stop,
            mir::Terminator::Invalid => ir::TerminatorKind::Invalid,
            mir::Terminator::SelfDestruct { recipient } => {
                prepare(context, &mut stack, &mut insts, &[*recipient], |_| false)?;
                ir::TerminatorKind::SelfDestruct
            }
            mir::Terminator::TailCall { function: callee, args } => {
                if layout.uses_spill_protocol() {
                    insts.extend(stack.reconcile(&[], 0, context.version).map_err(schedule_error)?);
                    for &value in args.iter().rev() {
                        load_value(context, value, &mut insts)?;
                        stack.push(Slot::Value(value));
                    }
                } else {
                    prepare(context, &mut stack, &mut insts, args, |_| false)?;
                }
                let desired = args.iter().rev().copied().map(Slot::Value).collect::<Vec<_>>();
                // <argN..arg0>
                // install the non-returning callee frame and jump without a return label
                insts
                    .extend(stack.reconcile(&desired, 0, context.version).map_err(schedule_error)?);
                if layouts[callee].returning {
                    return Err(format!(
                        "tail-call target `{}` from `{}` requires a returning activation",
                        context.module.function(*callee).name,
                        function.name
                    ));
                }
                call_entry::lower_tail(
                    context,
                    *callee,
                    &layouts[callee],
                    args,
                    &stack,
                    (current, &mut insts),
                    output,
                )?;
                continue;
            }
            mir::Terminator::RevertReturndata => {
                return Err("returndata bubbling requires MIR ABI lowering".into());
            }
        };
        // <scheduled instructions>
        // <physical terminator>
        output.blocks[current].insts = insts;
        output.blocks[current].terminator = terminator.into();
        debug::terminator(context, block, &mut output.blocks[current].terminator);
    }
    Ok(())
}

fn enter_call(
    context: &Context<'_>,
    callee: &FunctionStorage,
    arguments: usize,
    current: ir::BlockId,
    insts: &mut Vec<ir::Instruction>,
    target: ir::BlockId,
    output: &mut ir::Module,
) -> Result<(), String> {
    let setup = calls::enter(
        callee,
        context.storage,
        arguments,
        context.plan.fixed_memory_end,
        context.plan.max_dynamic_frame_size,
        context.version,
    )?;
    emit_call_setup(setup, current, insts, target, output);
    Ok(())
}

fn emit_call_setup(
    setup: calls::CallSetup,
    current: ir::BlockId,
    insts: &mut Vec<ir::Instruction>,
    target: ir::BlockId,
    output: &mut ir::Module,
) {
    if setup.guard.is_empty() {
        // <frame argument stores or checked canonical entry schedule>
        // jump <callee>
        insts.extend(setup.setup);
        output.blocks[current].insts = std::mem::take(insts);
        output.blocks[current].terminator = ir::TerminatorKind::Jump(target).into();
    } else {
        // <allocation guard>
        // jumpi <memory panic>, <frame setup>
        insts.extend(setup.guard);
        let success = output.blocks.push(ir::Block {
            insts: setup.setup,
            terminator: ir::TerminatorKind::Jump(target).into(),
            ..Default::default()
        });
        let panic = memory_panic(output);
        output.blocks[current].insts = std::mem::take(insts);
        output.blocks[current].terminator = ir::TerminatorKind::JumpI(panic, success).into();
    }
}

fn return_values(
    context: &Context<'_>,
    stack: &mut Stack<Slot>,
    insts: &mut Vec<ir::Instruction>,
    values: &[mir::ValueId],
) -> Result<(), String> {
    for (index, &value) in values.iter().enumerate().skip(1) {
        prepare(context, stack, insts, &[value], |value| values.contains(&value))?;
        // mstore(frame + return_offset + index * 32, result)
        insts.extend(calls::address(context.storage.return_address(index).map_err(str::to_owned)?));
        insts.push(ir::InstKind::Op(op::MSTORE).into());
        stack.truncate(stack.values().len() - 1);
    }
    if values.len() > 1 {
        // mstore(0x20, frame + return_offset)
        insts.extend(calls::address(context.storage.return_address(0).map_err(str::to_owned)?));
        insts.push(
            ir::InstKind::Push(U256::from(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT)).into(),
        );
        insts.push(ir::InstKind::Op(op::MSTORE).into());
    }
    let mut desired = Vec::new();
    if let Some(&first) = values.first() {
        desired.push(Slot::Value(first));
    }
    desired.push(Slot::ReturnAddress);
    materialize(context, stack, insts, &desired)?;
    // <first result if any>; <return label>
    // restore suspended dynamic activation
    insts.extend(stack.reconcile(&desired, 0, context.version).map_err(schedule_error)?);
    insts.extend(calls::leave(context.storage));
    Ok(())
}

/// Values held above the virtual activation prefix while an overlapping writer executes.
struct SavedHomes {
    protection: Option<writer::Protection>,
    addresses: Vec<super::storage::FrameAddress>,
    tracked: usize,
    protected_prefix: Option<usize>,
}

fn control_live_after(context: &Context<'_>, block: mir::BlockId, position: usize) -> bool {
    if context.layout.returning {
        return true;
    }
    let calls = |instructions: &[mir::InstId]| {
        instructions
            .iter()
            .any(|&inst| matches!(context.function.inst(inst).kind, mir::InstKind::ICall { .. }))
    };
    if calls(&context.function.blocks[block].instructions[position + 1..]) {
        return true;
    }
    let mut seen = DenseBitSet::new_empty(context.function.blocks.len());
    let mut work = context.function.blocks[block]
        .terminator
        .as_ref()
        .map(mir::Terminator::successors)
        .unwrap_or_default()
        .to_vec();
    while let Some(block) = work.pop() {
        if !seen.insert(block) {
            continue;
        }
        if calls(&context.function.blocks[block].instructions) {
            return true;
        }
        if let Some(term) = &context.function.blocks[block].terminator {
            work.extend(term.successors());
        }
    }
    false
}

fn save_writer_homes(
    context: &Context<'_>,
    (block, position): (mir::BlockId, usize),
    stack: &mut Stack<Slot>,
    output: &mut Vec<ir::Instruction>,
    live: impl Fn(mir::ValueId) -> bool,
    operands: usize,
    control_live: impl FnOnce() -> bool,
) -> Result<SavedHomes, String> {
    let inst = context.function.blocks[block].instructions[position];
    let effects = context.layout.alias.instruction_mod_ref(context.function, inst);
    if !effects.writes_space(crate::analysis::AddressSpace::Memory) {
        return Ok(SavedHomes {
            protection: None,
            addresses: Vec::new(),
            tracked: 0,
            protected_prefix: None,
        });
    }
    let control_live = context.plan.max_dynamic_frame_size != 0 && control_live();
    let mut homes = context
        .layout
        .spills
        .homes
        .iter()
        .filter_map(|(&value, &home)| {
            // The future-use query requires an already-defined value. A future definition may
            // reuse this word, but its old contents need no preservation across this writer.
            let available = context.layout.live.live_in(block).contains(value)
                || context.layout.home_definitions.get(&value).is_none_or(
                    |&(defined_block, available_at)| {
                        defined_block == block && available_at <= position
                    },
                );
            (available && live(value)).then_some(home)
        })
        .collect::<Vec<_>>();
    homes.sort_unstable();
    homes.dedup();
    let mut addresses = Vec::new();
    for home in homes {
        let address = context.storage.spill_address(home).map_err(str::to_owned)?;
        if super::spills::may_overlap(
            context.storage,
            context.plan.fixed_memory_end,
            &effects,
            address,
        ) {
            addresses.push(address);
        }
    }
    let spill_homes = addresses.len();
    if control_live
        && context.storage.base == super::storage::FrameBase::Dynamic
        && !context.storage.stack_arguments
    {
        for offset in [super::storage::PREVIOUS_FRAME_OFFSET, super::storage::SAVED_FMP_OFFSET] {
            let address = super::storage::FrameAddress::Relative(offset);
            if super::spills::may_overlap(
                context.storage,
                context.plan.fixed_memory_end,
                &effects,
                address,
            ) {
                addresses.push(address);
            }
        }
    }
    let frame_pointer =
        super::storage::FrameAddress::Absolute(EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT);
    if control_live
        && context.plan.max_dynamic_frame_size != 0
        && super::spills::may_overlap(
            context.storage,
            context.plan.fixed_memory_end,
            &effects,
            frame_pointer,
        )
    {
        addresses.push(frame_pointer);
    }
    let mixed = context.layout.uses_spill_protocol()
        && super::spills::direct_writer(&context.function.inst(inst).kind);
    let tracked = if !context.layout.uses_spill_protocol() || mixed { addresses.len() } else { 0 };
    let mut saved = SavedHomes { protection: None, addresses, tracked, protected_prefix: None };
    if saved.addresses.is_empty() {
        return Ok(saved);
    }
    if saved.addresses.len() + operands + stack.values().len() + 3 > 1024 {
        return Err("live values across a memory writer exceed the EVM stack limit".into());
    }
    if context.optimization.is_gas()
        && matches!(context.function.inst(inst).kind, mir::InstKind::MStore(..))
        && let Some(protection) = writer::choose(&saved.addresses, spill_homes, context.version)
    {
        // <unselected saved homes>
        // <selected homes are protected after operand preparation>
        saved.addresses.drain(protection.range.clone());
        saved.tracked = saved.addresses.len();
        saved.protection = Some(protection);
    }
    let writer_operands = mixed.then(|| context.function.inst(inst).kind.operands());
    let resident_operand =
        |value| writer_operands.as_ref().is_some_and(|operands| operands.contains(&value));
    // <activation return label>; <live residents and dying writer operands>
    // <saved homes>; <saved frame pointer>
    if context.layout.uses_spill_protocol() {
        let mut base = stack.values()[..prefix(context)].to_vec();
        if mixed {
            base.extend(stack.values()[prefix(context)..].iter().copied().filter(|slot| {
                matches!(slot, Slot::Value(value)
                    if !context.layout.spills.homes.contains_key(value)
                        && !context.layout.rematerialized.contains_key(value)
                        && (live(*value) || resident_operand(*value)))
            }));
        }
        output.extend(
            stack.reconcile(&base, prefix(context), context.version).map_err(schedule_error)?,
        );
    }
    for (index, &address) in saved.addresses.iter().enumerate() {
        output.extend(calls::address(address));
        output.push(ir::InstKind::Op(op::MLOAD).into());
        if tracked != 0 {
            stack.push(Slot::Protected(index));
        }
    }
    // A bounded dying operand below the backups uses ordinary preparation, which keeps
    // surviving residents and Protected words in their original order before the operands.
    if mixed
        && !stack
            .values()
            .iter()
            .any(|slot| matches!(slot, Slot::Value(value) if resident_operand(*value)))
    {
        saved.protected_prefix = Some(stack.values().len());
    }
    Ok(saved)
}

/// Restores private words while keeping an optional source result above the activation.
/// Absolute, disjoint words can restore in chunks after one result rotation; relative addresses
/// retain their original order because restoring the dynamic frame pointer affects their bases.
fn restore_writer_homes(saved: &SavedHomes, output: &mut Vec<ir::Instruction>, result: bool) {
    let absolute = result
        && saved
            .addresses
            .iter()
            .all(|address| matches!(address, super::storage::FrameAddress::Absolute(_)));
    for chunk in saved.addresses.rchunks(16) {
        let independent = absolute
            && chunk.iter().enumerate().all(|(index, address)| {
                chunk[..index].iter().all(|other| {
                    matches!((address, other),
                        (super::storage::FrameAddress::Absolute(a),
                         super::storage::FrameAddress::Absolute(b))
                        if a.abs_diff(*b) >= EvmMemoryLayout::WORD_SIZE)
                })
            });
        if independent {
            // <saved0..savedN-1>; result
            // swapN
            // mstore(home0, saved0)
            // mstore(homeN-1, savedN-1); ...; mstore(home1, saved1)
            output.push(ir::InstKind::Swap(chunk.len() as u16).into());
            for &address in std::iter::once(&chunk[0]).chain(chunk[1..].iter().rev()) {
                output.extend(calls::address(address));
                output.push(ir::InstKind::Op(op::MSTORE).into());
            }
        } else {
            for &address in chunk.iter().rev() {
                // <optional result>; mstore(protected_address, saved_value)
                if result {
                    output.push(ir::InstKind::Swap(1).into());
                }
                output.extend(calls::address(address));
                output.push(ir::InstKind::Op(op::MSTORE).into());
            }
        }
    }
}

fn lower_opcode(
    context: &Context<'_>,
    (block_id, position): (mir::BlockId, usize),
    inst_id: mir::InstId,
    opcode: u8,
    stack: &mut Stack<Slot>,
    insts: &mut Vec<ir::Instruction>,
    operand_order: entry_order::OperandOrder,
) -> Result<(), String> {
    let origin_start = insts.len();
    let function = context.function;
    let instruction = function.inst(inst_id);
    let live = |value| context.layout.live.is_used_at_or_after(value, block_id, position + 1);
    if context.layout.suppressed.as_ref().is_some_and(|suppressed| suppressed.contains(inst_id)) {
        // <retained live values>; pop any dependency whose last use was suppressed
        // The recipe emits the omitted computation at its surviving consumer.
        prepare(context, stack, insts, &[], live)?;
        return Ok(());
    }
    if (opcode == op::CALLDATALOAD || matches!(context.optimization, OptimizationMode::None))
        && let Some(value) = function.inst_result_value(inst_id)
        && ((opcode == op::CALLDATALOAD && context.layout.rematerialized.contains_key(&value))
            || rematerialize::nullary(function, value, context.version, context.optimization)
                .is_some())
    {
        // <retained live values>; discard any unused original offset
        // Re-emit the immutable read only at its consumers.
        prepare(context, stack, insts, &[], live)?;
        return Ok(());
    }
    let operands = instruction.kind.operands();
    let saved = save_writer_homes(
        context,
        (block_id, position),
        stack,
        insts,
        live,
        operands.len(),
        || control_live_after(context, block_id, position),
    )?;
    if let Some(fixed_prefix) = saved.protected_prefix {
        let operands = operands.iter().copied().map(Slot::Value).collect::<Vec<_>>();
        materialize(context, stack, insts, &operands)?;
        // <return label>; <residents>; <Protected backups>; <reverse operand pop order>
        insts.extend(
            stack
                .prepare(&operands, fixed_prefix, context.version, |_| false)
                .map_err(schedule_error)?,
        );
    } else if saved.tracked == 0
        && !matches!(opcode, op::GAS | op::PC)
        && op::stack_io(opcode).is_some_and(|(_, outputs)| outputs == 1)
        && let Some(preferred) = preferred_branch_values(
            context,
            block_id,
            position,
            function.inst_result_value(inst_id),
        )
    {
        let operands = operands.iter().copied().map(Slot::Value).collect::<Vec<_>>();
        materialize(context, stack, insts, &operands)?;
        // <fixed prefix>; <live values in successor order>; <opcode operands>
        insts.extend(
            super::entry_layout::prepare(
                stack,
                &operands,
                &preferred,
                prefix(context),
                context.version,
                opcode,
                |slot| match slot {
                    Slot::Value(value) => resident(context, value) && live(value),
                    Slot::ReturnAddress | Slot::Protected(_) => true,
                    Slot::CallLabel(_) | Slot::Argument(_) => false,
                },
            )
            .map_err(schedule_error)?,
        );
    } else if operand_order.allows(operands.len())
        && saved.tracked == 0
        && let Some(prepared) = stack.prepare_dead_operands(
            &operands.iter().copied().map(Slot::Value).collect::<Vec<_>>(),
            prefix(context),
            context.version,
            |slot| match slot {
                Slot::Value(value) => resident(context, value) && live(value),
                _ => false,
            },
        )
    {
        // <fixed prefix>; <reordered retained values>; <reverse last-use operand pop order>
        insts.extend(prepared);
    } else {
        prepare(context, stack, insts, &operands, live)?;
    }
    if let Some(protection) = &saved.protection {
        // <other backups>; value; destination
        // select and save two initialized homes; mstore; restore selected homes
        // The original capacity guard reserved at least twelve removed backups; the
        // template instead needs five words above the prepared operands.
        debug_assert!(stack.values().len() + 5 <= 1024);
        insts.extend_from_slice(&protection.instructions);
    } else {
        // <operands in pop order>
        // opcode
        insts.push(ir::InstKind::Op(opcode).into());
    }
    stack.truncate(stack.values().len() - operands.len());
    stack.truncate(stack.values().len() - saved.tracked);
    restore_writer_homes(
        &saved,
        insts,
        op::stack_io(opcode).is_some_and(|(_, outputs)| outputs == 1),
    );
    record_result(
        context,
        inst_id,
        stack,
        insts,
        op::stack_io(opcode).is_some_and(|(_, outputs)| outputs == 1),
    )?;
    debug::instructions(context, &instruction.metadata, &mut insts[origin_start..]);
    Ok(())
}

fn record_result(
    context: &Context<'_>,
    inst: mir::InstId,
    stack: &mut Stack<Slot>,
    output: &mut Vec<ir::Instruction>,
    produces: bool,
) -> Result<(), String> {
    if produces && stack.values().len() == 1024 {
        return Err("EVM instruction result exceeds the 1024-word stack limit".into());
    }
    if let Some(result) = context.function.inst_result_value(inst) {
        if let Some(&home) = context.layout.spills.homes.get(&result) {
            // mstore(value_home, result)
            store_spill(context, home, output)?;
        } else {
            stack.push(Slot::Value(result));
        }
    } else if produces {
        // pop <unused result>
        output.push(ir::InstKind::Op(op::POP).into());
    }
    Ok(())
}

fn external_argument(function: &mir::Function) -> bool {
    !function.attributes.is_constructor
        && (function.selector.is_some()
            || function.attributes.is_receive
            || function.attributes.is_fallback)
}

fn stored(
    function: &mir::Function,
    value: mir::ValueId,
    version: EvmVersion,
    optimization: OptimizationMode,
) -> bool {
    match function.value(value) {
        mir::Value::Inst(_) => {
            !matches!(optimization, OptimizationMode::None)
                || rematerialize::nullary(function, value, version, optimization).is_none()
        }
        mir::Value::Arg(_) => !external_argument(function),
        _ => false,
    }
}

fn resident(context: &Context<'_>, value: mir::ValueId) -> bool {
    stored(context.function, value, context.version, context.optimization)
        && !context.layout.spills.homes.contains_key(&value)
        && !context.layout.rematerialized.contains_key(&value)
}

fn prefix(context: &Context<'_>) -> usize {
    usize::from(context.layout.returning)
}

fn load_argument(
    context: &Context<'_>,
    index: mir::ArgIdx,
    output: &mut Vec<ir::Instruction>,
) -> Result<(), String> {
    let offset = u64::try_from(index.index())
        .ok()
        .and_then(|index| index.checked_mul(EvmMemoryLayout::WORD_SIZE))
        .ok_or("argument offset overflow")?;
    if external_argument(context.function) {
        // calldataload(4 + argument_offset)
        output.push(
            ir::InstKind::Push(U256::from(
                offset.checked_add(4).ok_or("ABI head offset overflow")?,
            ))
            .into(),
        );
        output.push(ir::InstKind::Op(op::CALLDATALOAD).into());
    } else if context.function.attributes.is_constructor {
        // mload(constructor_arg_base + argument_offset)
        output.push(
            ir::InstKind::Push(U256::from(
                context
                    .plan
                    .constructor_arg_base
                    .checked_add(offset)
                    .ok_or("constructor argument offset overflow")?,
            ))
            .into(),
        );
        output.push(ir::InstKind::Op(op::MLOAD).into());
    } else {
        // mload(frame + argument_offset)
        let offset = context
            .storage
            .argument_offset
            .checked_add(offset)
            .ok_or("internal argument offset overflow")?;
        output.extend(calls::address(context.storage.address(offset).map_err(str::to_owned)?));
        output.push(ir::InstKind::Op(op::MLOAD).into());
    }
    Ok(())
}

fn materialize(
    context: &Context<'_>,
    stack: &mut Stack<Slot>,
    output: &mut Vec<ir::Instruction>,
    values: &[Slot],
) -> Result<(), String> {
    for &slot in values.iter().rev() {
        if stack.values().contains(&slot) {
            continue;
        }
        let Slot::Value(value) = slot else {
            return Err("return label is absent from its activation stack".into());
        };
        load_value(context, value, output)?;
        stack.push(slot);
    }
    Ok(())
}

fn load_value(
    context: &Context<'_>,
    value: mir::ValueId,
    output: &mut Vec<ir::Instruction>,
) -> Result<(), String> {
    if let Some(recipe) = context.layout.rematerialized.get(&value) {
        debug_assert!(!context.layout.spills.homes.contains_key(&value));
        rematerialize::emit(recipe, output);
        return Ok(());
    }
    if matches!(context.optimization, OptimizationMode::None)
        && let Some(opcode) =
            rematerialize::nullary(context.function, value, context.version, context.optimization)
    {
        // <stable nullary read>
        output.push(ir::InstKind::Op(opcode).into());
        return Ok(());
    }
    if let Some(&home) = context.layout.spills.homes.get(&value) {
        // mload(value_home)
        output.extend(calls::address(context.storage.spill_address(home).map_err(str::to_owned)?));
        output.push(ir::InstKind::Op(op::MLOAD).into());
        return Ok(());
    }
    match context.function.value(value) {
        mir::Value::Arg(index) if external_argument(context.function) => {
            load_argument(context, *index, output)?
        }
        mir::Value::Immediate(immediate) => {
            // push <literal>
            output.push(
                ir::InstKind::Push(immediate.as_u256().ok_or("non-scalar immediate")?).into(),
            );
        }
        mir::Value::Undef(_) => {
            // push 0
            output.push(ir::InstKind::Push(U256::ZERO).into());
        }
        _ => return Err(format!("value v{} is absent from the physical stack", value.index())),
    }
    Ok(())
}

fn store_spill(
    context: &Context<'_>,
    home: usize,
    output: &mut Vec<ir::Instruction>,
) -> Result<(), String> {
    // push <value home>; mstore
    output.extend(calls::address(context.storage.spill_address(home).map_err(str::to_owned)?));
    output.push(ir::InstKind::Op(op::MSTORE).into());
    Ok(())
}

fn prepare(
    context: &Context<'_>,
    stack: &mut Stack<Slot>,
    output: &mut Vec<ir::Instruction>,
    values: &[mir::ValueId],
    mut live: impl FnMut(mir::ValueId) -> bool,
) -> Result<(), String> {
    let values = values.iter().copied().map(Slot::Value).collect::<Vec<_>>();
    materialize(context, stack, output, &values)?;
    // <retained live values>; <operands in reverse pop order>
    output.extend(
        stack
            .prepare(&values, prefix(context), context.version, |slot| match slot {
                Slot::Value(value) => resident(context, value) && live(value),
                Slot::ReturnAddress | Slot::Protected(_) => true,
                Slot::CallLabel(_) | Slot::Argument(_) => false,
            })
            .map_err(schedule_error)?,
    );
    Ok(())
}

fn edge(
    context: &Context<'_>,
    from: mir::BlockId,
    to: mir::BlockId,
    stack: &Stack<Slot>,
    output: &mut ir::Module,
) -> Result<ir::BlockId, String> {
    if context.layout.uses_spill_protocol() {
        let mut insts = Vec::new();
        let desired = edge_values(context, from, to)?;
        // <canonical resident successor values>; <memory-only simultaneous Phi copies>
        insts.extend(
            stack
                .clone()
                .reconcile(&desired, prefix(context), context.version)
                .map_err(schedule_error)?,
        );
        let mut transfers = Vec::new();
        for &inst in &context.function.blocks[to].instructions {
            if let mir::InstKind::Phi(incoming) = &context.function.inst(inst).kind
                && let Some(result) = context.function.inst_result_value(inst)
            {
                let source = incoming
                    .iter()
                    .find(|(pred, _)| *pred == from)
                    .ok_or("missing MIR phi predecessor")?
                    .1;
                let source = context
                    .layout
                    .spills
                    .homes
                    .get(&source)
                    .copied()
                    .map_or(parallel_copy::Source::Value(source), parallel_copy::Source::Home);
                transfers.push((source, context.layout.spills.homes[&result]));
            }
        }
        let ordered = parallel_copy::schedule(transfers.clone(), context.layout.spills.phi_scratch);
        let mut copies = emit_spill_copies(context, &ordered)?;
        if context.version.has_mcopy()
            && matches!(context.storage.base, super::storage::FrameBase::Static(_))
            && transfers.len() >= 2
        {
            let staged = emit_spill_copies(
                context,
                &parallel_copy::stage(&transfers, context.layout.spills.phi_scratch),
            )?;
            let direct_cost = ir::copy_cost(context.version, &copies);
            let staged_cost = ir::copy_cost(context.version, &staged);
            let staged_wins = if context.optimization.is_size() {
                // Contiguous staging retains MCOPY and shared-tail opportunities across cycles.
                ordered.iter().any(|&(_, home)| home >= context.layout.spills.phi_scratch)
                    || (staged_cost.1, staged_cost.0) <= (direct_cost.1, direct_cost.0)
            } else {
                staged_cost.0 <= direct_cost.0 && staged_cost.1 <= direct_cost.1
            };
            if staged_wins {
                copies = staged;
            }
        }
        // <simultaneous spill copies in the selected safe order>
        insts.extend(copies);
        return if insts.is_empty() {
            Ok(context.layout.blocks[to])
        } else {
            Ok(output.blocks.push(ir::Block {
                insts,
                terminator: ir::TerminatorKind::Jump(context.layout.blocks[to]).into(),
                ..Default::default()
            }))
        };
    }
    let desired = edge_values(context, from, to)?;
    let mut stack = stack.clone();
    let mut insts = Vec::new();
    materialize(context, &mut stack, &mut insts, &desired)?;
    // <simultaneously selected phi sources and live-in values>
    // jump <successor>
    insts.extend(
        stack.reconcile(&desired, prefix(context), context.version).map_err(schedule_error)?,
    );
    if insts.is_empty() {
        return Ok(context.layout.blocks[to]);
    }
    Ok(output.blocks.push(ir::Block {
        insts,
        terminator: ir::TerminatorKind::Jump(context.layout.blocks[to]).into(),
        ..Default::default()
    }))
}

fn emit_spill_copies(
    context: &Context<'_>,
    transfers: &[(parallel_copy::Source, usize)],
) -> Result<Vec<ir::Instruction>, String> {
    let mut insts = Vec::new();
    for &(source, home) in transfers {
        match source {
            parallel_copy::Source::Home(source) => {
                // mload(source_home)
                insts.extend(calls::address(
                    context.storage.spill_address(source).map_err(str::to_owned)?,
                ));
                insts.push(ir::InstKind::Op(op::MLOAD).into());
            }
            parallel_copy::Source::Value(value) => load_value(context, value, &mut insts)?,
        }
        // mstore(destination_home, source_value)
        store_spill(context, home, &mut insts)?;
    }
    Ok(insts)
}

fn edge_values(
    context: &Context<'_>,
    from: mir::BlockId,
    to: mir::BlockId,
) -> Result<Vec<Slot>, String> {
    let mut desired = context.layout.entries[to].clone();
    for slot in &mut desired {
        if let Slot::Value(value) = slot
            && let mir::Value::Inst(inst) = context.function.value(*value)
            && context.function.blocks[to].instructions.contains(inst)
            && let mir::InstKind::Phi(incoming) = &context.function.inst(*inst).kind
        {
            *value = incoming
                .iter()
                .find(|(pred, _)| *pred == from)
                .ok_or("missing MIR phi predecessor")?
                .1;
        }
    }
    Ok(desired)
}

fn preferred_branch_values(
    context: &Context<'_>,
    block: mir::BlockId,
    position: usize,
    result: Option<mir::ValueId>,
) -> Option<Vec<Slot>> {
    let source = &context.function.blocks[block];
    if position + 1 != source.instructions.len() || context.layout.uses_spill_protocol() {
        return None;
    }
    let mir::Terminator::Branch { condition, then_block, else_block } =
        source.terminator.as_ref()?
    else {
        return None;
    };
    if result != Some(*condition) || context.layout.live.live_out(block).contains(*condition) {
        return None;
    }
    let cyclic = context.layout.cfg.cyclic_blocks();
    let target = match (cyclic.contains(*then_block), cyclic.contains(*else_block)) {
        (true, false) => *then_block,
        (false, true) => *else_block,
        _ => return None,
    };
    edge_values(context, block, target).ok()
}

fn load_immutable(
    context: &Context<'_>,
    id: mir::ImmutableId,
    output: &mut Vec<ir::Instruction>,
) -> Result<(), String> {
    if context.deployment {
        // mload(immutable_staging_base + id * 32)
        let address = context
            .plan
            .immutable_staging_base
            .checked_add((id.index() as u64) * EvmMemoryLayout::WORD_SIZE)
            .ok_or("immutable staging address overflow")?;
        output.push(ir::InstKind::Push(U256::from(address)).into());
        output.push(ir::InstKind::Op(op::MLOAD).into());
    } else {
        let encoding = context
            .module
            .immutable_type(id)
            .immutable_encoding()
            .ok_or("unsupported immutable representation")?;
        let width = immutable::immutable_push_type_size(
            encoding,
            context.optimization,
            context.version.has_bitwise_shifting(),
        )
        .bytes();
        // push_immutable <id>, <encoded width>
        output.push(ir::InstKind::PushImmutable { id, width }.into());
        if width < 32 {
            match encoding {
                mir::ImmutableEncoding::Signed(_) => {
                    // signextend(encoded_width - 1, value)
                    output.push(ir::InstKind::Push(U256::from(width - 1)).into());
                    output.push(ir::InstKind::Op(op::SIGNEXTEND).into());
                }
                mir::ImmutableEncoding::LeftAligned(_) => {
                    // shl(256 - encoded_width * 8, value)
                    output.push(ir::InstKind::Push(U256::from((32 - width) as u64 * 8)).into());
                    output.push(ir::InstKind::Op(op::SHL).into());
                }
                mir::ImmutableEncoding::Unsigned(_) => {}
            }
        }
    }
    Ok(())
}

fn memory_panic(output: &mut ir::Module) -> ir::BlockId {
    // mstore(0, 0x4e487b71 << 224)
    // mstore(4, 0x41)
    // revert(0, 36)
    output.blocks.push(ir::Block {
        insts: vec![
            ir::InstKind::Push(U256::from(0x4e487b71_u64) << 224).into(),
            ir::InstKind::Push(U256::ZERO).into(),
            ir::InstKind::Op(op::MSTORE).into(),
            ir::InstKind::Push(U256::from(0x41)).into(),
            ir::InstKind::Push(U256::from(4)).into(),
            ir::InstKind::Op(op::MSTORE).into(),
            ir::InstKind::Push(U256::from(36)).into(),
            ir::InstKind::Push(U256::ZERO).into(),
        ],
        terminator: ir::TerminatorKind::Revert.into(),
        cold: true,
        ..Default::default()
    })
}

fn schedule_error(error: super::scheduler::StackError) -> String {
    format!("cannot schedule EVM stack: {error:?}")
}
