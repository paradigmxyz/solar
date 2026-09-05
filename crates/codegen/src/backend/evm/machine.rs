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
    storage::{FunctionStorage, ModulePlan},
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
    let mut output = ir::Module { name: module.name.name, ..Default::default() };
    if deployment {
        output.program_size_id = Some(PROGRAM_END_ID);
    }
    let mut reachable = CallGraphInfo::new(module).reachable_callees_from([root]);
    reachable.insert(root);
    let mut returnable = DenseBitSet::new_empty(module.functions.len());
    for (id, function) in module.iter_functions() {
        let cfg = CfgInfo::new(function);
        if function.blocks.iter_enumerated().any(|(block_id, block)| cfg.is_reachable(block_id) && (matches!(block.terminator, Some(mir::Terminator::Return { .. })) || (function.returns.is_empty() && matches!(block.terminator, Some(mir::Terminator::Stop))))) {
            returnable.insert(id);
        }
    }
    let mut returning = DenseBitSet::new_empty(module.functions.len());
    for id in reachable.iter() {
        let function = module.function(id);
        let cfg = CfgInfo::new(function);
        for (block_id, block) in function.blocks.iter_enumerated() {
            if !cfg.is_reachable(block_id) { continue; }
            for &inst in &block.instructions {
                if let mir::InstKind::InternalCall { function: callee, .. } = function.inst(inst).kind
                    && returnable.contains(callee)
                {
                    returning.insert(callee);
                }
            }
        }
    }
    for (id, storage) in plan.functions.iter_mut_enumerated() {
        storage.stack_arguments &= returning.contains(id);
    }
    // mstore(0x40, fixed_memory_end)
    // mstore(0xa0, 0) when dynamic activations are reachable
    let prologue = output.blocks.push(ir::Block::default());
    let mut layouts = IndexVec::<mir::FunctionId, FunctionLayout>::new();
    for (id, function) in module.iter_functions() {
        let live = Liveness::compute(function);
        let cfg = CfgInfo::new(function);
        let spills = if reachable.contains(id) {
            SpillPlan::new(function, &live, &cfg, returning.contains(id), version, |value| {
                stored(function, value)
            })
        } else {
            SpillPlan::default()
        };
        plan.reserve_spills(id, spills.words).map_err(str::to_owned)?;
        let mut blocks = IndexVec::new();
        let mut entries = IndexVec::<mir::BlockId, Vec<mir::ValueId>>::new();
        for (block_id, block) in function.blocks.iter_enumerated() {
            blocks.push(if reachable.contains(id) && cfg.is_reachable(block_id) {
                output.blocks.push(ir::Block::default())
            } else {
                prologue
            });
            let mut values = live
                .live_in(block_id)
                .iter()
                .filter(|&value| stored(function, value))
                .collect::<Vec<_>>();
            for &inst in &block.instructions {
                if matches!(function.inst(inst).kind, mir::InstKind::Phi(_))
                    && let Some(result) = function.inst_result_value(inst)
                {
                    values.push(result);
                }
            }
            values.sort_unstable();
            values.dedup();
            values.retain(|value| !spills.homes.contains_key(value));
            entries.push(values);
        }
        let entries = entries.into_iter().map(|values| {
            let mut entry = Vec::new();
            if returning.contains(id) {
                entry.push(Slot::ReturnAddress);
            }
            entry.extend(values.into_iter().map(Slot::Value));
            entry
        }).collect();
        let entry = if reachable.contains(id) {
            output.blocks.push(ir::Block::default())
        } else {
            prologue
        };
        layouts.push(FunctionLayout {
            blocks,
            entries,
            entry,
            returning: returning.contains(id),
            live,
            cfg,
            alias: AliasAnalysis::new(function),
            spills,
        });
    }
    plan.finalize().map_err(str::to_owned)?;
    let reads_fmp = plan.max_dynamic_frame_size != 0
        || reachable.iter().any(|id| {
            let function = module.function(id);
            function.instructions().any(|inst| {
                if matches!(function.inst(inst).kind, mir::InstKind::InternalCall { .. }) {
                    return false;
                }
                let effects = layouts[id].alias.instruction_mod_ref(function, inst);
                effects.observes_memory_size()
                    || super::spills::accesses_overlap(
                        effects.reads(),
                        super::storage::FrameAddress::Absolute(EvmMemoryLayout::FMP_SLOT),
                    )
            })
        });
    if reads_fmp {
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
    output.blocks[prologue].terminator = ir::TerminatorKind::Jump(layouts[root].entry).into();
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
        if used_data.contains(id) || (!deployment && module.data_is_emitted_in_runtime(id)) {
            let output_id =
                output.data.push(ir::Data { name: module.data_name(id), bytes: data.to_vec() });
            data_map.insert(id, output_id);
        }
    }
    for id in reachable.iter() {
        let function = module.function(id);
        let context = Context {
            module,
            function,
            storage: &plan.functions[id],
            plan: &plan,
            layout: &layouts[id],
            version,
            optimization,
            deployment,
            data_map: &data_map,
        };
        let mut entry = Vec::new();
        if context.storage.stack_arguments {
            let mut incoming = vec![Slot::ReturnAddress];
            incoming.extend(function.params.indices().rev().map(Slot::Argument));
            let mut desired = Vec::new();
            for &slot in &context.layout.entries[mir::BlockId::ENTRY] {
                desired.push(match slot {
                    Slot::Value(value) => {
                        let mir::Value::Arg(index) = function.value(value) else {
                            return Err("non-argument value is live at function entry".into());
                        };
                        Slot::Argument(*index)
                    }
                    slot => slot,
                });
            }
            // <return label>; <canonical incoming argument values>
            entry.extend(
                Stack::new(incoming).reconcile(&desired, 1, version).map_err(schedule_error)?,
            );
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
            if let mir::Value::Arg(index) = function.value(value)
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
        lower_function(&context, &layouts, &mut output, switches)?;
    }
    Ok(MachineOutput { ir: output, plan })
}

fn lower_function(
    context: &Context<'_>,
    layouts: &IndexVec<mir::FunctionId, FunctionLayout>,
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
        for (position, &inst_id) in block.instructions.iter().enumerate() {
            let instruction = function.inst(inst_id);
            if matches!(instruction.kind, mir::InstKind::Phi(_)) {
                continue;
            }
            let live = |value| layout.live.is_used_at_or_after(value, block_id, position + 1);
            if let mir::InstKind::InternalCall { function: callee, args, returns } =
                &instruction.kind
            {
                let saved = save_writer_homes(context, inst_id, &mut stack, &mut insts, live, args.len() + 5, || true)?;
                let continuation = output.blocks.push(ir::Block::default());
                let caller;
                if !layout.spills.homes.is_empty() {
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
                    prepare(context, &mut stack, &mut insts, args, live)?;
                    caller = stack.values()[..stack.values().len() - args.len()].to_vec();
                    // push <continuation>
                    // <suspended caller>; <continuation>; <argN..arg0>
                    insts.push(ir::InstKind::PushLabel(continuation).into());
                    stack.push(Slot::CallLabel(continuation));
                    let mut desired = caller.clone();
                    desired.push(Slot::CallLabel(continuation));
                    desired.extend(args.iter().rev().copied().map(Slot::Value));
                    insts.extend(
                        stack
                            .reconcile(&desired, prefix(context), context.version)
                            .map_err(schedule_error)?,
                    );
                }
                enter_call(
                    context,
                    &context.plan.functions[*callee],
                    args.len(),
                    current,
                    &mut insts,
                    layouts[*callee].entry,
                    output,
                )?;
                current = continuation;
                stack = Stack::new(caller);
                stack.truncate(stack.values().len() - saved.tracked);
                restore_writer_homes(context, &saved, &mut insts, *returns != 0)?;
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
                let operands = instruction.kind.operands();
                let saved = save_writer_homes(
                    context,
                    inst_id,
                    &mut stack,
                    &mut insts,
                    live,
                    operands.len(),
                    || control_live_after(context, block_id, position),
                )?;
                if saved.tracked == 0
                    && !matches!(opcode, op::GAS | op::PC)
                    && op::stack_io(opcode).is_some_and(|(_, outputs)| outputs == 1)
                    && let Some(preferred) = preferred_branch_values(context, block_id, position, function.inst_result_value(inst_id)) {
                    let operands = operands.iter().copied().map(Slot::Value).collect::<Vec<_>>();
                    materialize(context, &mut stack, &mut insts, &operands)?;
                    // <fixed prefix>; <live values in successor order>; <opcode operands>
                    insts.extend(super::entry_layout::prepare(&mut stack, &operands, &preferred, prefix(context), context.version, opcode, |slot| match slot {
                        Slot::Value(value) => stored(function, value) && live(value),
                        Slot::ReturnAddress | Slot::Protected(_) => true,
                        Slot::CallLabel(_) | Slot::Argument(_) => false,
                    }).map_err(schedule_error)?);
                } else {
                    prepare(context, &mut stack, &mut insts, &operands, live)?;
                }
                // <operands in pop order>
                // opcode
                insts.push(ir::InstKind::Op(opcode).into());
                stack.truncate(stack.values().len() - operands.len());
                stack.truncate(stack.values().len() - saved.tracked);
                restore_writer_homes(
                    context,
                    &saved,
                    &mut insts,
                    op::stack_io(opcode).is_some_and(|(_, outputs)| outputs == 1),
                )?;
                record_result(
                    context,
                    inst_id,
                    &mut stack,
                    &mut insts,
                    op::stack_io(opcode).is_some_and(|(_, outputs)| outputs == 1),
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
        }
        let term = block.terminator.as_ref().ok_or("unterminated MIR block")?;
        let terminator = match term {
            mir::Terminator::Jump(target) => {
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
                if !layout.spills.homes.is_empty() {
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
                if layouts[*callee].returning {
                    return Err(format!("tail-call target `{}` from `{}` requires a returning activation", context.module.function(*callee).name, function.name));
                }
                enter_call(
                    context,
                    &context.plan.functions[*callee],
                    args.len(),
                    current,
                    &mut insts,
                    layouts[*callee].entry,
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
    if setup.guard.is_empty() {
        // <frame argument stores>
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
    Ok(())
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
        insts.extend(calls::address(
            context.storage.return_address(0).map_err(str::to_owned)?,
        ));
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
    addresses: Vec<super::storage::FrameAddress>,
    tracked: usize,
}

fn control_live_after(context: &Context<'_>, block: mir::BlockId, position: usize) -> bool {
    if context.layout.returning {
        return true;
    }
    let calls = |instructions: &[mir::InstId]| {
        instructions.iter().any(|&inst| {
            matches!(context.function.inst(inst).kind, mir::InstKind::InternalCall { .. })
        })
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
    inst: mir::InstId,
    stack: &mut Stack<Slot>,
    output: &mut Vec<ir::Instruction>,
    live: impl Fn(mir::ValueId) -> bool,
    operands: usize,
    control_live: impl FnOnce() -> bool,
) -> Result<SavedHomes, String> {
    let effects = context.layout.alias.instruction_mod_ref(context.function, inst);
    if !effects.writes_space(crate::analysis::AddressSpace::Memory) {
        return Ok(SavedHomes { addresses: Vec::new(), tracked: 0 });
    }
    let control_live = context.plan.max_dynamic_frame_size != 0 && control_live();
    let mut homes = context.layout.spills.homes.iter().filter_map(|(&value, &home)| live(value).then_some(home)).collect::<Vec<_>>();
    homes.sort_unstable();
    homes.dedup();
    let mut addresses = Vec::new();
    for home in homes {
        let address = context.storage.spill_address(home).map_err(str::to_owned)?;
        if super::spills::may_overlap(&effects, address) { addresses.push(address); }
    }
    if control_live && context.storage.base == super::storage::FrameBase::Dynamic && !context.storage.stack_arguments {
        for offset in [super::storage::PREVIOUS_FRAME_OFFSET, super::storage::SAVED_FMP_OFFSET] {
            let address = super::storage::FrameAddress::Relative(offset);
            if super::spills::may_overlap(&effects, address) { addresses.push(address); }
        }
    }
    let frame_pointer = super::storage::FrameAddress::Absolute(EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT);
    if control_live && context.plan.max_dynamic_frame_size != 0 && super::spills::may_overlap(&effects, frame_pointer) {
        addresses.push(frame_pointer);
    }
    let tracked = if context.layout.spills.homes.is_empty() { addresses.len() } else { 0 };
    let saved = SavedHomes { addresses, tracked };
    if saved.addresses.is_empty() { return Ok(saved); }
    if saved.addresses.len() + operands + stack.values().len() + 3 > 1024 {
        return Err("live values across a memory writer exceed the EVM stack limit".into());
    }
    // <activation return label>; <opaque saved words>; <saved frame pointer>
    if !context.layout.spills.homes.is_empty() {
        let base = stack.values()[..prefix(context)].to_vec();
        output.extend(stack.reconcile(&base, prefix(context), context.version).map_err(schedule_error)?);
    }
    for (index, &address) in saved.addresses.iter().enumerate() {
        output.extend(calls::address(address));
        output.push(ir::InstKind::Op(op::MLOAD).into());
        if tracked != 0 { stack.push(Slot::Protected(index)); }
    }
    Ok(saved)
}

fn restore_writer_homes(context: &Context<'_>, saved: &SavedHomes, output: &mut Vec<ir::Instruction>, result: bool) -> Result<(), String> {
    for &address in saved.addresses.iter().rev() {
        // <optional result>; mstore(protected_address, saved_value)
        if result { output.push(ir::InstKind::Swap(1).into()); }
        output.extend(calls::address(address));
        output.push(ir::InstKind::Op(op::MSTORE).into());
    }
    let _ = context;
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

fn stored(function: &mir::Function, value: mir::ValueId) -> bool {
    matches!(function.value(value), mir::Value::Inst(_))
        || (matches!(function.value(value), mir::Value::Arg(_)) && !external_argument(function))
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
                Slot::Value(value) => {
                    stored(context.function, value)
                        && !context.layout.spills.homes.contains_key(&value)
                        && live(value)
                }
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
    if !context.layout.spills.homes.is_empty() {
        let mut insts = Vec::new();
        let desired = stack.values()[..prefix(context)].to_vec();
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

fn edge_values(context: &Context<'_>, from: mir::BlockId, to: mir::BlockId) -> Result<Vec<Slot>, String> {
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

fn preferred_branch_values(context: &Context<'_>, block: mir::BlockId, position: usize, result: Option<mir::ValueId>) -> Option<Vec<Slot>> {
    let source = &context.function.blocks[block];
    if position + 1 != source.instructions.len() || !context.layout.spills.homes.is_empty() {
        return None;
    }
    let mir::Terminator::Branch { condition, then_block, else_block } = source.terminator.as_ref()? else { return None };
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
        .bytes() as u8;
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
