//! Translation validation of stack scheduling: SMT queries that the EVM IR scheduling emits for
//! each MIR block does what the block does.
//!
//! With `-Zvalidate-backend=DIR`, the stack scheduler records a [`ScheduleMap`] of the runtime it
//! lowers: for each MIR block, where its code starts and which value each stack word holds there,
//! and where each spill slot lies. After scheduling, [`write_queries`] writes one SMT-LIB query per
//! MIR block to `DIR` whose satisfiability means the block's EVM IR can end differently from the
//! MIR block, from a start where the stack and memory hold the block's values where the map says;
//! `unsat` proves the block lowered faithfully. A block the encoding does not cover gets a `.skip`
//! file with the reason. `tools/prove-analyses --validate-backend` compiles a corpus this way and
//! reports the verdicts under the pass name `stack-schedule`, per block and per function.
//!
//! The check is a simulation: the map is the relation between a MIR state and an EVM state at each
//! block boundary, so a loop needs no invariant of its own. Proving every block of a function
//! proves, by induction over executions, that the function's EVM IR ends as the MIR function does
//! from every entry the map relates, given the trusted facts below. The scheduler's choices
//! (layouts, shuffles, spills, rematerialization, rebuilt values, trampolines, switch lowering,
//! instruction selection) are checked by what they emit, not trusted.
//!
//! # Encoding
//!
//! A query relates one MIR block to the EVM IR that runs from the block's start until control
//! reaches the start of another MIR block, leaves the function, or calls one. Both start from one
//! shared state: memory, storage, transient storage and the world, as in `validate_passes`, whose
//! encoder both sides use, so a computation they share is one term. The block's live values are
//! terms: a value that pure opcodes or calldata compute is the same computation over its operands,
//! since an SSA value's operands keep their values wherever it is live; a phi, an argument or any
//! other value is a symbol, bounded by its type. The EVM stack starts as the block's layout, the
//! function's words from the bottom, over symbolic words of the caller. Its memory is the MIR
//! memory with each spill slot holding its value when the value is live at the block, or is a phi
//! of the block its layout leaves out, which the predecessors store to the slot; any other slot
//! holds an unknown word. Across a call, the slots of the values the block still reads keep their
//! words.
//!
//! The MIR block runs as in `validate_passes`. The EVM IR runs every path from the block's start:
//! stack operations move terms, pure opcodes use the same exact semantics, and other opcodes the
//! same uninterpreted functions, named by the effect the MIR operation schema declares for the
//! opcode. An internal call, the push of a return label and a jump to a function's entry, is
//! matched with the MIR block's call of the same number: the query asks whether the arguments,
//! storage, transient storage, world, or memory outside the spill slots differ there, and both
//! sides then continue from the same world after the call, the EVM stack below the arguments kept
//! and the callee's results on top. A path ends where control reaches the start of a MIR block, a
//! return jumps to the return address, a tail call leaves, or a terminal opcode stops.
//!
//! The query asserts that some MIR outcome and some EVM path end differently under their guards:
//!
//! - at a block start: the EVM path reaches another block than the MIR edge, or the stack it has
//!   there differs from the successor's layout, in height, in the caller's words, or in a word the
//!   layout names (a phi takes its incoming value), or a spill slot of a value live there does not
//!   hold it, or memory outside the spill slots, storage, transient storage or the world differ;
//! - at an internal return: the results, storage, transient storage, the world, or memory outside
//!   the spill slots;
//! - at an end of the message call: as `validate_passes` compares them;
//! - in kind, when one side returns, calls or stops where the other does not.
//!
//! The queries take on trust, and list in their headers:
//!
//! - `value-type`: a value fits its type, as the MIR verifier and every pass keep it;
//! - `calling-convention`: a callee keeps the words below its arguments, and the caller's spill
//!   slots unless the scheduler marks the call as one that may overwrite them; it leaves its
//!   results where its arguments and the return address were;
//! - `spill-area-private`: a memory load of the MIR block reads none of the function's spill slots,
//!   which only the backend addresses: pointers reach allocations, frame objects and memory the
//!   function was given, all of which lie apart from the spill frame of a running function;
//! - `backend-placement`: a frame object, an allocation or the heap floor is at the address the
//!   backend resolved for it, which the MIR side reads from the scheduled EVM IR;
//! - the facts of `validate_passes` its encoder adds, such as `memory-gas` and `result-type`.
//!
//! # Limits
//!
//! - Only runtime code is validated; the constructor and the deployment prefix are not.
//! - Functions that publish multi-word results to memory, call functions with several results, or
//!   read program data or immutables are skipped, as are MIR operations the encoder does not model.
//! - A plan made for a rebuilt copy of a function is validated against that copy: the rebuild
//!   itself, which recomputes pure values, is not.
//! - Values a block reads through the world, such as `gas`, that the scheduler recomputes after a
//!   call give a counterexample through uninterpreted functions, marked opaque.
//! - Validation checks equivalence; gas is not compared.
//!
//! The map and the queries cost nothing without the flag: the scheduler records the map only with
//! it, after each decision, and the recording never changes what is emitted.

use super::{
    ir,
    schedule_map::{FunctionMap, Location, Position, ScheduleMap},
};
use crate::{
    backend::evm::{
        op::{self, StackOp},
        select::InstKindOpcode,
    },
    mir::{
        BlockId, Callee, EffectKind, Function, FunctionId, InstId, InstKind, Module, PtrLocation,
        Semantics, Terminator, Value, ValueId,
        analysis::{Liveness, integers::integer_mask},
        memory::EvmMemoryLayout,
        semantics::opcode_samples,
        transform::validate_passes::{
            BOOL, Encoder, Kind, MEMORY, Outcome, Query, Run, STORAGE, State, WORD, WORLD, all,
            and, any, arg_name, copy_destination, fmp_slot, query_files, sanitize, word,
        },
    },
};
use alloy_primitives::U256;
use solar_data_structures::map::{FxHashMap, FxHashSet};
use solar_sema::Gcx;
use std::{
    fmt::Write as _,
    hash::{DefaultHasher, Hash, Hasher},
    io::Write as _,
    path::Path,
};

/// The pass name the queries report under.
const PASS: &str = "stack-schedule";

/// Caller words modeled below a function's own stack words.
const CALLER_WORDS: usize = 64;

/// Most EVM paths one block's code may take before the block is skipped.
const MAX_PATHS: usize = 256;

/// Most EVM instructions one block's paths may run before the block is skipped.
const MAX_STEPS: usize = 1 << 16;

/// Writes a query to `dir`, or stdout when `dir` is `-`, for every block of every function `map`
/// describes, relating the block in `module` to its code in `program`, the runtime EVM IR as
/// scheduling emitted it.
pub(crate) fn write_queries(
    gcx: Gcx<'_>,
    module: &Module,
    map: &ScheduleMap,
    program: &ir::Module,
    dir: &Path,
) -> std::io::Result<()> {
    let to_stdout = dir == Path::new("-");
    if !to_stdout {
        std::fs::create_dir_all(dir)?;
    }
    let (faulty_map, faulty_program);
    let (map, program) = match gcx.sess.opts.unstable.validate_backend_fault.as_deref() {
        Some("layout") => {
            faulty_map = with_swapped_layout(map);
            (&faulty_map, program)
        }
        Some("opcode") => {
            faulty_program = with_sub_for_add(program);
            (map, &faulty_program)
        }
        _ => (map, program),
    };
    let cx = Context::new(gcx, module, map, program);
    let module_name = module.name.as_str();
    let mut stdout = String::new();
    for function in &map.functions {
        let func = function.rebuilt.as_deref().unwrap_or(&module.functions[function.function]);
        let name = module.functions[function.function].name.to_string();
        let id = function.function.index();
        let slots = cx.slots(function);
        let liveness = Liveness::compute_live_sets(func);
        for (block, entry) in function.blocks.iter_enumerated() {
            if entry.is_none() {
                continue;
            }
            let header = format!(
                "; module: {module_name}\n; function: #{id} {name}\n; block: {block:?}\n; pass: {PASS}\n; claim: {PASS} lowers #{id} {name} {block:?}\n"
            );
            let result = match (&slots, function.unsupported) {
                (_, Some(reason)) => Err(reason.to_owned()),
                (Err(reason), _) => Err(reason.clone()),
                (Ok(slots), None) => {
                    BlockQuery::new(&cx, function, func, &liveness, slots, block).encode()
                }
            };
            let mut hasher = DefaultHasher::new();
            match &result {
                Ok(query) => query.text.hash(&mut hasher),
                Err(reason) => reason.hash(&mut hasher),
            }
            let name = format!(
                "{}-stack_schedule_f{id}-b{}-{:012x}",
                sanitize(module_name),
                block.index(),
                hasher.finish() & 0xffff_ffff_ffff
            );
            for (file, text) in query_files(&name, &header, result) {
                if to_stdout {
                    stdout.push_str(&format!("; query: {file}\n{text}"));
                } else {
                    std::fs::write(dir.join(file), text)?;
                }
            }
        }
    }
    if to_stdout {
        std::io::stdout().lock().write_all(stdout.as_bytes())?;
    }
    Ok(())
}

/// What every query of an artifact shares.
struct Context<'a> {
    /// Whether the target has return data, which a revert that forwards it copies.
    returndata: bool,
    module: &'a Module,
    program: &'a ir::Module,
    /// The function each entry block starts.
    entries: FxHashMap<ir::BlockId, &'a FunctionMap>,
    /// The effect, mnemonic and copy destination operand of each opcode the MIR operation schema
    /// declares.
    effects: FxHashMap<u8, (EffectKind, String, Option<usize>)>,
}

impl<'a> Context<'a> {
    fn new(
        gcx: Gcx<'_>,
        module: &'a Module,
        map: &'a ScheduleMap,
        program: &'a ir::Module,
    ) -> Self {
        let entries =
            map.functions.iter().filter_map(|function| Some((function.entry?, function))).collect();
        let mut effects = FxHashMap::default();
        for kind in opcode_samples() {
            let Some(Semantics::Opcode(mnemonic, values)) = kind.semantics() else { continue };
            let Some(opcode) = op::from_mnemonic(mnemonic) else { continue };
            let destination = copy_destination(&kind)
                .and_then(|destination| values.iter().position(|&value| value == destination));
            effects.entry(opcode).or_insert((kind.effect_kind(), mnemonic.to_owned(), destination));
        }
        let returndata = gcx.sess.opts.evm_version.supports_returndata();
        Self { returndata, module, program, entries, effects }
    }

    /// The instruction at `position`.
    fn inst(&self, (block, index): Position) -> Option<&'a ir::Instruction> {
        self.program.blocks.get(block)?.instructions.get(index)
    }

    /// The address of each spill slot of `function`, read from the pushes of its address.
    fn slots(&self, function: &FunctionMap) -> Result<Vec<(Location, U256)>, String> {
        let mut slots: Vec<(Location, U256)> = Vec::new();
        for &(location, position) in &function.slots {
            let address = self
                .inst(position)
                .and_then(ir::Instruction::concrete_immediate)
                .ok_or("spill slot address is not a constant")?;
            match slots.iter().find(|(other, _)| *other == location) {
                Some(&(_, other)) if other != address => {
                    return Err("spill slot with two addresses".into());
                }
                Some(_) => {}
                None => slots.push((location, address)),
            }
        }
        Ok(slots)
    }
}

/// A word of the symbolic EVM stack.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Word {
    Term(String),
    /// The address of a block, pushed for a jump or a return.
    Label(ir::BlockId),
}

/// A MIR call of the block, in order, which the EVM IR's call of the same number must match.
struct MirCall {
    callee: FunctionId,
    inst: Option<InstId>,
    guard: String,
    arguments: Vec<String>,
    before: State,
    world: String,
    /// The values the block still reads after the call, whose spill slots the call keeps.
    live: FxHashSet<ValueId>,
}

/// How a MIR outcome or an EVM path ends.
enum End {
    /// At the start of a block: for MIR the successor with the edge's condition and the values its
    /// layout names, for EVM the position reached with the stack there.
    Block {
        block: BlockId,
        condition: String,
    },
    Position {
        position: Position,
        stack: Vec<Word>,
    },
    /// An outcome of `validate_passes`.
    Outcome(Kind),
    /// An EVM path that cannot match any MIR outcome, for the reason given.
    Mismatch(&'static str),
}

struct Ending {
    guard: String,
    end: End,
    state: State,
}

/// An EVM path under construction.
#[derive(Clone)]
struct Trace {
    position: Position,
    stack: Vec<Word>,
    state: State,
    guard: String,
    calls: usize,
}

struct BlockQuery<'a, 'b> {
    cx: &'b Context<'a>,
    function: &'b FunctionMap,
    func: &'b Function,
    liveness: &'b Liveness,
    slots: &'b [(Location, U256)],
    block: BlockId,
    encoder: Encoder,
    /// The caller's words below the function's, bottom first.
    caller: Vec<String>,
    /// The values of the block's phis, which are its inputs like its live-in values.
    inputs: FxHashSet<ValueId>,
    differences: Vec<String>,
    junk: usize,
}

impl<'a, 'b> BlockQuery<'a, 'b> {
    fn new(
        cx: &'b Context<'a>,
        function: &'b FunctionMap,
        func: &'b Function,
        liveness: &'b Liveness,
        slots: &'b [(Location, U256)],
        block: BlockId,
    ) -> Self {
        Self {
            cx,
            function,
            func,
            liveness,
            slots,
            block,
            encoder: Encoder::default(),
            caller: Vec::new(),
            inputs: FxHashSet::default(),
            differences: Vec::new(),
            junk: 0,
        }
    }

    fn encode(mut self) -> Result<Query, String> {
        let func = self.func;
        let map = self.function.blocks[self.block].as_ref().ok_or("block without a layout")?;
        let start = State {
            memory: self.encoder.declare("mem0", MEMORY),
            storage: self.encoder.declare("sto0", STORAGE),
            transient: self.encoder.declare("tra0", STORAGE),
            world: self.encoder.declare("world0", WORLD),
        };
        let mut run = Run {
            func,
            funcs: &self.cx.module.functions,
            values: FxHashMap::default(),
            ends: FxHashMap::default(),
            outcomes: Vec::new(),
        };
        for &inst in &func.blocks[self.block].instructions {
            if let InstKind::Phi(_) = func.inst(inst).kind
                && let Some(value) = func.inst_result_value(inst)
            {
                self.inputs.insert(value);
            }
        }

        // The EVM start: the caller's words, the layout, and the spill slots.
        self.caller = (0..CALLER_WORDS).map(|index| format!("caller{index}")).collect();
        let mut stack: Vec<Word> = self.caller.iter().cloned().map(Word::Term).collect();
        for location in map.layout.clone() {
            let term = match location {
                Location::Value(value) => self.live(&mut run, value)?,
                Location::Ret => self.encoder.declare("ret", WORD),
                Location::Junk => self.fresh("junk"),
            };
            stack.push(Word::Term(term));
        }
        let mut memory = start.memory.clone();
        for &(location, address) in self.slots {
            let held = match location {
                Location::Value(value)
                    if self.block != BlockId::ENTRY && self.in_slot(self.block, value) =>
                {
                    self.live(&mut run, value)?
                }
                Location::Ret if self.block != BlockId::ENTRY => self.encoder.declare("ret", WORD),
                _ => self.fresh("slot"),
            };
            memory = self.encoder.store(&memory, &word(address), &held, 32);
        }
        let evm_start = State { memory, ..start.clone() };

        // The MIR block.
        let (mir, calls) = self.run_mir(&mut run, &start)?;
        // The EVM paths.
        let first = match map.continues {
            Some(target) => Err(target),
            None => Ok(map.start.ok_or("block without code")?),
        };
        let evm = match first {
            Err(target) => vec![Ending {
                guard: "true".into(),
                end: End::Position { position: self.final_start(target)?, stack },
                state: evm_start,
            }],
            Ok(position) => self.run_evm(
                Trace { position, stack, state: evm_start, guard: "true".into(), calls: 0 },
                &calls,
            )?,
        };

        for m in &mir {
            for e in &evm {
                let guard = match &m.end {
                    End::Block { condition, .. } => and(&m.guard, condition),
                    _ => m.guard.clone(),
                };
                let differ = self.differ(&run, m, e)?;
                if differ != "false" {
                    self.differences.push(all(&[
                        guard.as_str(),
                        e.guard.as_str(),
                        differ.as_str(),
                    ]));
                }
            }
        }
        let differences = std::mem::take(&mut self.differences);
        let _ = writeln!(self.encoder.assertions, "(assert {})", any(&differences));
        // Only the caller's words the query names need a declaration.
        for caller in &self.caller {
            let named = [" ", ")"]
                .iter()
                .any(|end| self.encoder.assertions.contains(&format!(" {caller}{end}")));
            if named {
                self.encoder.declare(caller, WORD);
            }
        }
        Ok(self.encoder.finish())
    }

    fn fresh(&mut self, prefix: &str) -> String {
        self.junk += 1;
        self.encoder.declare(&format!("{prefix}{}", self.junk), WORD)
    }

    /// The values defined before `inst` that the block reads after it, or that live on past the
    /// block.
    fn live_after(&self, inst: InstId) -> FxHashSet<ValueId> {
        let func = self.func;
        let data = &func.blocks[self.block];
        let at = data.instructions.iter().position(|&other| other == inst).unwrap_or(0);
        let later: FxHashSet<ValueId> = data.instructions[at..]
            .iter()
            .filter_map(|&inst| func.inst_result_value(inst))
            .collect();
        let mut live: FxHashSet<ValueId> = self.liveness.live_out(self.block).iter().collect();
        for &inst in &data.instructions[at + 1..] {
            live.extend(func.inst(inst).kind.operands().iter().copied());
        }
        if let Some(terminator) = &data.terminator {
            live.extend(terminator.operands().iter().copied());
            for succ in terminator.successors() {
                for &inst in &func.blocks[succ].instructions {
                    if let InstKind::Phi(arms) = &func.inst(inst).kind {
                        live.extend(
                            arms.iter()
                                .filter(|&&(pred, _)| pred == self.block)
                                .map(|&(_, value)| value),
                        );
                    }
                }
            }
        }
        live.retain(|value| !later.contains(value));
        live
    }

    /// Whether `value`'s spill slot holds it at the start of `block`: a value live there that the
    /// block does not define, or a phi of the block that its layout leaves out, which its
    /// predecessors store to the slot.
    fn in_slot(&self, block: BlockId, value: ValueId) -> bool {
        if self.live_at(block, value) {
            return true;
        }
        let is_phi = self.func.blocks[block].instructions.iter().any(|&inst| {
            matches!(self.func.inst(inst).kind, InstKind::Phi(_))
                && self.func.inst_result_value(inst) == Some(value)
        });
        is_phi
            && self.function.blocks[block]
                .as_ref()
                .is_some_and(|map| !map.layout.contains(&Location::Value(value)))
    }

    /// Whether `value` is live at the start of `block` and not one of its phis.
    fn live_at(&self, block: BlockId, value: ValueId) -> bool {
        self.liveness.live_in(block).contains(value)
            && !self.func.blocks[block].instructions.iter().any(|&inst| {
                matches!(self.func.inst(inst).kind, InstKind::Phi(_))
                    && self.func.inst_result_value(inst) == Some(value)
            })
    }

    /// The term of a value the block does not define: a constant, the same computation for a
    /// value pure opcodes or calldata compute, and otherwise a symbol bounded by its type.
    fn live(&mut self, run: &mut Run<'_>, value: ValueId) -> Result<String, String> {
        if let Some(term) = run.values.get(&value) {
            return Ok(term.clone());
        }
        let func = self.func;
        let term = match *func.value(value) {
            _ if func.value_u256(value).is_some() => return run.term(value),
            Value::Arg(arg) => {
                let name = arg_name(arg);
                if !self.encoder.values.contains_key(&name) {
                    self.encoder.declare(&name, WORD);
                    self.bound(&name, value);
                }
                name
            }
            Value::Inst(inst) if !self.inputs.contains(&value) && stable(&func.inst(inst).kind) => {
                for operand in func.inst(inst).kind.operands() {
                    if func.value_u256(operand).is_none() {
                        let term = self.live(run, operand)?;
                        run.values.insert(operand, term);
                    }
                }
                let mut scratch = State {
                    memory: "mem0".into(),
                    storage: "sto0".into(),
                    transient: "tra0".into(),
                    world: "world0".into(),
                };
                let mut guard = "true".to_owned();
                self.encoder.inst(run, inst, &mut scratch, &mut guard)?;
                run.values.get(&value).cloned().ok_or("stable value without a term")?
            }
            Value::Inst(_) => {
                let name = self.encoder.declare(&format!("v{}", value.index()), WORD);
                self.bound(&name, value);
                name
            }
            _ => return Err("a value that is not a word".into()),
        };
        run.values.insert(value, term.clone());
        Ok(term)
    }

    /// Asserts that the symbol `name` fits the type of `value`.
    fn bound(&mut self, name: &str, value: ValueId) {
        let Some(bits) = self.func.value_ty(value).and_then(|ty| ty.integer_bits()) else { return };
        if bits < 256 {
            self.encoder.axioms.insert("value-type");
            self.encoder.values.insert(name.to_owned(), (U256::ZERO, integer_mask(bits)));
            let _ = writeln!(
                self.encoder.assertions,
                "(assert (bvule {name} {}))",
                word(integer_mask(bits))
            );
        } else {
            self.encoder.values.entry(name.to_owned()).or_insert((U256::ZERO, U256::MAX));
        }
    }

    /// Runs the MIR block from `start`, returning its outcomes and its calls in order.
    fn run_mir(
        &mut self,
        run: &mut Run<'_>,
        start: &State,
    ) -> Result<(Vec<Ending>, Vec<MirCall>), String> {
        let func = self.func;
        let block = self.block;
        let data = &func.blocks[block];
        // Every value the block and its successors read, defined before the block.
        let mut reads = Vec::new();
        for &inst in &data.instructions {
            if !matches!(func.inst(inst).kind, InstKind::Phi(_)) {
                reads.extend(func.inst(inst).kind.operands().iter().copied());
            }
        }
        if let Some(terminator) = &data.terminator {
            reads.extend(terminator.operands().iter().copied());
        }
        for &value in &reads {
            if !defined_in(func, value, block) || self.inputs.contains(&value) {
                let term = self.live(run, value)?;
                run.values.insert(value, term);
            }
        }
        let mut state = start.clone();
        let mut guard = "true".to_owned();
        let mut calls = Vec::new();
        for &inst in &data.instructions {
            let kind = &func.inst(inst).kind;
            match *kind {
                InstKind::Phi(_) => {}
                InstKind::InternalFrameAddr(_) | InstKind::HeapFloor | InstKind::Alloc { .. } => {
                    let term = self.placement(run, inst, &mut state)?;
                    let value = func.inst_result_value(inst).ok_or("placement without a result")?;
                    run.values.insert(value, term);
                }
                InstKind::LibraryAddress(library) => {
                    let term = self.encoder.declare(&format!("library{}", library.index()), WORD);
                    let value = func.inst_result_value(inst).ok_or("address without a result")?;
                    run.values.insert(value, term);
                }
                InstKind::ICall { function: Callee::Function(callee), .. } => {
                    if self.cx.module.functions[callee].return_components().len() > 1 {
                        return Err("call with several results".into());
                    }
                    let arguments =
                        kind.operands().iter().map(|&v| run.term(v)).collect::<Result<_, _>>()?;
                    let before = state.clone();
                    let call_guard = guard.clone();
                    self.encoder.inst(run, inst, &mut state, &mut guard)?;
                    calls.push(MirCall {
                        callee,
                        inst: Some(inst),
                        guard: call_guard,
                        arguments,
                        before,
                        world: state.world.clone(),
                        live: self.live_after(inst),
                    });
                }
                InstKind::LoadImmutable(_)
                | InstKind::DataSize(_)
                | InstKind::DataCopy(..)
                | InstKind::ConstructorArgsBase
                | InstKind::ConstructorArgsEnd => {
                    return Err(format!("`{}` is not modeled", kind.mnemonic()));
                }
                InstKind::Load(pointer, PtrLocation::Memory) => {
                    // MIR code reads memory it allocated or was given, never the spill frame the
                    // backend keeps for itself.
                    let pointer = run.term(pointer)?;
                    let mut apart = Vec::new();
                    for &(_, address) in self.slots {
                        apart.push(format!(
                            "(or (bvule (bvadd {pointer} {}) {}) (bvuge {pointer} {}))",
                            word(U256::from(32)),
                            word(address),
                            word(address + U256::from(32))
                        ));
                    }
                    if !apart.is_empty() {
                        self.encoder.axioms.insert("spill-area-private");
                        let fact = all(&apart);
                        let _ = writeln!(self.encoder.assertions, "(assert (=> {guard} {fact}))");
                    }
                    self.encoder.inst(run, inst, &mut state, &mut guard)?;
                }
                _ => self.encoder.inst(run, inst, &mut state, &mut guard)?,
            }
        }
        let mut endings: Vec<Ending> = std::mem::take(&mut run.outcomes)
            .into_iter()
            .map(|Outcome { guard, kind, state }| Ending { guard, end: End::Outcome(kind), state })
            .collect();
        let outcome = |kind: Kind, state: &State, guard: &String| Ending {
            guard: guard.clone(),
            end: End::Outcome(kind),
            state: state.clone(),
        };
        let zero = word(U256::ZERO);
        match data.terminator.as_ref().ok_or("block without a terminator")? {
            Terminator::Jump(_) | Terminator::Branch { .. } | Terminator::Switch { .. } => {
                let mut targets: Vec<BlockId> = Vec::new();
                for target in data.terminator.as_ref().unwrap().successors() {
                    if !targets.contains(&target) {
                        targets.push(target);
                    }
                }
                for target in targets {
                    let condition = self.encoder.edge(run, block, target)?;
                    endings.push(Ending {
                        guard: guard.clone(),
                        end: End::Block { block: target, condition },
                        state: state.clone(),
                    });
                }
            }
            Terminator::Return { values } => {
                let kind = if self.function.returns {
                    Kind::Return(values.iter().map(|&v| run.term(v)).collect::<Result<_, _>>()?)
                } else {
                    // An external body's return stops.
                    Kind::ReturnData { offset: zero.clone(), size: zero }
                };
                endings.push(outcome(kind, &state, &guard));
            }
            &Terminator::ReturnData { offset, size } => {
                let kind = Kind::ReturnData { offset: run.term(offset)?, size: run.term(size)? };
                endings.push(outcome(kind, &state, &guard));
            }
            &Terminator::Revert { offset, size } => {
                let kind = Kind::Revert { offset: run.term(offset)?, size: run.term(size)? };
                endings.push(outcome(kind, &state, &guard));
            }
            Terminator::RevertReturndata => {
                // As the backend lowers it: returndatacopy(0, 0, returndatasize);
                // revert(0, returndatasize).
                let size = if self.cx.returndata {
                    let size = self.opcode(op::RETURNDATASIZE, Vec::new(), &mut state)?;
                    let operands = vec![zero.clone(), zero.clone(), size.clone()];
                    self.opcode(op::RETURNDATACOPY, operands, &mut state)?;
                    size
                } else {
                    zero.clone()
                };
                endings.push(outcome(Kind::Revert { offset: zero, size }, &state, &guard));
            }
            Terminator::Stop => {
                let kind = Kind::ReturnData { offset: zero.clone(), size: zero };
                endings.push(outcome(kind, &state, &guard));
            }
            &Terminator::SelfDestruct { recipient } => {
                endings.push(outcome(Kind::SelfDestruct(run.term(recipient)?), &state, &guard));
            }
            Terminator::Invalid => endings.push(outcome(Kind::Invalid, &state, &guard)),
            Terminator::TailCall { function: callee, args } => {
                let callee = *callee;
                if self.cx.module.functions[callee].return_components().len() > 1 {
                    return Err("tail call with several results".into());
                }
                let arguments: Vec<String> =
                    args.iter().map(|&v| run.term(v)).collect::<Result<_, _>>()?;
                let before = state.clone();
                let name = format!(
                    "icall_{}",
                    sanitize(&self.cx.module.functions[callee].name.to_string())
                );
                let world = self.encoder.event(&name, arguments.clone(), &mut state);
                calls.push(MirCall {
                    callee,
                    inst: None,
                    guard: guard.clone(),
                    arguments,
                    before,
                    world: world.clone(),
                    live: FxHashSet::default(),
                });
                endings.extend(self.after_tail_call(callee, &world, &state, &guard));
            }
        }
        Ok((endings, calls))
    }

    /// The outcomes of a tail call to `callee` that made the world `world`: it ends the execution,
    /// and a callee that returns may instead return its results to this function's caller.
    fn after_tail_call(
        &mut self,
        callee: FunctionId,
        world: &str,
        state: &State,
        guard: &str,
    ) -> Vec<Ending> {
        let returns = self.returns(callee);
        if !returns {
            return vec![Ending {
                guard: guard.to_owned(),
                end: End::Outcome(Kind::Ended),
                state: state.clone(),
            }];
        }
        let ended = self.encoder.apply("ended", &[(world.to_owned(), WORLD)], BOOL);
        let results = (0..self.cx.module.functions[callee].return_components().len())
            .map(|_| self.encoder.apply("result", &[(world.to_owned(), WORLD)], WORD))
            .collect();
        vec![
            Ending {
                guard: self.encoder.define(BOOL, and(guard, &ended)),
                end: End::Outcome(Kind::Ended),
                state: state.clone(),
            },
            Ending {
                guard: self.encoder.define(BOOL, and(guard, &format!("(not {ended})"))),
                end: End::Outcome(Kind::Return(results)),
                state: state.clone(),
            },
        ]
    }

    /// Whether `callee` is entered with a return address.
    fn returns(&self, callee: FunctionId) -> bool {
        self.cx.entries.values().any(|function| function.function == callee && function.returns)
    }

    /// The word the backend resolved for a frame object, an allocation or the heap floor: the
    /// constant its code pushes, or for an allocation that bumps the free-memory pointer, that
    /// bump.
    fn placement(
        &mut self,
        run: &Run<'_>,
        inst: InstId,
        state: &mut State,
    ) -> Result<String, String> {
        let position = self
            .function
            .placements
            .iter()
            .find(|&&(other, _)| other == inst)
            .map(|&(_, position)| position)
            .ok_or("placement without code")?;
        let first = self.cx.inst(position).ok_or("placement outside the program")?;
        self.encoder.axioms.insert("backend-placement");
        if let InstKind::Alloc { size, .. } = self.func.inst(inst).kind
            && first.concrete_immediate() == Some(U256::from(EvmMemoryLayout::FMP_SLOT))
            && self
                .cx
                .inst((position.0, position.1 + 1))
                .is_some_and(|next| next.opcode == op::MLOAD)
        {
            // fmp = mload(0x40); mstore(0x40, fmp + size)
            let pointer = self.encoder.load(&state.memory, &fmp_slot());
            let bytes = run.term(size)?;
            let next = self.encoder.define(WORD, format!("(bvadd {pointer} {bytes})"));
            state.memory = self.encoder.store(&state.memory, &fmp_slot(), &next, 32);
            return Ok(pointer);
        }
        first.concrete_immediate().map(word).ok_or_else(|| "placement is not a constant".into())
    }

    /// The result of an opcode that is not a plain access, by the effect the MIR operation schema
    /// declares for it.
    fn opcode(
        &mut self,
        opcode: u8,
        operands: Vec<String>,
        state: &mut State,
    ) -> Result<String, String> {
        if op::is_pure(opcode) {
            let mnemonic = op::mnemonic(opcode).unwrap_or("op");
            return Ok(self.encoder.pure(opcode, mnemonic, &operands));
        }
        let Some((effect, mnemonic, destination)) = self.cx.effects.get(&opcode).cloned() else {
            return Err(format!("opcode `{}` is not modeled", op::mnemonic(opcode).unwrap_or("?")));
        };
        let destination = destination.and_then(|index| operands.get(index).cloned());
        self.encoder.effect(&mnemonic, Some(opcode), effect, operands, destination, state)
    }

    /// The start of `block`'s code, through the blocks that only jump on.
    fn final_start(&self, mut block: BlockId) -> Result<Position, String> {
        for _ in 0..self.function.blocks.len() {
            let map = self.function.blocks[block].as_ref().ok_or("successor without a layout")?;
            match (map.start, map.continues) {
                (_, Some(next)) => block = next,
                (Some(start), None) => return Ok(start),
                (None, None) => return Err("successor without code".into()),
            }
        }
        Err("blocks that only jump on in a cycle".into())
    }

    /// Runs every EVM path from `path`, matching its calls with the MIR block's `calls`.
    fn run_evm(&mut self, path: Trace, calls: &[MirCall]) -> Result<Vec<Ending>, String> {
        let mut starts: FxHashMap<Position, BlockId> = FxHashMap::default();
        for (block, map) in self.function.blocks.iter_enumerated() {
            if let Some(map) = map
                && map.continues.is_none()
                && let Some(start) = map.start
                && starts.insert(start, block).is_some()
            {
                return Err("two blocks start at one position".into());
            }
        }
        let mut endings = Vec::new();
        let mut work = vec![path];
        let mut steps = 0;
        while let Some(mut path) = work.pop() {
            loop {
                steps += 1;
                if steps > MAX_STEPS {
                    return Err("too many EVM steps".into());
                }
                match self.step(&mut path, calls, &starts, &mut work, &mut endings)? {
                    true => continue,
                    false => break,
                }
            }
            if endings.len() + work.len() > MAX_PATHS {
                return Err("too many EVM paths".into());
            }
        }
        Ok(endings)
    }

    /// Runs one instruction or terminator of `path`. Returns whether the path goes on; a path that
    /// ends adds its ending, and a branch pushes the path it forks.
    fn step(
        &mut self,
        path: &mut Trace,
        calls: &[MirCall],
        starts: &FxHashMap<Position, BlockId>,
        work: &mut Vec<Trace>,
        endings: &mut Vec<Ending>,
    ) -> Result<bool, String> {
        let program = self.cx.program;
        let (block, index) = path.position;
        let data = program.blocks.get(block).ok_or("path leaves the program")?;
        if let Some(inst) = data.instructions.get(index) {
            path.position.1 += 1;
            return self.instruction(inst, path, calls, starts, work, endings);
        }
        let terminator = data.terminator.as_ref().ok_or("block without a terminator")?;
        match &terminator.kind {
            &ir::TerminatorKind::Jump(target) => {
                self.transfer(target, path, calls, starts, endings)
            }
            &ir::TerminatorKind::JumpI { then_block, else_block } => {
                let condition = self.pop_term(path)?;
                let zero = word(U256::ZERO);
                let mut taken = path.clone();
                taken.guard = self
                    .encoder
                    .define(BOOL, and(&path.guard, &format!("(not (= {condition} {zero}))")));
                path.guard =
                    self.encoder.define(BOOL, and(&path.guard, &format!("(= {condition} {zero})")));
                if self.transfer(then_block, &mut taken, calls, starts, endings)? {
                    work.push(taken);
                }
                self.transfer(else_block, path, calls, starts, endings)
            }
            ir::TerminatorKind::IndexedJump(targets) => {
                let index = self.pop_term(path)?;
                let mut outside = Vec::new();
                for (number, &target) in targets.iter().enumerate() {
                    let case = format!("(= {index} {})", word(U256::from(number)));
                    let mut taken = path.clone();
                    taken.guard = self.encoder.define(BOOL, and(&path.guard, &case));
                    if self.transfer(target, &mut taken, calls, starts, endings)? {
                        work.push(taken);
                    }
                    outside.push(format!("(not {case})"));
                }
                // The table has no bounds check: an index past it jumps nowhere valid.
                let guard = self.encoder.define(BOOL, and(&path.guard, &all(&outside)));
                endings.push(Ending {
                    guard,
                    end: End::Outcome(Kind::Invalid),
                    state: path.state.clone(),
                });
                Ok(false)
            }
            &ir::TerminatorKind::Op(opcode) => self.terminal(opcode, path, calls, starts, endings),
        }
    }

    fn pop(&mut self, path: &mut Trace) -> Result<Word, String> {
        if path.stack.len() <= 1 {
            return Err("reads below the modeled caller words".into());
        }
        Ok(path.stack.pop().unwrap())
    }

    fn pop_term(&mut self, path: &mut Trace) -> Result<String, String> {
        match self.pop(path)? {
            Word::Term(term) => Ok(term),
            Word::Label(_) => Err("a block address used as a value".into()),
        }
    }

    fn instruction(
        &mut self,
        inst: &ir::Instruction,
        path: &mut Trace,
        calls: &[MirCall],
        starts: &FxHashMap<Position, BlockId>,
        work: &mut Vec<Trace>,
        endings: &mut Vec<Ending>,
    ) -> Result<bool, String> {
        if let Some(stack_op) = inst.as_stack_op() {
            let len = path.stack.len();
            if stack_op.required_depth() >= len {
                return Err("reads below the modeled caller words".into());
            }
            match stack_op {
                StackOp::Dup(n) => path.stack.push(path.stack[len - n as usize].clone()),
                StackOp::Swap(n) => path.stack.swap(len - 1, len - 1 - n as usize),
                StackOp::Exchange(a, b) => {
                    path.stack.swap(len - 1 - a as usize, len - 1 - b as usize)
                }
                StackOp::Pop => {
                    path.stack.pop();
                }
            }
            return Ok(true);
        }
        if inst.is_encoded_push() || inst.value.is_some() {
            if inst.immutable_push().is_some() {
                return Err("immutable".into());
            }
            let pushed = match inst.value {
                Some(ir::PushValue::Immediate(value)) => Word::Term(word(value)),
                Some(ir::PushValue::Block(block)) => Word::Label(block),
                Some(ir::PushValue::Library(library)) => {
                    Word::Term(self.encoder.declare(&format!("library{}", library.index()), WORD))
                }
                _ => return Err("program data".into()),
            };
            path.stack.push(pushed);
            return Ok(true);
        }
        let opcode = inst.opcode;
        match opcode {
            op::JUMPDEST => Ok(true),
            op::JUMP => self.terminal(opcode, path, calls, starts, endings),
            op::JUMPI => {
                let Word::Label(target) = self.pop(path)? else {
                    return Err("dynamic conditional jump".into());
                };
                let condition = self.pop_term(path)?;
                let zero = word(U256::ZERO);
                let mut taken = path.clone();
                taken.guard = self
                    .encoder
                    .define(BOOL, and(&path.guard, &format!("(not (= {condition} {zero}))")));
                path.guard =
                    self.encoder.define(BOOL, and(&path.guard, &format!("(= {condition} {zero})")));
                if self.transfer(target, &mut taken, calls, starts, endings)? {
                    work.push(taken);
                }
                Ok(true)
            }
            _ => {
                let (inputs, outputs) =
                    op::stack_io(opcode).ok_or("opcode without a stack effect")?;
                let mut operands = Vec::with_capacity(usize::from(inputs));
                for _ in 0..inputs {
                    operands.push(self.pop_term(path)?);
                }
                let guard = path.guard.clone();
                let state = &mut path.state;
                let result = match opcode {
                    op::MLOAD => {
                        self.encoder.access(&guard, &operands[0], 32);
                        self.encoder.load(&state.memory, &operands[0])
                    }
                    op::MSTORE | op::MSTORE8 => {
                        let size = if opcode == op::MSTORE { 32 } else { 1 };
                        self.encoder.access(&guard, &operands[0], size);
                        state.memory =
                            self.encoder.store(&state.memory, &operands[0], &operands[1], size);
                        String::new()
                    }
                    op::SLOAD => self
                        .encoder
                        .define(WORD, format!("(select {} {})", state.storage, operands[0])),
                    op::TLOAD => self
                        .encoder
                        .define(WORD, format!("(select {} {})", state.transient, operands[0])),
                    op::SSTORE => {
                        state.storage = self.encoder.define(
                            STORAGE,
                            format!("(store {} {} {})", state.storage, operands[0], operands[1]),
                        );
                        String::new()
                    }
                    op::TSTORE => {
                        state.transient = self.encoder.define(
                            STORAGE,
                            format!("(store {} {} {})", state.transient, operands[0], operands[1]),
                        );
                        String::new()
                    }
                    op::CALLDATALOAD => self.encoder.apply(
                        "input_calldataload",
                        &[(operands[0].clone(), WORD)],
                        WORD,
                    ),
                    op::KECCAK256 => {
                        let [offset, size] = <[String; 2]>::try_from(operands).unwrap();
                        let memory = state.memory.clone();
                        self.encoder.keccak(&memory, offset, size)
                    }
                    _ => {
                        let mut state = path.state.clone();
                        let result = self.opcode(opcode, operands, &mut state)?;
                        path.state = state;
                        result
                    }
                };
                if outputs == 1 {
                    if result.is_empty() {
                        return Err("opcode without a modeled result".into());
                    }
                    path.stack.push(Word::Term(result));
                }
                Ok(true)
            }
        }
    }

    /// A terminal opcode, or a jump to the address on the stack.
    fn terminal(
        &mut self,
        opcode: u8,
        path: &mut Trace,
        calls: &[MirCall],
        starts: &FxHashMap<Position, BlockId>,
        endings: &mut Vec<Ending>,
    ) -> Result<bool, String> {
        let zero = word(U256::ZERO);
        let kind = match opcode {
            op::JUMP => match self.pop(path)? {
                Word::Label(target) => return self.transfer(target, path, calls, starts, endings),
                Word::Term(term) if term == "ret" && self.function.returns => {
                    let results =
                        self.cx.module.functions[self.function.function].return_components().len();
                    let mut values = Vec::with_capacity(results);
                    for _ in 0..results {
                        values.push(self.pop(path)?);
                    }
                    let mut terms = Vec::new();
                    for value in values {
                        match value {
                            Word::Term(term) => terms.push(term),
                            Word::Label(_) => {
                                return self.mismatch(path, "a block address returned", endings);
                            }
                        }
                    }
                    if !self.keeps_caller_words(&path.stack, 0) {
                        return self.mismatch(
                            path,
                            "return leaves other words below its results",
                            endings,
                        );
                    }
                    Kind::Return(terms)
                }
                Word::Term(_) => return Err("jump to a computed address".into()),
            },
            op::STOP => Kind::ReturnData { offset: zero.clone(), size: zero },
            op::RETURN | op::REVERT => {
                let offset = self.pop_term(path)?;
                let size = self.pop_term(path)?;
                if opcode == op::RETURN {
                    Kind::ReturnData { offset, size }
                } else {
                    Kind::Revert { offset, size }
                }
            }
            op::INVALID => Kind::Invalid,
            op::SELFDESTRUCT => Kind::SelfDestruct(self.pop_term(path)?),
            _ => return Err(format!("terminal `{}`", op::mnemonic(opcode).unwrap_or("?"))),
        };
        endings.push(Ending {
            guard: path.guard.clone(),
            end: End::Outcome(kind),
            state: path.state.clone(),
        });
        Ok(false)
    }

    fn mismatch(
        &mut self,
        path: &Trace,
        reason: &'static str,
        endings: &mut Vec<Ending>,
    ) -> Result<bool, String> {
        endings.push(Ending {
            guard: path.guard.clone(),
            end: End::Mismatch(reason),
            state: path.state.clone(),
        });
        Ok(false)
    }

    /// Whether `stack` holds exactly the caller's words and then `extra` more.
    fn keeps_caller_words(&self, stack: &[Word], extra: usize) -> bool {
        stack.len() == self.caller.len() + extra
            && stack
                .iter()
                .zip(&self.caller)
                .all(|(word, caller)| *word == Word::Term(caller.clone()))
    }

    /// Control reaching the start of EVM block `target`: a call, a tail call, the start of a MIR
    /// block, or code of the same block.
    fn transfer(
        &mut self,
        target: ir::BlockId,
        path: &mut Trace,
        calls: &[MirCall],
        starts: &FxHashMap<Position, BlockId>,
        endings: &mut Vec<Ending>,
    ) -> Result<bool, String> {
        if let Some(&callee) = self.cx.entries.get(&target) {
            return self.call(callee, path, calls, endings);
        }
        let position = (target, 0);
        if starts.contains_key(&position) {
            endings.push(Ending {
                guard: path.guard.clone(),
                end: End::Position { position, stack: path.stack.clone() },
                state: path.state.clone(),
            });
            return Ok(false);
        }
        path.position = position;
        Ok(true)
    }

    /// A jump to the entry of `callee`: a call when a return label is on top, else a tail call.
    fn call(
        &mut self,
        callee: &FunctionMap,
        path: &mut Trace,
        calls: &[MirCall],
        endings: &mut Vec<Ending>,
    ) -> Result<bool, String> {
        let func = &self.cx.module.functions[callee.function];
        let continuation = match path.stack.last() {
            Some(&Word::Label(label)) if callee.returns => Some(label),
            _ => None,
        };
        let tail_return = callee.returns && continuation.is_none();
        if continuation.is_some() || tail_return {
            // The return address: a label for a call, the inherited one for a tail call.
            let ret = self.pop(path)?;
            if tail_return && ret != Word::Term("ret".into()) {
                return self.mismatch(
                    path,
                    "tail call without this function's return address",
                    endings,
                );
            }
        }
        let mut arguments = Vec::with_capacity(func.params.len());
        for _ in 0..func.params.len() {
            match self.pop(path)? {
                Word::Term(term) => arguments.push(term),
                Word::Label(_) => {
                    return self.mismatch(path, "a block address passed as an argument", endings);
                }
            }
        }
        let Some(call) = calls.get(path.calls) else {
            return self.mismatch(path, "a call the MIR block does not make", endings);
        };
        path.calls += 1;
        if call.callee != callee.function || call.arguments.len() != arguments.len() {
            return self.mismatch(path, "a call to another function", endings);
        }
        // The call's inputs must agree.
        let mut parts = Vec::new();
        for (left, right) in call.arguments.iter().zip(&arguments) {
            if left != right {
                parts.push(format!("(not (= {left} {right}))"));
            }
        }
        self.committed(&call.before, &path.state, &mut parts);
        if let Some(memory) = self.memory_differs(&call.before.memory, &path.state.memory) {
            parts.push(memory);
        }
        if !parts.is_empty() {
            let difference = all(&[path.guard.as_str(), call.guard.as_str(), any(&parts).as_str()]);
            self.differences.push(difference);
        }
        // Both continue from the world after the call, with the caller's spill slots kept unless
        // the call may overwrite them.
        self.encoder.axioms.insert("calling-convention");
        let world = call.world.clone();
        let after = [(world.clone(), WORLD)];
        let mut memory = self.encoder.apply("memory_after", &after, MEMORY);
        if call.inst.is_none_or(|inst| !self.function.clobbers.contains(&inst)) {
            for &(location, address) in self.slots {
                let kept = match location {
                    Location::Value(value) => call.live.contains(&value),
                    Location::Ret => true,
                    Location::Junk => false,
                };
                if !kept {
                    continue;
                }
                let address = word(address);
                let held = self.encoder.load(&path.state.memory, &address);
                memory = self.encoder.store(&memory, &address, &held, 32);
            }
        }
        path.state = State {
            memory,
            storage: self.encoder.apply("storage_after", &after, STORAGE),
            transient: self.encoder.apply("transient_after", &after, STORAGE),
            world: world.clone(),
        };
        if call.inst.is_none() || continuation.is_none() {
            // A tail call: the callee ends the execution, or returns to this function's caller
            // with the stack holding the caller's words and its results.
            if tail_return && !self.keeps_caller_words(&path.stack, 0) {
                return self.mismatch(
                    path,
                    "tail call leaves other words below its arguments",
                    endings,
                );
            }
            if continuation.is_some() || call.inst.is_some() {
                return self.mismatch(path, "a call where the MIR block tail calls", endings);
            }
            let guard = path.guard.clone();
            let state = path.state.clone();
            endings.extend(self.after_tail_call(callee.function, &world, &state, &guard));
            return Ok(false);
        }
        let ended = self.encoder.apply("ended", &[(world.clone(), WORLD)], BOOL);
        let guard = self.encoder.define(BOOL, and(&path.guard, &ended));
        endings.push(Ending { guard, end: End::Outcome(Kind::Ended), state: path.state.clone() });
        path.guard = self.encoder.define(BOOL, and(&path.guard, &format!("(not {ended})")));
        for _ in 0..func.return_components().len() {
            let result = self.encoder.apply("result", &[(world.clone(), WORLD)], WORD);
            path.stack.push(Word::Term(result));
        }
        path.position = (continuation.unwrap(), 0);
        Ok(true)
    }

    /// Adds to `parts` the differences in storage, transient storage and the world.
    fn committed(&self, x: &State, y: &State, parts: &mut Vec<String>) {
        for (left, right) in
            [(&x.storage, &y.storage), (&x.transient, &y.transient), (&x.world, &y.world)]
        {
            if left != right {
                parts.push(format!("(not (= {left} {right}))"));
            }
        }
    }

    /// Whether MIR memory `mir` and EVM memory `evm` differ outside the spill slots.
    fn memory_differs(&mut self, mir: &str, evm: &str) -> Option<String> {
        if mir == evm {
            return None;
        }
        let index = self.encoder.declare("index", WORD);
        let mut outside = Vec::new();
        for &(_, address) in self.slots {
            outside.push(format!(
                "(or (bvult {index} {}) (bvuge {index} {}))",
                word(address),
                word(address + U256::from(32))
            ));
        }
        outside.push(format!("(not (= (select {mir} {index}) (select {evm} {index})))"));
        Some(all(&outside))
    }

    /// Whether MIR ending `m` and EVM ending `e` differ, as a Boolean term.
    fn differ(&mut self, run: &Run<'_>, m: &Ending, e: &Ending) -> Result<String, String> {
        let (x, y) = (&m.state, &e.state);
        let mut parts = Vec::new();
        match (&m.end, &e.end) {
            (_, &End::Mismatch(reason)) => {
                let _ = writeln!(self.encoder.assertions, "; EVM path: {reason}");
                return Ok("true".into());
            }
            (&End::Block { block, .. }, End::Position { position, stack }) => {
                if self.final_start(block)? != *position {
                    return Ok("true".into());
                }
                let map =
                    self.function.blocks[block].as_ref().ok_or("successor without a layout")?;
                let layout = &map.layout;
                let below = self.caller.len();
                if map.floating {
                    if stack.len() < layout.len() + 1 {
                        return Ok("true".into());
                    }
                } else if !(stack.len() == below + layout.len()
                    && stack[..below]
                        .iter()
                        .zip(&self.caller)
                        .all(|(word, caller)| *word == Word::Term(caller.clone())))
                {
                    return Ok("true".into());
                }
                let top = &stack[stack.len() - layout.len()..];
                for (location, held) in layout.iter().zip(top) {
                    let expected = match *location {
                        Location::Value(value) => self.edge_value(run, block, value)?,
                        Location::Ret => self.encoder.declare("ret", WORD),
                        Location::Junk => continue,
                    };
                    match held {
                        Word::Term(term) if *term == expected => {}
                        Word::Term(term) => parts.push(format!("(not (= {term} {expected}))")),
                        Word::Label(_) => return Ok("true".into()),
                    }
                }
                for &(location, address) in self.slots {
                    let expected = match location {
                        Location::Value(value)
                            if block != BlockId::ENTRY && self.in_slot(block, value) =>
                        {
                            self.edge_value(run, block, value)?
                        }
                        Location::Ret if block != BlockId::ENTRY => {
                            self.encoder.declare("ret", WORD)
                        }
                        _ => continue,
                    };
                    let held = self.encoder.load(&y.memory, &word(address));
                    if held != expected {
                        parts.push(format!("(not (= {held} {expected}))"));
                    }
                }
                self.committed(x, y, &mut parts);
                parts.extend(self.memory_differs(&x.memory, &y.memory));
            }
            (End::Outcome(left), End::Outcome(right)) => match (left, right) {
                (Kind::Return(left), Kind::Return(right)) if left.len() == right.len() => {
                    for (left, right) in left.iter().zip(right) {
                        if left != right {
                            parts.push(format!("(not (= {left} {right}))"));
                        }
                    }
                    self.committed(x, y, &mut parts);
                    parts.extend(self.memory_differs(&x.memory, &y.memory));
                }
                (
                    Kind::ReturnData { offset: left_offset, size: left_size },
                    Kind::ReturnData { offset: right_offset, size: right_size },
                ) => {
                    parts.extend(
                        self.encoder
                            .data((left_offset, left_size, x), (right_offset, right_size, y)),
                    );
                    self.committed(x, y, &mut parts);
                }
                (
                    Kind::Revert { offset: left_offset, size: left_size },
                    Kind::Revert { offset: right_offset, size: right_size },
                ) => parts.extend(
                    self.encoder.data((left_offset, left_size, x), (right_offset, right_size, y)),
                ),
                (Kind::Ended, Kind::Ended) => {
                    if x.world != y.world {
                        parts.push(format!("(not (= {} {}))", x.world, y.world));
                    }
                }
                (Kind::SelfDestruct(left), Kind::SelfDestruct(right)) => {
                    if left != right {
                        parts.push(format!("(not (= {left} {right}))"));
                    }
                    self.committed(x, y, &mut parts);
                }
                (Kind::Invalid, Kind::Invalid) => {}
                _ => return Ok("true".into()),
            },
            _ => return Ok("true".into()),
        }
        Ok(any(&parts))
    }

    /// The term the stack or a spill slot must hold for `value` on entry to `succ` from this block:
    /// a phi's incoming value, or the value itself.
    fn edge_value(
        &mut self,
        run: &Run<'_>,
        succ: BlockId,
        value: ValueId,
    ) -> Result<String, String> {
        for &inst in &self.func.blocks[succ].instructions {
            if let InstKind::Phi(arms) = &self.func.inst(inst).kind
                && self.func.inst_result_value(inst) == Some(value)
            {
                let &(_, incoming) = arms
                    .iter()
                    .find(|&&(pred, _)| pred == self.block)
                    .ok_or("phi without an incoming value")?;
                return self.term(run, incoming);
            }
        }
        self.term(run, value)
    }

    /// The term of `value` after the block: what the block computed, or what it was on entry.
    fn term(&mut self, run: &Run<'_>, value: ValueId) -> Result<String, String> {
        if let Ok(term) = run.term(value) {
            return Ok(term);
        }
        let mut scratch = Run {
            func: run.func,
            funcs: run.funcs,
            values: run.values.clone(),
            ends: FxHashMap::default(),
            outcomes: Vec::new(),
        };
        self.live(&mut scratch, value)
    }
}

/// Whether the instruction computes its value from its operands and transaction inputs alone, so
/// the same computation gives the value wherever it is live.
fn stable(kind: &InstKind) -> bool {
    match kind {
        InstKind::Load(_, PtrLocation::Calldata) | InstKind::PtrAdd(..) => true,
        InstKind::Phi(_) => false,
        _ => match kind.semantics() {
            Some(Semantics::Opcode(..)) => kind
                .evm_opcode()
                .is_some_and(|opcode| op::is_pure(opcode) || op::is_tx_input(opcode)),
            Some(
                Semantics::Word(_)
                | Semantics::LowBits(..)
                | Semantics::SignExtend(..)
                | Semantics::NotEqual(..)
                | Semantics::Select(..),
            ) => true,
            _ => false,
        },
    }
}

/// Whether `value` is the result of an instruction of `block`.
fn defined_in(func: &Function, value: ValueId, block: BlockId) -> bool {
    let Value::Inst(inst) = *func.value(value) else { return false };
    func.blocks[block].instructions.contains(&inst)
}

/// `map` with the first two words of the first layout that names two values swapped, for
/// `-Zvalidate-backend-fault=layout`.
fn with_swapped_layout(map: &ScheduleMap) -> ScheduleMap {
    let mut map = map.clone();
    let layouts =
        map.functions.iter_mut().flat_map(|function| function.blocks.iter_mut().flatten());
    for block in layouts {
        let mut values = block
            .layout
            .iter()
            .enumerate()
            .filter(|(_, location)| matches!(location, Location::Value(_)))
            .map(|(index, _)| index);
        if let (Some(first), Some(second)) = (values.next(), values.next())
            && block.layout[first] != block.layout[second]
        {
            block.layout.swap(first, second);
            break;
        }
    }
    map
}

/// `program` with its first `add` replaced by `sub`, for `-Zvalidate-backend-fault=opcode`.
fn with_sub_for_add(program: &ir::Module) -> ir::Module {
    let mut program = program.clone();
    if let Some(inst) = program
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
        .find(|inst| !inst.is_encoded_push() && inst.opcode == op::ADD)
    {
        inst.replace_preserving_metadata(ir::Instruction::opcode(op::SUB));
    }
    program
}
