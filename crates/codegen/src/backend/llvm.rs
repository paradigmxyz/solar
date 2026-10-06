//! Lowers MIR to LLVM IR for solx's statically linked EVM target.
//!
//! Word arithmetic uses i256, with EVM intrinsics for operations whose behavior
//! differs from LLVM on zero divisors or oversized shifts. Address spaces keep
//! heap, calldata, code, and persistent storage separate. LLVM keeps SSA phis
//! and CFG edges until its own optimizer and EVM stack scheduler run. Typed
//! builders emit reachable blocks in reverse postorder and attach phi inputs
//! after all definitions exist. Text serialization is only a worker transport
//! and diagnostic output format.
//! Native emission runs in a child of the statically linked CLI so fatal LLVM
//! failures become diagnostics. Native object relocations resolve constructor
//! sizes and immutable offsets. Activation frames use the heap; the worker reports
//! native spill requirements so contract memory can start above the private region.

use super::{assembler::ImmutableRef, evm::EvmArtifact};
use crate::mir::{
    Function, ImmutableId, InstKind, Module, Terminator, TypeSize, Value, ValueId,
    analysis::{CallGraphInfo, CfgInfo},
    immutable::immutable_staging_base,
};
use inkwell::{
    AddressSpace, IntPredicate, OptimizationLevel,
    attributes::{Attribute, AttributeLoc},
    builder::Builder,
    context::Context,
    intrinsics::Intrinsic,
    memory_buffer::{CodeSegment, MemoryBuffer},
    module::{Linkage, Module as LlvmModule},
    passes::PassBuilderOptions,
    targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetTriple},
    types::IntType,
    values::{BasicMetadataValueEnum, BasicValue, FunctionValue, IntValue, PointerValue},
};
use solar_config::OptimizationMode;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write as IoWrite},
    path::PathBuf,
    process::{Command, ExitCode, Stdio},
    sync::OnceLock,
};

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct NativeArtifact {
    bytes: Vec<u8>,
    immutables: BTreeMap<String, BTreeSet<u64>>,
}

#[derive(Clone, Copy)]
struct MemoryLayout {
    base: u64,
    preserve_expansion: bool,
}

static WORKER: OnceLock<PathBuf> = OnceLock::new();
const WORKER_ARG: &str = "--internal-llvm-codegen-worker";

/// Initializes the CLI's statically linked LLVM worker before argument parsing.
///
/// Workers report spill requirements for replanning and isolate fatal native
/// failures. Hosts must call this at startup and return a supplied exit code.
/// Ordinary invocations only register the current executable for later workers.
pub fn initialize_cli_worker() -> Option<ExitCode> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new(WORKER_ARG)) {
        if let Ok(executable) = std::env::current_exe() {
            let _ = WORKER.set(executable);
        }
        return None;
    }
    let result = (|| {
        let optimization = match args.next().as_deref().and_then(|a| a.to_str()) {
            Some("none") => OptimizationMode::None,
            Some("size") => OptimizationMode::Size,
            Some("gas") => OptimizationMode::Gas,
            _ => return Err("invalid LLVM worker optimization".into()),
        };
        let constructor = match args.next().as_deref().and_then(|a| a.to_str()) {
            Some("init") => true,
            Some("runtime") => false,
            _ => return Err("invalid LLVM worker section".into()),
        };
        let base = args
            .next()
            .and_then(|a| a.to_str().and_then(|s| s.parse::<u64>().ok()))
            .ok_or("invalid LLVM worker memory base")?;
        let mut text = String::new();
        std::io::stdin().read_to_string(&mut text).map_err(|e| e.to_string())?;
        let artifact = assemble_native(&text, optimization, constructor, base)?;
        serde_json::to_writer(std::io::stdout(), &artifact).map_err(|e| e.to_string())
    })();
    Some(match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    })
}

unsafe extern "C" fn stack_error(size: u64) {
    eprintln!("solar-llvm-spill-bytes:{size}");
    std::process::exit(1);
}

pub(super) fn compile(
    module: &Module,
    optimization: OptimizationMode,
) -> Result<EvmArtifact, String> {
    let mut base = 0;
    for _ in 0..16 {
        let translated = super::memory::translate(module, base);
        let mut required = base;
        let artifact = compile_with_memory(&translated, optimization, base, &mut required)?;
        if required <= base {
            return Ok(artifact);
        }
        base = required;
    }
    Err("LLVM spill layout did not stabilize".into())
}

fn compile_with_memory(
    module: &Module,
    optimization: OptimizationMode,
    base: u64,
    required: &mut u64,
) -> Result<EvmArtifact, String> {
    lower_contract(module, optimization, base, required).map_err(|e| e.to_string())
}

type LowerResult<T> = Result<T, Box<dyn std::error::Error>>;

fn lower_contract(
    module: &Module,
    optimization: OptimizationMode,
    base: u64,
    required: &mut u64,
) -> LowerResult<EvmArtifact> {
    let context = Context::create();
    let memory = MemoryLayout { base, preserve_expansion: super::memory::observes_size(module) };
    let fmp = base + 64;
    let entry = module.dispatch_entry().ok_or("missing runtime entry")?;
    let staging = immutable_staging_base(module);
    let (_, args_base) = super::alternative::memory_layout(module);
    let graph = CallGraphInfo::new(module);
    let mut runtime_ir = Lowering::new(&context, module, memory);
    let mut reachable = graph.reachable_callees_from([entry]);
    reachable.insert(entry);
    for id in reachable.iter() {
        runtime_ir.function(&module.functions[id], id.index(), false)?;
    }
    runtime_ir.entry();
    runtime_ir.store(runtime_ir.word(fmp), runtime_ir.word(args_base), 1)?;
    runtime_ir.builder.build_call(runtime_ir.functions[entry.index()], &[], "")?;
    runtime_ir.exit("stop", &[])?;
    runtime_ir.module.verify().map_err(|e| e.to_string())?;
    let runtime_text = runtime_ir.module.print_to_string().to_string();
    let mut runtime = assemble(&runtime_text, optimization, false, base, required)?;
    if *required > base {
        return Ok(EvmArtifact::default());
    }
    super::alternative::append_runtime_tail(module, &mut runtime.bytes);

    let mut init = Lowering::new(&context, module, memory);
    let constructor = module
        .functions
        .iter_enumerated()
        .find(|(_, f)| f.attributes.is_constructor)
        .map(|(id, _)| id);
    let mut reachable = graph.reachable_callees_from(constructor);
    if let Some(id) = constructor {
        reachable.insert(id);
    }
    for id in reachable.iter() {
        init.function(&module.functions[id], id.index(), true)?;
    }
    let runtime_data = init.data("runtime", &runtime.bytes);
    let entry = init.entry();
    init.store(init.word(fmp), init.word(args_base), 1)?;
    if let Some(id) = module.library_deploy_address() {
        let address = init.evm_word("address", &[], "address")?;
        init.store(init.word(base + staging + id.index() as u64 * 32), address, 1)?;
    }
    if let Some(id) = constructor {
        let size = init.constructor_args_size()?;
        init.copy_memory(4, false, init.word(base + args_base), size.0, size.1)?;
        let rounded = init.builder.build_int_add(size.1, init.word(args_base + 31), "rounded")?;
        let free = init.builder.build_and(rounded, init.word(31).const_not(), "free")?;
        init.store(init.word(fmp), free, 1)?;
        let args = (0..module.functions[id].params.len())
            .map(|i| {
                init.load(init.word(base + args_base + i as u64 * 32), 1, false, &format!("arg{i}"))
                    .map(Into::into)
            })
            .collect::<LowerResult<Vec<_>>>()?;
        init.builder.build_call(init.functions[id.index()], &args, "")?;
    } else if !module.is_library {
        let value = init.evm_word("callvalue", &[], "value")?;
        let payable =
            init.builder.build_int_compare(IntPredicate::EQ, value, init.word(0), "payable")?;
        let reject = context.append_basic_block(entry, "reject");
        let postlude = context.append_basic_block(entry, "postlude");
        init.builder.build_conditional_branch(payable, postlude, reject)?;
        init.builder.position_at_end(reject);
        init.exit(
            "revert",
            &[context.ptr_type(AddressSpace::from(1)).const_null().into(), init.word(0).into()],
        )?;
        init.builder.position_at_end(postlude);
    }
    let logical_deploy = init.load(init.word(fmp), 1, false, "logical_deploy")?;
    let deploy = if base == 0 {
        logical_deploy
    } else {
        let deploy = init.builder.build_int_add(logical_deploy, init.word(base), "deploy")?;
        let overflowed = init.builder.build_int_compare(
            IntPredicate::ULT,
            deploy,
            logical_deploy,
            "overflowed",
        )?;
        let overflow = context.append_basic_block(entry, "overflow");
        let copy = context.append_basic_block(entry, "copy_runtime");
        init.builder.build_conditional_branch(overflowed, overflow, copy)?;
        init.builder.position_at_end(overflow);
        init.exit("invalid", &[])?;
        init.builder.position_at_end(copy);
        deploy
    };
    let deploy_ptr = init.pointer(deploy, 1)?;
    init.copy_pointers(deploy_ptr, runtime_data, init.word(runtime.bytes.len() as u64), false)?;
    let mut immutable_references = Vec::new();
    for (name, offsets) in &runtime.immutables {
        let id = name
            .strip_prefix('i')
            .and_then(|s| s.parse::<usize>().ok())
            .ok_or("invalid LLVM immutable identifier")?;
        if id >= module.immutable_count() {
            return Err("invalid LLVM immutable identifier".into());
        }
        let value =
            init.load(init.word(base + staging + id as u64 * 32), 1, false, &format!("imm{id}"))?;
        for &offset in offsets {
            if offset == 0
                || offset.checked_add(32).is_none_or(|end| end > runtime.bytes.len() as u64)
                || runtime.bytes.get(offset as usize - 1) != Some(&0x7f)
            {
                return Err("invalid LLVM immutable offset".into());
            }
            let patch = init.builder.build_int_add(deploy, init.word(offset), "patch")?;
            init.store(patch, value, 1)?;
            immutable_references.push(ImmutableRef {
                id: ImmutableId::from_usize(id),
                code_offset: offset as usize - 1,
                type_size: TypeSize::new_int_bits(256),
            });
        }
    }
    init.exit("return", &[deploy_ptr.into(), init.word(runtime.bytes.len() as u64).into()])?;
    init.module.verify().map_err(|e| e.to_string())?;
    let init_text = init.module.print_to_string().to_string();
    Ok(EvmArtifact {
        deployment: assemble(&init_text, optimization, true, base, required)?.bytes,
        runtime: runtime.bytes,
        immutable_references,
        backend_ir: Some(format!("; Runtime\n{runtime_text}\n; Deployment\n{init_text}")),
        ..Default::default()
    })
}

fn assemble(
    text: &str,
    optimization: OptimizationMode,
    constructor: bool,
    base: u64,
    required: &mut u64,
) -> Result<NativeArtifact, String> {
    let executable =
        WORKER.get().ok_or("LLVM requires initialize_cli_worker at process startup")?;
    let mut child = Command::new(executable)
        .arg(WORKER_ARG)
        .arg(if matches!(optimization, OptimizationMode::None) {
            "none"
        } else if optimization.is_size() {
            "size"
        } else {
            "gas"
        })
        .arg(if constructor { "init" } else { "runtime" })
        .arg(base.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start LLVM worker: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("missing LLVM worker stdin")?;
    std::thread::scope(|scope| {
        let writer = scope.spawn(move || stdin.write_all(text.as_bytes()));
        let output = child.wait_with_output().map_err(|e| e.to_string())?;
        let written = writer.join().map_err(|_| "LLVM input writer panicked")?;
        if !output.status.success() {
            if let Some(size) = String::from_utf8_lossy(&output.stderr).lines().find_map(|line| {
                line.strip_prefix("solar-llvm-spill-bytes:").and_then(|s| s.parse::<u64>().ok())
            }) {
                *required = (*required).max(size);
                if size > base {
                    return Ok(NativeArtifact::default());
                }
            }
            return Err(format!(
                "LLVM worker exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        written.map_err(|e| e.to_string())?;
        serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("invalid LLVM worker response: {e}"))
    })
}

fn assemble_native(
    text: &str,
    optimization: OptimizationMode,
    constructor: bool,
    base: u64,
) -> Result<NativeArtifact, String> {
    Target::initialize_evm(&InitializationConfig::default());
    inkwell::support::parse_command_line_options(
        &["solar", "--evm-stack-region-offset=0", &format!("--evm-stack-region-size={base}")],
        "",
    );
    inkwell::support::error_handling::install_stack_error_handler(stack_error);
    let llvm = inkwell::context::Context::create();
    let module = llvm
        .create_module_from_ir(MemoryBuffer::create_from_memory_range_copy(
            text.as_bytes(),
            "Contract",
        ))
        .map_err(|e| e.to_string())?;
    module.add_basic_value_flag(
        "evm-memory-guard",
        inkwell::module::FlagBehavior::Override,
        llvm.i64_type().const_zero(),
    );
    module.add_basic_value_flag(
        "evm-stack-region-size",
        inkwell::module::FlagBehavior::Override,
        llvm.i64_type().const_int(base, false),
    );
    module.verify().map_err(|e| e.to_string())?;
    let level = if matches!(optimization, OptimizationMode::None) {
        OptimizationLevel::None
    } else {
        OptimizationLevel::Aggressive
    };
    let target = Target::from_name("evm")
        .ok_or("EVM LLVM target is unavailable")?
        .create_target_machine(
            &TargetTriple::create("evm-unknown-unknown"),
            "",
            "",
            level,
            RelocMode::Default,
            CodeModel::Default,
        )
        .ok_or("could not create EVM target machine")?;
    module.set_data_layout(&target.get_target_data().get_data_layout());
    let pipeline = match optimization {
        OptimizationMode::None => "default<O0>",
        OptimizationMode::Size => "default<Oz>",
        _ => "default<O3>",
    };
    module
        .run_passes(pipeline, &target, PassBuilderOptions::create())
        .map_err(|e| e.to_string())?;
    let object =
        target.write_to_memory_buffer(&module, FileType::Object).map_err(|e| e.to_string())?;
    let immutables = object.get_immutables_evm();
    let object = MemoryBuffer::assemble_evm(
        &[&object],
        &["Contract"],
        if constructor { CodeSegment::Deploy } else { CodeSegment::Runtime },
    )
    .map_err(|e| e.to_string())?;
    let bytecode = object.link_evm(&Default::default()).map_err(|e| e.to_string())?;
    Ok(NativeArtifact { bytes: bytecode.as_slice().to_vec(), immutables })
}

struct Lowering<'ctx, 'mir> {
    context: &'ctx Context,
    module: LlvmModule<'ctx>,
    builder: Builder<'ctx>,
    word_type: IntType<'ctx>,
    mir: &'mir Module,
    memory: MemoryLayout,
    entry_function: FunctionValue<'ctx>,
    functions: Vec<FunctionValue<'ctx>>,
    globals: Vec<PointerValue<'ctx>>,
}

impl<'ctx, 'mir> Lowering<'ctx, 'mir> {
    fn new(context: &'ctx Context, mir: &'mir Module, memory: MemoryLayout) -> Self {
        let module = context.create_module("Contract");
        module.set_triple(&TargetTriple::create("evm-unknown-unknown"));
        module.set_data_layout(
            &inkwell::targets::TargetData::create("E-p:256:256-i256:256:256-S256-a:256:256")
                .get_data_layout(),
        );
        // The EVM target requires the entry function to precede every other function.
        let entry_function =
            module.add_function("__entry", context.void_type().fn_type(&[], false), None);
        let word_type = context.custom_width_int_type(256);
        let functions = mir
            .functions
            .iter_enumerated()
            .map(|(id, f)| {
                let params = vec![word_type.into(); f.params.len()];
                let ty = if f.return_components().is_empty() {
                    context.void_type().fn_type(&params, false)
                } else {
                    word_type.fn_type(&params, false)
                };
                module.add_function(&format!("f{}", id.index()), ty, None)
            })
            .collect();
        let mut this = Self {
            context,
            module,
            builder: context.create_builder(),
            word_type,
            mir,
            memory,
            entry_function,
            functions,
            globals: Vec::new(),
        };
        this.globals = mir
            .data
            .iter_enumerated()
            .map(|(id, data)| this.data(&format!("d{}", id.index()), data.bytes.linked()))
            .collect();
        this
    }

    fn attributes(&self, function: FunctionValue<'ctx>) {
        function.add_attribute(
            AttributeLoc::Function,
            self.context.create_enum_attribute(
                Attribute::get_named_enum_kind_id("null_pointer_is_valid"),
                0,
            ),
        );
        function.add_attribute(
            AttributeLoc::Function,
            self.context.create_string_attribute("target-features", "+osaka"),
        );
    }

    fn entry(&self) -> FunctionValue<'ctx> {
        let entry = self.entry_function;
        self.attributes(entry);
        entry.add_attribute(
            AttributeLoc::Function,
            self.context.create_enum_attribute(Attribute::get_named_enum_kind_id("noreturn"), 0),
        );
        entry.add_attribute(
            AttributeLoc::Function,
            self.context.create_string_attribute("evm-entry-function", ""),
        );
        self.builder.position_at_end(self.context.append_basic_block(entry, "entry"));
        entry
    }

    fn data(&self, name: &str, bytes: &[u8]) -> PointerValue<'ctx> {
        let value = self.context.const_string(bytes, false);
        let global = self.module.add_global(value.get_type(), Some(AddressSpace::from(4)), name);
        global.set_linkage(Linkage::Private);
        global.set_constant(true);
        global.set_initializer(&value);
        global.as_pointer_value()
    }

    fn word(&self, value: u64) -> IntValue<'ctx> {
        self.word_type.const_int(value, false)
    }

    fn intrinsic(
        &self,
        name: &str,
        overloads: &[inkwell::types::BasicTypeEnum<'ctx>],
        args: &[BasicMetadataValueEnum<'ctx>],
        result: &str,
    ) -> LowerResult<Option<IntValue<'ctx>>> {
        let intrinsic =
            Intrinsic::find(name).ok_or_else(|| format!("unknown LLVM intrinsic `{name}`"))?;
        let function = intrinsic
            .get_declaration(&self.module, overloads)
            .ok_or_else(|| format!("invalid LLVM intrinsic overload `{name}`"))?;
        let call = self.builder.build_call(function, args, result)?;
        Ok(call.try_as_basic_value().basic().map(|value| value.into_int_value()))
    }

    fn evm(
        &self,
        name: &str,
        args: &[BasicMetadataValueEnum<'ctx>],
        result: &str,
    ) -> LowerResult<Option<IntValue<'ctx>>> {
        self.intrinsic(&format!("llvm.evm.{name}"), &[], args, result)
    }

    fn evm_word(
        &self,
        name: &str,
        args: &[BasicMetadataValueEnum<'ctx>],
        result: &str,
    ) -> LowerResult<IntValue<'ctx>> {
        self.evm(name, args, result)?
            .ok_or_else(|| format!("LLVM intrinsic `{name}` has no result").into())
    }

    fn exit(&self, name: &str, args: &[BasicMetadataValueEnum<'ctx>]) -> LowerResult<()> {
        self.evm(name, args, "")?;
        self.builder.build_unreachable()?;
        Ok(())
    }

    fn pointer(&self, value: IntValue<'ctx>, space: u16) -> LowerResult<PointerValue<'ctx>> {
        Ok(self.builder.build_int_to_ptr(
            value,
            self.context.ptr_type(AddressSpace::from(space)),
            "ptr",
        )?)
    }

    fn load(
        &self,
        offset: IntValue<'ctx>,
        space: u16,
        volatile: bool,
        name: &str,
    ) -> LowerResult<IntValue<'ctx>> {
        let value = self.builder.build_load(self.word_type, self.pointer(offset, space)?, name)?;
        let instruction = value.as_instruction_value().ok_or("load without instruction")?;
        instruction.set_alignment(1)?;
        instruction.set_volatile(volatile)?;
        Ok(value.into_int_value())
    }

    fn store(&self, offset: IntValue<'ctx>, value: IntValue<'ctx>, space: u16) -> LowerResult<()> {
        self.builder.build_store(self.pointer(offset, space)?, value)?.set_alignment(1)?;
        Ok(())
    }

    fn copy_memory(
        &self,
        space: u16,
        volatile: bool,
        dest: IntValue<'ctx>,
        source: IntValue<'ctx>,
        size: IntValue<'ctx>,
    ) -> LowerResult<()> {
        self.copy_pointers(self.pointer(dest, 1)?, self.pointer(source, space)?, size, volatile)
    }

    fn copy_pointers(
        &self,
        dest: PointerValue<'ctx>,
        source: PointerValue<'ctx>,
        size: IntValue<'ctx>,
        volatile: bool,
    ) -> LowerResult<()> {
        let name = if source.get_type().get_address_space() == AddressSpace::from(1) {
            "llvm.memmove"
        } else {
            "llvm.memcpy"
        };
        self.intrinsic(
            name,
            &[dest.get_type().into(), source.get_type().into(), self.word_type.into()],
            &[
                dest.into(),
                source.into(),
                size.into(),
                self.context.bool_type().const_int(volatile.into(), false).into(),
            ],
            "",
        )?;
        Ok(())
    }

    fn constructor_args_size(&self) -> LowerResult<(IntValue<'ctx>, IntValue<'ctx>)> {
        let metadata =
            self.context.metadata_node(&[self.context.metadata_string("Contract").into()]);
        let end = self.evm_word("datasize", &[metadata.into()], "end")?;
        let total = self.evm_word("codesize", &[], "total")?;
        Ok((end, self.builder.build_int_sub(total, end, "size")?))
    }

    fn function(&mut self, f: &Function, id: usize, constructor: bool) -> LowerResult<()> {
        let function = self.functions[id];
        function.set_linkage(Linkage::Internal);
        self.attributes(function);
        let cfg = CfgInfo::new(f);
        let blocks = f
            .blocks
            .iter_enumerated()
            .map(|(id, _)| {
                cfg.is_reachable(id)
                    .then(|| self.context.append_basic_block(function, &format!("b{}", id.index())))
            })
            .collect::<Vec<_>>();
        let mut values = BTreeMap::new();
        let mut args = function
            .get_param_iter()
            .enumerate()
            .map(|(i, value)| {
                value.set_name(&format!("a{i}"));
                (i, value.into_int_value())
            })
            .collect::<BTreeMap<_, _>>();
        let mut phis = Vec::new();
        for &bid in cfg.rpo() {
            self.builder.position_at_end(blocks[bid.index()].unwrap());
            for &iid in &f.blocks[bid].instructions {
                if let InstKind::Phi(inputs) = &f.inst(iid).kind {
                    let id = f.inst_result_value(iid).ok_or("phi without result")?;
                    let phi =
                        self.builder.build_phi(self.word_type, &format!("v{}", id.index()))?;
                    values.insert(id.index(), phi.as_basic_value().into_int_value());
                    phis.push((phi, inputs));
                }
            }
        }
        let base = self.memory.base;
        let (return_base, args_base) = super::alternative::memory_layout(self.mir);
        let mut frame = None;
        for &bid in cfg.rpo() {
            let block = &f.blocks[bid];
            self.builder.position_at_end(blocks[bid.index()].unwrap());
            if bid.index() == 0 {
                if super::alternative::frame_size(f) != 0 {
                    let start = self.load(self.word(base + 64), 1, false, "frame")?;
                    let end = self.builder.build_int_add(
                        start,
                        self.word(super::alternative::frame_size(f)),
                        "frame_end",
                    )?;
                    self.store(self.word(base + 64), end, 1)?;
                    frame = Some(start);
                }
                if f.params.is_empty() {
                    for arg in f.arg_indices() {
                        if f.selector.is_none() {
                            return Err("unsupported lazy LLVM argument".into());
                        }
                        let pointer = self.pointer(self.word(4 + arg.index() as u64 * 32), 2)?;
                        args.insert(
                            arg.index(),
                            self.evm_word(
                                "calldataload",
                                &[pointer.into()],
                                &format!("a{}", arg.index()),
                            )?,
                        );
                    }
                }
            }
            for &iid in &block.instructions {
                let inst = f.inst(iid);
                if matches!(inst.kind, InstKind::Phi(_)) {
                    continue;
                }
                let result = f.inst_result_value(iid);
                let name = result.map(|v| format!("v{}", v.index())).unwrap_or_default();
                let operands = inst
                    .operands()
                    .iter()
                    .map(|&id| self.value(f, id, &values, &args))
                    .collect::<LowerResult<Vec<_>>>()?;
                let value = match &inst.kind {
                    InstKind::ConstructorArgsBase if constructor => Some(self.word(args_base)),
                    InstKind::ConstructorArgsEnd if constructor => {
                        let (_, size) = self.constructor_args_size()?;
                        Some(self.builder.build_int_add(self.word(args_base), size, &name)?)
                    }
                    InstKind::LoadImmutable(id) if constructor => Some(self.load(
                        self.word(base + immutable_staging_base(self.mir) + id.index() as u64 * 32),
                        1,
                        false,
                        &name,
                    )?),
                    InstKind::LoadImmutable(id) => {
                        let metadata = self.context.metadata_node(&[self
                            .context
                            .metadata_string(&format!("i{}", id.index()))
                            .into()]);
                        Some(self.evm_word("loadimmutable", &[metadata.into()], &name)?)
                    }
                    _ => self.instruction(&inst.kind, operands, frame, &name)?,
                };
                if let Some(id) = result {
                    values.insert(id.index(), value.ok_or("LLVM instruction without result")?);
                }
            }
            let value = |id| self.value(f, id, &values, &args);
            match block.terminator.as_ref().ok_or("missing terminator")? {
                Terminator::Jump(b) => {
                    self.builder.build_unconditional_branch(blocks[b.index()].unwrap())?;
                }
                Terminator::Branch { condition, then_block, else_block } => {
                    let condition = self.builder.build_int_compare(
                        IntPredicate::NE,
                        value(*condition)?,
                        self.word(0),
                        "branch",
                    )?;
                    self.builder.build_conditional_branch(
                        condition,
                        blocks[then_block.index()].unwrap(),
                        blocks[else_block.index()].unwrap(),
                    )?;
                }
                Terminator::Switch { value: v, default, cases } => {
                    let cases = cases
                        .iter()
                        .map(|&(v, b)| Ok((value(v)?, blocks[b.index()].unwrap())))
                        .collect::<LowerResult<Vec<_>>>()?;
                    self.builder.build_switch(
                        value(*v)?,
                        blocks[default.index()].unwrap(),
                        &cases,
                    )?;
                }
                Terminator::Return { values: returns } => {
                    if returns.len() > 1 {
                        for (i, &v) in returns.iter().enumerate() {
                            self.store(
                                self.word(base + return_base + i as u64 * 32),
                                value(v)?,
                                1,
                            )?;
                        }
                        self.store(self.word(base + 32), self.word(return_base), 1)?;
                    }
                    let result = returns.first().map(|&v| value(v)).transpose()?;
                    self.builder.build_return(result.as_ref().map(|v| v as &dyn BasicValue<'_>))?;
                }
                Terminator::TailCall { function, args } => {
                    let args = args
                        .iter()
                        .map(|&v| value(v).map(Into::into))
                        .collect::<LowerResult<Vec<_>>>()?;
                    self.builder.build_call(self.functions[function.index()], &args, "")?;
                    self.exit("stop", &[])?;
                }
                Terminator::ReturnData { offset, size } | Terminator::Revert { offset, size } => {
                    let name = if matches!(block.terminator, Some(Terminator::ReturnData { .. })) {
                        "return"
                    } else {
                        "revert"
                    };
                    self.exit(
                        name,
                        &[self.pointer(value(*offset)?, 1)?.into(), value(*size)?.into()],
                    )?;
                }
                Terminator::Stop => self.exit("stop", &[])?,
                Terminator::Invalid => self.exit("invalid", &[])?,
                Terminator::SelfDestruct { recipient } => {
                    self.exit("selfdestruct", &[value(*recipient)?.into()])?
                }
                Terminator::RevertReturndata => {
                    let size = self.evm_word("returndatasize", &[], "returndata")?;
                    self.copy_memory(3, false, self.word(base), self.word(0), size)?;
                    self.exit("revert", &[self.pointer(self.word(base), 1)?.into(), size.into()])?;
                }
            }
        }
        for (phi, inputs) in phis {
            for &(block, value) in inputs {
                if let Some(block) = blocks[block.index()] {
                    phi.add_incoming(&[(&self.value(f, value, &values, &args)?, block)]);
                }
            }
        }
        Ok(())
    }

    fn value(
        &self,
        f: &Function,
        id: ValueId,
        values: &BTreeMap<usize, IntValue<'ctx>>,
        args: &BTreeMap<usize, IntValue<'ctx>>,
    ) -> LowerResult<IntValue<'ctx>> {
        match f.value(id) {
            Value::Immediate(imm) => Ok(self.word_type.const_int_arbitrary_precision(
                imm.as_u256().ok_or("non-word immediate")?.as_limbs(),
            )),
            Value::Arg(arg) => {
                args.get(&arg.index()).copied().ok_or_else(|| "undefined LLVM argument".into())
            }
            Value::Inst(inst) => {
                let id = f.inst_result_value(*inst).ok_or("instruction without result")?;
                values
                    .get(&id.index())
                    .copied()
                    .ok_or_else(|| "undefined SSA value in LLVM lowering".into())
            }
            _ => Err("undefined value in LLVM lowering".into()),
        }
    }

    fn instruction(
        &self,
        inst: &InstKind,
        mut args: Vec<IntValue<'ctx>>,
        frame: Option<IntValue<'ctx>>,
        name: &str,
    ) -> LowerResult<Option<IntValue<'ctx>>> {
        let value = match inst {
            InstKind::DataSize(size) => self.word_type.const_int_arbitrary_precision(
                size.value(self.mir.data[size.data].bytes.linked().len()).as_limbs(),
            ),
            InstKind::Zext(_) | InstKind::IntToPtr(_) => args[0],
            InstKind::Trunc(_, bits) | InstKind::PtrToInt(_, bits) => {
                let mask = self.word_type.const_int_arbitrary_precision(
                    (alloy_primitives::U256::MAX >> (256 - bits)).as_limbs(),
                );
                self.builder.build_and(args[0], mask, name)?
            }
            InstKind::Sext(_, from, to) => {
                let shift = self.word((256 - from) as u64);
                let left = self.builder.build_left_shift(args[0], shift, "left")?;
                let signed = self.builder.build_right_shift(left, shift, true, "signed")?;
                let mask = self.word_type.const_int_arbitrary_precision(
                    (alloy_primitives::U256::MAX >> (256 - to)).as_limbs(),
                );
                self.builder.build_and(signed, mask, name)?
            }
            InstKind::InternalFrameAddr(offset) => self.builder.build_int_add(
                frame.ok_or("missing LLVM activation frame")?,
                self.word(*offset),
                name,
            )?,
            InstKind::DataCopy(data, ..) => {
                // SAFETY: A single integer index addresses bytes in the constant data array.
                let source = unsafe {
                    self.globals[data.id.index()]
                        .const_gep(self.context.i8_type(), &[self.word(u64::from(data.offset))])
                };
                self.copy_pointers(
                    self.pointer(args[0], 1)?,
                    source,
                    args[1],
                    self.memory.preserve_expansion,
                )?;
                return Ok(None);
            }
            InstKind::CalldataCopy(..)
            | InstKind::CodeCopy(..)
            | InstKind::ReturnDataCopy(..)
            | InstKind::MCopy(..) => {
                let space = match inst {
                    InstKind::CalldataCopy(..) => 2,
                    InstKind::CodeCopy(..) => 4,
                    InstKind::ReturnDataCopy(..) => 3,
                    _ => 1,
                };
                self.copy_memory(space, self.memory.preserve_expansion, args[0], args[1], args[2])?;
                return Ok(None);
            }
            InstKind::Select(..) => {
                let condition = self.builder.build_int_compare(
                    IntPredicate::NE,
                    args[0],
                    self.word(0),
                    "condition",
                )?;
                self.builder.build_select(condition, args[1], args[2], name)?.into_int_value()
            }
            InstKind::Clz(..) => {
                return self.intrinsic(
                    "llvm.ctlz",
                    &[self.word_type.into()],
                    &[args[0].into(), self.context.bool_type().const_zero().into()],
                    name,
                );
            }
            InstKind::Log0(..)
            | InstKind::Log1(..)
            | InstKind::Log2(..)
            | InstKind::Log3(..)
            | InstKind::Log4(..)
            | InstKind::Create(..)
            | InstKind::Create2(..)
            | InstKind::Call { .. }
            | InstKind::CallCode { .. }
            | InstKind::StaticCall { .. }
            | InstKind::DelegateCall { .. }
            | InstKind::ExtCodeCopy(..) => {
                let pointers: &[(usize, u16)] = match inst {
                    InstKind::Call { .. } | InstKind::CallCode { .. } => &[(3, 1), (5, 1)],
                    InstKind::StaticCall { .. } | InstKind::DelegateCall { .. } => {
                        &[(2, 1), (4, 1)]
                    }
                    InstKind::Create(..) | InstKind::Create2(..) => &[(1, 1)],
                    InstKind::ExtCodeCopy(..) => &[(1, 1), (2, 4)],
                    _ => &[(0, 1)],
                };
                let mut typed = args.iter().copied().map(Into::into).collect::<Vec<_>>();
                for &(index, space) in pointers {
                    typed[index] = self.pointer(args[index], space)?.into();
                }
                return self.evm(inst.mnemonic(), &typed, name);
            }
            InstKind::Add(..) => self.builder.build_int_add(args[0], args[1], name)?,
            InstKind::Sub(..) => self.builder.build_int_sub(args[0], args[1], name)?,
            InstKind::Mul(..) => self.builder.build_int_mul(args[0], args[1], name)?,
            InstKind::And(..) => self.builder.build_and(args[0], args[1], name)?,
            InstKind::Or(..) => self.builder.build_or(args[0], args[1], name)?,
            InstKind::Xor(..) => self.builder.build_xor(args[0], args[1], name)?,
            InstKind::Not(..) => self.builder.build_not(args[0], name)?,
            InstKind::Lt(..)
            | InstKind::Gt(..)
            | InstKind::SLt(..)
            | InstKind::SGt(..)
            | InstKind::Eq(..)
            | InstKind::Ne(..) => {
                let predicate = match inst {
                    InstKind::Ne(..) => IntPredicate::NE,
                    InstKind::Lt(..) => IntPredicate::ULT,
                    InstKind::Gt(..) => IntPredicate::UGT,
                    InstKind::SLt(..) => IntPredicate::SLT,
                    InstKind::SGt(..) => IntPredicate::SGT,
                    _ => IntPredicate::EQ,
                };
                let cmp = self.builder.build_int_compare(predicate, args[0], args[1], "cmp")?;
                self.builder.build_int_z_extend(cmp, self.word_type, name)?
            }
            InstKind::MLoad(..)
            | InstKind::MStore(..)
            | InstKind::SLoad(..)
            | InstKind::SStore(..)
            | InstKind::TLoad(..)
            | InstKind::TStore(..)
            | InstKind::Fmp
            | InstKind::SetFmp(..) => {
                let space = match inst {
                    InstKind::SLoad(..) | InstKind::SStore(..) => 5,
                    InstKind::TLoad(..) | InstKind::TStore(..) => 6,
                    _ => 1,
                };
                if matches!(inst, InstKind::Fmp | InstKind::SetFmp(..)) {
                    args.insert(0, self.word(self.memory.base + 64));
                }
                if matches!(
                    inst,
                    InstKind::MStore(..)
                        | InstKind::SStore(..)
                        | InstKind::TStore(..)
                        | InstKind::SetFmp(..)
                ) {
                    self.store(args[0], args[1], space)?;
                    return Ok(None);
                }
                self.load(args[0], space, space == 1 && self.memory.preserve_expansion, name)?
            }
            InstKind::CalldataLoad(..) | InstKind::Keccak256(..) | InstKind::MStore8(..) => {
                if self.memory.preserve_expansion && matches!(inst, InstKind::Keccak256(..)) {
                    self.copy_memory(1, true, args[0], args[0], args[1])?;
                }
                let space = if matches!(inst, InstKind::CalldataLoad(..)) { 2 } else { 1 };
                let mut typed = vec![self.pointer(args[0], space)?.into()];
                typed.extend(args.into_iter().skip(1).map(BasicMetadataValueEnum::from));
                return self.evm(
                    if matches!(inst, InstKind::Keccak256(..)) { "sha3" } else { inst.mnemonic() },
                    &typed,
                    name,
                );
            }
            InstKind::ICall { function: crate::mir::Callee::Function(function), .. } => {
                let args = args.into_iter().map(Into::into).collect::<Vec<_>>();
                return Ok(self
                    .builder
                    .build_call(self.functions[function.index()], &args, name)?
                    .try_as_basic_value()
                    .basic()
                    .map(|v| v.into_int_value()));
            }
            InstKind::Div(..)
            | InstKind::SDiv(..)
            | InstKind::Mod(..)
            | InstKind::SMod(..)
            | InstKind::Shl(..)
            | InstKind::Shr(..)
            | InstKind::Sar(..)
            | InstKind::Exp(..)
            | InstKind::AddMod(..)
            | InstKind::MulMod(..)
            | InstKind::Byte(..)
            | InstKind::SignExtend(..)
            | InstKind::CalldataSize
            | InstKind::Caller
            | InstKind::CallValue
            | InstKind::CodeSize
            | InstKind::ExtCodeSize(..)
            | InstKind::ExtCodeHash(..)
            | InstKind::ReturnDataSize
            | InstKind::Origin
            | InstKind::GasPrice
            | InstKind::BlockHash(..)
            | InstKind::Coinbase
            | InstKind::Timestamp
            | InstKind::BlockNumber
            | InstKind::GasLimit
            | InstKind::ChainId
            | InstKind::Balance(..)
            | InstKind::SelfBalance
            | InstKind::Gas
            | InstKind::BaseFee
            | InstKind::BlobBaseFee
            | InstKind::BlobHash(..)
            | InstKind::MSize
            | InstKind::Address => {
                return self.evm(
                    inst.mnemonic(),
                    &args.into_iter().map(Into::into).collect::<Vec<_>>(),
                    name,
                );
            }
            InstKind::PrevRandao => return self.evm("difficulty", &[], name),
            other => {
                return Err(format!("unsupported LLVM instruction `{}`", other.mnemonic()).into());
            }
        };
        Ok(Some(value))
    }
}
