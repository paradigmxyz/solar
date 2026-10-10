//! Runs lowered MIR through the MIR interpreter.
//!
//! [`transact`] runs a transaction: a module's dispatch entry on calldata, with zeroed memory and a
//! [`Host`] answering what the contract reads from its context, such as the caller or a storage
//! slot's value before the transaction. [`call`] runs one internal function on argument words
//! instead, with memory that holds only the free memory pointer. Both take the text of one lowered
//! module, as `-Zdump=mir-final` prints it, and [`parse_dump`] splits such a dump into its
//! modules. [`Options::trace`] receives a line for every operation a run executes.
//!
//! The UI test runner executes every `run-call` directive in an EVM. With `SOLAR_RUN_CALL_MIR` set,
//! it also runs the call through [`transact`] on the MIR the compiler lowered to that bytecode.
//! Both execute the same compiled program, so a disagreement is a bug in the backend or in the
//! interpreter. The `solar-mir-interp` tool runs MIR from the command line.
//!
//! The interpreter models one contract: calls, contract creation, and `gas` end a run as
//! unsupported, and so does a module the compiler dumped before lowering it, as `-O none` does.

use crate::mir::{
    BlockId, Callee, Function, InstId, InstKind, MirPhase, Module, Terminator, Value, ValueId,
    analysis::validate_phase,
    display::{display_instruction, display_terminator, display_val},
    memory::EvmMemoryLayout,
    utils::interp::{
        Effects, Execution as RunExecution, HOST_OPCODES, Limit, Limits, Machine, Memory, Meter,
        Outcome as RunOutcome,
    },
};
use alloy_primitives::U256;
use solar_config::EvmVersion;
use solar_data_structures::{
    index::{IndexVec, index_vec},
    map::FxHashMap,
};
use solar_interface::{ColorChoice, Session, source_map::FileName};
use std::fmt::Write as _;

pub use crate::mir::utils::interp::{Host, Log};

/// How far [`transact`] moves the regions the interpreter places itself on its second run: an odd
/// number of words, so that no alignment hides a moved address.
const MOVED_REGIONS: u64 = 0x2a0;

/// The opcodes whose values a run asks its [`Host`] for, such as `CALLER` and `TIMESTAMP`.
pub const HOST_READS: &[u8] = &HOST_OPCODES;

/// How a run ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The called function returned these words. Only [`call`] ends this way.
    Return(Vec<U256>),
    /// It returned this data with `RETURN`, or stopped with none.
    Success(Vec<u8>),
    /// It reverted with this data.
    Revert(Vec<u8>),
    /// It reached `INVALID` or another exceptional halt.
    Halt,
    /// The interpreter cannot run it, for this reason.
    Unsupported(String),
}

/// What a run did.
#[derive(Clone, Debug)]
pub struct Execution {
    /// How it ended.
    pub outcome: Outcome,
    /// The events it logged, in order.
    pub logs: Vec<Log>,
    /// The persistent storage slots it wrote, with their final values, ordered by slot.
    pub storage: Vec<(U256, U256)>,
    /// The transient storage slots it wrote, with their final values, ordered by slot.
    pub transient: Vec<(U256, U256)>,
}

/// How a run executes.
pub struct Options<'a> {
    /// The EVM version the module targets, which decides what `revert` does in a transaction:
    /// before Byzantium, `REVERT` is an undefined opcode, so it halts.
    pub evm_version: EvmVersion,
    /// Units of work before the run stops: one per operation and per copied, hashed, or returned
    /// word.
    pub fuel: u64,
    /// Calls that may be live at once, the first included.
    pub depth: usize,
    /// Receives a line for every operation the run executes, with its operand values and result.
    pub trace: Option<&'a mut dyn FnMut(&str)>,
}

impl Default for Options<'_> {
    fn default() -> Self {
        Self { evm_version: EvmVersion::default(), fuel: 10_000_000, depth: 1024, trace: None }
    }
}

/// One module of a `-Zdump=mir-final` dump.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DumpedModule {
    /// The contract the module compiles, as the dump's header names it, or the module's name.
    pub name: String,
    /// The module's text.
    pub mir: String,
}

/// Runs a transaction with `calldata` on the contract whose lowered MIR module `mir` holds, asking
/// `host` for its context. Fails when `mir` is not a valid lowered module.
///
/// The backend decides where some memory goes, such as the allocations it places itself and the
/// buffer of a call's further results, so the interpreter picks its own addresses. The
/// transaction runs again with those regions moved, and a result that changes with them is
/// unsupported, since the compiled code would see other addresses.
pub fn transact(
    mir: &str,
    calldata: &[u8],
    host: &mut dyn Host,
    options: Options<'_>,
) -> Result<Execution, String> {
    with_module(mir, |module| {
        let limits = Limits { fuel: options.fuel, depth: options.depth };
        let mut tracer = options.trace.map(|sink| Tracer::new(module, sink));
        let meter: &mut dyn Meter = match &mut tracer {
            Some(tracer) => tracer,
            None => &mut (),
        };
        let machine = Machine::new(module);
        let execution = machine.transact(calldata, host, options.evm_version, 0, limits, meter);
        if let Some(tracer) = &mut tracer {
            tracer.flush();
        }
        let moved =
            machine.transact(calldata, host, options.evm_version, MOVED_REGIONS, limits, &mut ());
        if (&moved.outcome, &moved.effects) != (&execution.outcome, &execution.effects) {
            let outcome = RunOutcome::Unsupported("a result that depends on the memory layout");
            return Ok(execution_from(outcome, Effects::default()));
        }
        Ok(execution_from(execution.outcome, execution.effects))
    })
}

/// Runs internal function `function` of the lowered MIR module `mir` on the words `args`, asking
/// `host` for its context. Memory starts zeroed except for the free memory pointer, which holds
/// the heap start the host reports. Fails when `mir` is not a
/// valid lowered module, it has no such function, or the function reads its arguments from
/// calldata.
pub fn call(
    mir: &str,
    function: &str,
    args: &[U256],
    host: &mut dyn Host,
    options: Options<'_>,
) -> Result<Execution, String> {
    let name = function.strip_prefix('@').unwrap_or(function);
    with_module(mir, |module| {
        let Some((id, body)) =
            module.iter_functions().find(|(_, function)| function.name.to_string() == name)
        else {
            return Err(format!("the module has no function `@{name}`"));
        };
        if body.arg_indices().count() != body.params.len() {
            return Err(format!(
                "`@{name}` reads its arguments from calldata; run it in a transaction"
            ));
        }
        if args.len() != body.params.len() {
            let words = |count: usize| format!("{count} word{}", if count == 1 { "" } else { "s" });
            return Err(format!(
                "`@{name}` takes {}, not {}",
                words(body.params.len()),
                words(args.len())
            ));
        }
        // mstore 0x40, heap start
        let mut memory = Memory::zeroed();
        memory.set(EvmMemoryLayout::FMP_SLOT, host.free_memory_start());
        let limits = Limits { fuel: options.fuel, depth: options.depth };
        let mut tracer = options.trace.map(|sink| Tracer::new(module, sink));
        let meter: &mut dyn Meter = match &mut tracer {
            Some(tracer) => tracer,
            None => &mut (),
        };
        let RunExecution { outcome, effects, .. } =
            Machine::new(module).run(id, args, memory, Some(host), limits, meter);
        if let Some(tracer) = &mut tracer {
            tracer.flush();
        }
        Ok(execution_from(outcome, effects))
    })
}

/// Splits the output of `solar -Zdump=mir-final` into its modules, each after a
/// `// === NAME ===` header. Text without headers is one module, named by its `@module` line. The
/// compiler's JSON output, which follows the dumps, ends the last module.
pub fn parse_dump(text: &str) -> Vec<DumpedModule> {
    let headed = text.lines().any(|line| header(line).is_some());
    let mut modules = Vec::new();
    let mut current = (!headed).then(DumpedModule::default);
    for line in text.lines() {
        if let Some(name) = header(line) {
            modules.extend(current.take());
            current = Some(DumpedModule { name: name.to_owned(), ..Default::default() });
            continue;
        }
        if line.starts_with('{') {
            break;
        }
        let Some(module) = &mut current else { continue };
        if module.name.is_empty()
            && let Some(name) = line.strip_prefix("@module ")
        {
            module.name = name.trim().to_owned();
        }
        module.mir.push_str(line);
        module.mir.push('\n');
    }
    modules.extend(current);
    modules.retain(|module| !module.mir.trim().is_empty());
    modules
}

/// Returns the contract a dump's module header names.
fn header(line: &str) -> Option<&str> {
    line.strip_prefix("// === ").and_then(|rest| rest.strip_suffix(" ==="))
}

/// Parses `mir` as one lowered module and runs `f` on it within a session of its own.
fn with_module(
    mir: &str,
    f: impl FnOnce(&Module) -> Result<Execution, String>,
) -> Result<Execution, String> {
    let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
    sess.enter_sequential(|| {
        let name = FileName::Custom("interpreted.mir".into());
        let file = sess
            .source_map()
            .new_source_file(name, mir)
            .map_err(|error| format!("unreadable MIR: {error}"))?;
        let Ok(module) = Module::parse(&sess, &file) else {
            let diagnostics = sess.dcx.emitted_diagnostics().map(|d| d.to_string());
            return Err(format!("unparsable MIR: {}", diagnostics.unwrap_or_default()));
        };
        if module.phase() != MirPhase::Lowered {
            return Err("the MIR was dumped before lowering, as `-O none` dumps it".into());
        }
        // Run only what the backend would compile.
        if validate_phase(&sess.dcx, &module, MirPhase::Lowered).is_err() {
            let diagnostics = sess.dcx.emitted_diagnostics().map(|d| d.to_string());
            return Err(format!("invalid lowered MIR: {}", diagnostics.unwrap_or_default()));
        }
        f(&module)
    })
}

/// Converts what a run did into what the interpreter reports.
fn execution_from(outcome: RunOutcome, effects: Effects) -> Execution {
    let outcome = match outcome {
        RunOutcome::Return(values) => Outcome::Return(values.to_vec()),
        RunOutcome::ReturnData(data) => Outcome::Success(data),
        RunOutcome::Stop => Outcome::Success(Vec::new()),
        RunOutcome::Revert(data) => Outcome::Revert(data),
        RunOutcome::Invalid => Outcome::Halt,
        RunOutcome::Limit(Limit::Fuel) => Outcome::Unsupported("runs too long".into()),
        RunOutcome::Limit(Limit::Depth) => Outcome::Unsupported("nests calls too deeply".into()),
        RunOutcome::Limit(Limit::Memory) => {
            Outcome::Unsupported("reaches past 16 MiB of memory".into())
        }
        RunOutcome::Unsupported(what) => Outcome::Unsupported(format!("reaches `{what}`")),
    };
    let sorted = |slots: FxHashMap<U256, U256>| {
        let mut slots = slots.into_iter().collect::<Vec<_>>();
        slots.sort_unstable();
        slots
    };
    Execution {
        outcome,
        logs: effects.logs,
        storage: sorted(effects.storage),
        transient: sorted(effects.transient),
    }
}

/// Writes a line for every operation a run executes: the function and block, the operation as
/// MIR text prints it, the values of its operands, and its result. Lines of a call are indented
/// one step deeper than the line of the call.
struct Tracer<'m, 's> {
    module: &'m Module,
    sink: &'s mut dyn FnMut(&str),
    /// Calls live below the running one.
    depth: usize,
    /// The line of an instruction waiting for its result.
    pending: Option<String>,
    /// The block of each instruction, by the address of its function.
    blocks: FxHashMap<usize, IndexVec<InstId, BlockId>>,
}

impl<'m, 's> Tracer<'m, 's> {
    fn new(module: &'m Module, sink: &'s mut dyn FnMut(&str)) -> Self {
        Self { module, sink, depth: 0, pending: None, blocks: FxHashMap::default() }
    }

    /// Writes the waiting line, if any.
    fn flush(&mut self) {
        if let Some(line) = self.pending.take() {
            (self.sink)(&line);
        }
    }

    /// Returns the block holding instruction `inst` of `function`.
    fn block(&mut self, function: &Function, inst: InstId) -> BlockId {
        let blocks = self.blocks.entry(std::ptr::from_ref(function).addr()).or_insert_with(|| {
            let mut blocks = index_vec![BlockId::ENTRY; function.num_insts()];
            for (block, body) in function.blocks.iter_enumerated() {
                for &inst in &body.instructions {
                    blocks[inst] = block;
                }
            }
            blocks
        });
        blocks[inst]
    }

    /// Starts a line about `function` in `block`.
    fn start(&self, function: &Function, block: BlockId) -> String {
        format!("{:indent$}@{} bb{}: ", "", function.name, block.index(), indent = 2 * self.depth)
    }
}

/// Appends the values `read` finds for the operands that are not immediates, each once.
fn write_operands(
    line: &mut String,
    function: &Function,
    operands: impl IntoIterator<Item = ValueId>,
    read: &dyn Fn(ValueId) -> Option<U256>,
) {
    let mut written = Vec::new();
    for operand in operands {
        if written.contains(&operand) || matches!(function.value(operand), Value::Immediate(_)) {
            continue;
        }
        if let Some(value) = read(operand) {
            let separator = if written.is_empty() { "  [" } else { ", " };
            let _ = write!(line, "{separator}{} = {value:#x}", display_val(operand, function));
            written.push(operand);
        }
    }
    if !written.is_empty() {
        line.push(']');
    }
}

impl Meter for Tracer<'_, '_> {
    fn instruction(
        &mut self,
        function: &Function,
        inst: InstId,
        operand: &dyn Fn(ValueId) -> Option<U256>,
    ) {
        self.flush();
        let block = self.block(function, inst);
        let mut line = self.start(function, block);
        let _ = write!(line, "{}", display_instruction(function, Some(self.module), inst));
        let kind = &function.inst(inst).kind;
        // A phi's inputs come from different edges; its result shows the one taken.
        if !matches!(kind, InstKind::Phi(_)) {
            write_operands(&mut line, function, kind.operands(), operand);
        }
        if matches!(kind, InstKind::ICall { function: Callee::Function(_), .. }) {
            (self.sink)(&line);
            self.depth += 1;
        } else {
            self.pending = Some(line);
        }
    }

    fn terminator(
        &mut self,
        function: &Function,
        block: BlockId,
        operand: &dyn Fn(ValueId) -> Option<U256>,
    ) {
        self.flush();
        let Some(terminator) = &function.blocks[block].terminator else { return };
        let mut line = self.start(function, block);
        let _ = write!(line, "{}", display_terminator(terminator, function, Some(self.module)));
        write_operands(&mut line, function, terminator.operands(), operand);
        (self.sink)(&line);
        if matches!(terminator, Terminator::Return { .. }) {
            self.depth = self.depth.saturating_sub(1);
        }
    }

    fn result(&mut self, value: U256) {
        if let Some(line) = self.pending.take() {
            (self.sink)(&format!("{line} -> {value:#x}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODULE: &str = "@module Interpret
@phase lowered
fn @double(arg0: i256) -> i256 {
  bb0:
    v0 = add arg0, arg0
    ret v0
}

fn @twice(arg0: i256) -> i256 {
  bb0:
    v0 = icall @double, arg0
    v1 = icall @double, v0
    sstore 1, v1
    ret v1
}
";

    /// Answers every read with zero and places the heap at `0x80`.
    struct Zero;

    impl Host for Zero {
        fn read(&mut self, _: u8, _: &[U256]) -> Option<U256> {
            Some(U256::ZERO)
        }

        fn storage(&mut self, _: U256) -> U256 {
            U256::ZERO
        }
    }

    #[test]
    fn calls_and_traces() {
        let mut lines = Vec::new();
        let mut trace = |line: &str| lines.push(line.to_owned());
        let options = Options { trace: Some(&mut trace), ..Options::default() };
        let execution = call(MODULE, "@twice", &[U256::from(3)], &mut Zero, options).unwrap();
        assert_eq!(execution.outcome, Outcome::Return(vec![U256::from(12)]));
        assert_eq!(execution.storage, [(U256::ONE, U256::from(12))]);
        snapbox::assert_data_eq!(
            lines.join("\n"),
            snapbox::str![[r#"
@twice bb0: v0 = icall @double, arg0  [arg0 = 0x3]
  @double bb0: v0 = add arg0, arg0  [arg0 = 0x3] -> 0x6
  @double bb0: ret v0  [v0 = 0x6]
@twice bb0: v1 = icall @double, v0  [v0 = 0x6]
  @double bb0: v0 = add arg0, arg0  [arg0 = 0x6] -> 0xc
  @double bb0: ret v0  [v0 = 0xc]
@twice bb0: sstore 1, v1  [v1 = 0xc]
@twice bb0: ret v1  [v1 = 0xc]
"#]]
        );
        let error = call(MODULE, "missing", &[], &mut Zero, Options::default()).unwrap_err();
        assert_eq!(error, "the module has no function `@missing`");
        let error = call(MODULE, "double", &[], &mut Zero, Options::default()).unwrap_err();
        assert_eq!(error, "`@double` takes 1 word, not 0 words");
    }

    #[test]
    fn disambiguated_functions() {
        let module = "@module Names
@phase lowered
fn @f.0(arg0: i256) -> i256 {
  bb0:
    ret arg0
}

fn @f.1(arg0: i256) -> i256 {
  bb0:
    v0 = add arg0, 1
    ret v0
}
";
        let run = |name| call(module, name, &[U256::from(5)], &mut Zero, Options::default());
        assert_eq!(run("f.0").unwrap().outcome, Outcome::Return(vec![U256::from(5)]));
        assert_eq!(run("@f.1").unwrap().outcome, Outcome::Return(vec![U256::from(6)]));
        assert_eq!(run("f").unwrap_err(), "the module has no function `@f`");
    }

    #[test]
    fn transactions() {
        let transact = |body: &str| {
            let module = format!("@module Tx\n@phase lowered\nfn @entry() [entry] {{\n{body}}}\n");
            transact(&module, &[], &mut Zero, Options::default()).unwrap()
        };
        // A revert discards the transaction's writes and logs.
        let execution = transact(
            "  bb0:
    sstore 1, 2
    tstore 3, 4
    log0 0, 0
    revert 0, 0
",
        );
        assert_eq!(execution.outcome, Outcome::Revert(Vec::new()));
        assert_eq!((execution.storage, execution.transient, execution.logs), Default::default());

        // The backend places a deferred allocation, so its address must not reach the result.
        let alloc = "  bb0:
    v0 = alloc raw, exact, uninitialized, infallible, 32 !metadata(deferred_alloc)
    v1 = ptrtoint memptr v0 to i256
    mstore v1, 7
";
        let execution = transact(&format!(
            "{alloc}    v2 = mload v1
    mstore 0, v2
    returndata 0, 32
"
        ));
        let seven = U256::from(7).to_be_bytes::<32>().to_vec();
        assert_eq!(execution.outcome, Outcome::Success(seven));
        let execution = transact(&format!(
            "{alloc}    mstore 0, v1
    returndata 0, 32
"
        ));
        let unsupported = "reaches `a result that depends on the memory layout`".to_owned();
        assert_eq!(execution.outcome, Outcome::Unsupported(unsupported));
    }

    #[test]
    fn dumps() {
        let dump = "// === a.sol:A ===
@module A
@phase lowered
// === a.sol:B ===
@module B
{\"contracts\":{}}
";
        let modules = parse_dump(dump);
        assert_eq!(
            modules.iter().map(|module| &module.name[..]).collect::<Vec<_>>(),
            ["a.sol:A", "a.sol:B"]
        );
        assert!(!modules[1].mir.contains("contracts"));
        // A file without headers is one module, named by its `@module` line.
        let modules = parse_dump(MODULE);
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].name, "Interpret");
        assert_eq!(modules[0].mir, MODULE);
    }
}
