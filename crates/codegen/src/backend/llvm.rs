//! Lowers MIR to LLVM IR for solx's statically linked EVM target.
//!
//! Word arithmetic uses i256, with EVM intrinsics for operations whose behavior
//! differs from LLVM on zero divisors or oversized shifts. Address spaces keep
//! heap, calldata, code, and persistent storage separate. LLVM keeps SSA phis
//! and CFG edges until its own optimizer and EVM stack scheduler run.
//! Native emission runs in a child of the statically linked CLI so fatal LLVM
//! failures become diagnostics. Native object relocations resolve constructor
//! sizes and immutable offsets. Activation frames use the heap; native spills
//! remain unsupported because they do not have an isolated memory region.

use super::{assembler::ImmutableRef, evm::EvmArtifact};
use crate::mir::{
    Function, ImmutableId, InstKind, Module, Terminator, TypeSize, Value, ValueId,
    analysis::CallGraphInfo, immutable::immutable_staging_base,
};
use inkwell::{
    OptimizationLevel,
    memory_buffer::{CodeSegment, MemoryBuffer},
    passes::PassBuilderOptions,
    targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetTriple},
};
use solar_config::OptimizationMode;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
    io::{Read, Write as IoWrite},
    path::PathBuf,
    process::{Command, ExitCode, Stdio},
    sync::OnceLock,
};

#[derive(serde::Serialize, serde::Deserialize)]
struct NativeArtifact {
    bytes: Vec<u8>,
    immutables: BTreeMap<String, BTreeSet<u64>>,
}

static WORKER: OnceLock<PathBuf> = OnceLock::new();
const WORKER_ARG: &str = "--internal-llvm-codegen-worker";

/// Initializes the CLI's statically linked LLVM worker before argument parsing.
///
/// LLVM can terminate its process when EVM values need spills. Hosts must call this at
/// startup and return a supplied exit code, so native failures stay in a child.
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
        let mut text = String::new();
        std::io::stdin().read_to_string(&mut text).map_err(|e| e.to_string())?;
        let artifact = assemble_native(&text, optimization, constructor)?;
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

unsafe extern "C" fn stack_error(_size: u64) {
    eprintln!("LLVM stack spills require a separate memory layout");
    std::process::exit(1);
}

pub(super) fn compile(
    module: &Module,
    optimization: OptimizationMode,
) -> Result<EvmArtifact, String> {
    let entry = module.dispatch_entry().ok_or("missing runtime entry")?;
    let staging = immutable_staging_base(module);
    let (_, args_base) = super::alternative::memory_layout(module);
    let graph = CallGraphInfo::new(module);
    let mut declarations = BTreeSet::from(["declare void @llvm.evm.stop()".into()]);
    let mut functions = String::new();
    let mut reachable = graph.reachable_callees_from([entry]);
    reachable.insert(entry);
    for id in reachable.iter() {
        function(
            module,
            &module.functions[id],
            id.index(),
            false,
            &mut functions,
            &mut declarations,
        )?;
    }
    let mut globals = String::new();
    // @dN = private addrspace(4) constant [N x i8] c"bytes"
    for (id, bytes) in module.iter_data() {
        writeln!(
            globals,
            "@d{} = private addrspace(4) constant [{} x i8] c\"{}\"",
            id.index(),
            bytes.len(),
            bytes.iter().map(|b| format!("\\{b:02X}")).collect::<String>()
        )
        .unwrap();
    }
    // __entry: initialize heap; call dispatch; stop
    let runtime_text = format!(
        "{}\n{globals}\ndefine void @__entry() noreturn \"evm-entry-function\" #0 {{\nentry:\nstore i256 {args_base}, ptr addrspace(1) inttoptr (i256 64 to ptr addrspace(1)), align 1\ncall void @f{}()\ncall void @llvm.evm.stop()\nunreachable\n}}\n{functions}\n{}\n{}",
        header(),
        entry.index(),
        declarations.iter().cloned().collect::<Vec<_>>().join("\n"),
        attributes()
    );
    let mut runtime = assemble(&runtime_text, optimization, false)?;
    super::alternative::append_runtime_tail(module, &mut runtime.bytes);
    declarations.clear();
    functions.clear();
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
        function(
            module,
            &module.functions[id],
            id.index(),
            true,
            &mut functions,
            &mut declarations,
        )?;
    }
    let mut init = format!(
        "{}\n{globals}\n@runtime = private addrspace(4) constant [{} x i8] c\"{}\"\ndefine void @__entry() noreturn \"evm-entry-function\" #0 {{\nentry:\nstore i256 {args_base}, ptr addrspace(1) inttoptr (i256 64 to ptr addrspace(1)), align 1\n",
        header(),
        runtime.bytes.len(),
        runtime.bytes.iter().map(|b| format!("\\{b:02X}")).collect::<String>()
    );
    if let Some(id) = module.library_deploy_address() {
        // deploy_address = address; mstore(staging[id], deploy_address)
        call("address", Some("%address".into()), vec![], &mut init, &mut declarations);
        writeln!(
            init,
            "store i256 %address, ptr addrspace(1) inttoptr (i256 {} to ptr addrspace(1)), align 1",
            staging + id.index() as u64 * 32
        )
        .unwrap();
    }
    if let Some(id) = constructor {
        // end = datasize(Contract); size = codesize - end; codecopy(args_base, end, size)
        call(
            "datasize",
            Some("%end".into()),
            vec![("metadata".into(), "!0".into())],
            &mut init,
            &mut declarations,
        );
        call("codesize", Some("%total".into()), vec![], &mut init, &mut declarations);
        writeln!(init, "%size = sub i256 %total, %end").unwrap();
        copy_memory(
            4,
            false,
            &args_base.to_string(),
            "%end",
            "%size",
            &mut init,
            &mut declarations,
        );
        writeln!(init, "%rounded = add i256 %size, {}\n%free = and i256 %rounded, -32\nstore i256 %free, ptr addrspace(1) inttoptr (i256 64 to ptr addrspace(1)), align 1", args_base + 31).unwrap();
        for i in 0..module.functions[id].params.len() {
            writeln!(init, "%arg{i} = load i256, ptr addrspace(1) inttoptr (i256 {} to ptr addrspace(1)), align 1", args_base + i as u64 * 32).unwrap();
        }
        writeln!(
            init,
            "call void @f{}({})",
            id.index(),
            (0..module.functions[id].params.len())
                .map(|i| format!("i256 %arg{i}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
        .unwrap();
    } else if !module.is_library {
        // callvalue != 0 -> revert; otherwise continue deployment.
        declarations.insert("declare i256 @llvm.evm.callvalue()".into());
        declarations.insert("declare void @llvm.evm.revert(ptr addrspace(1), i256)".into());
        init.push_str("%value = call i256 @llvm.evm.callvalue()\n%payable = icmp eq i256 %value, 0\nbr i1 %payable, label %postlude, label %reject\nreject:\ncall void @llvm.evm.revert(ptr addrspace(1) null, i256 0)\nunreachable\npostlude:\n");
    }
    // deploy = mload(64); codecopy(deploy, runtime, size); patch immutables; return(deploy, size)
    init.push_str("%deploy = load i256, ptr addrspace(1) inttoptr (i256 64 to ptr addrspace(1)), align 1\n%deployptr = inttoptr i256 %deploy to ptr addrspace(1)\n");
    writeln!(init, "call void @llvm.memcpy.p1.p4.i256(ptr addrspace(1) %deployptr, ptr addrspace(4) @runtime, i256 {}, i1 false)", runtime.bytes.len()).unwrap();
    let mut immutable_references = Vec::new();
    for (name, offsets) in &runtime.immutables {
        let id = name
            .strip_prefix('i')
            .and_then(|s| s.parse::<usize>().ok())
            .ok_or("invalid LLVM immutable identifier")?;
        if id >= module.immutable_count() {
            return Err("invalid LLVM immutable identifier".into());
        }
        writeln!(init, "%imm{id} = load i256, ptr addrspace(1) inttoptr (i256 {} to ptr addrspace(1)), align 1", staging + id as u64 * 32).unwrap();
        for &offset in offsets {
            if offset == 0
                || offset.checked_add(32).is_none_or(|end| end > runtime.bytes.len() as u64)
                || runtime.bytes.get(offset as usize - 1) != Some(&0x7f)
            {
                return Err("invalid LLVM immutable offset".into());
            }
            writeln!(init, "%patch{offset} = add i256 %deploy, {offset}\n%patchptr{offset} = inttoptr i256 %patch{offset} to ptr addrspace(1)\nstore i256 %imm{id}, ptr addrspace(1) %patchptr{offset}, align 1").unwrap();
            immutable_references.push(ImmutableRef {
                id: ImmutableId::from_usize(id),
                code_offset: offset as usize - 1,
                type_size: TypeSize::new_int_bits(256),
            });
        }
    }
    writeln!(init, "call void @llvm.evm.return(ptr addrspace(1) %deployptr, i256 {})\nunreachable\n}}\n{functions}", runtime.bytes.len()).unwrap();
    declarations.insert(
        "declare void @llvm.memcpy.p1.p4.i256(ptr addrspace(1), ptr addrspace(4), i256, i1 immarg)"
            .into(),
    );
    declarations.insert("declare void @llvm.evm.return(ptr addrspace(1), i256)".into());
    writeln!(
        init,
        "{}\n{}",
        declarations.iter().cloned().collect::<Vec<_>>().join("\n"),
        attributes()
    )
    .unwrap();
    Ok(EvmArtifact {
        deployment: assemble(&init, optimization, true)?.bytes,
        runtime: runtime.bytes,
        immutable_references,
        backend_ir: Some(format!("; Runtime\n{runtime_text}\n; Deployment\n{init}")),
        ..Default::default()
    })
}

fn attributes() -> &'static str {
    "attributes #0 = { null_pointer_is_valid \"target-features\"=\"+osaka\" }\n!0 = !{!\"Contract\"}\n"
}

fn header() -> &'static str {
    "target datalayout = \"E-p:256:256-i256:256:256-S256-a:256:256\"\ntarget triple = \"evm-unknown-unknown\""
}

fn assemble(
    text: &str,
    optimization: OptimizationMode,
    constructor: bool,
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
) -> Result<NativeArtifact, String> {
    Target::initialize_evm(&InitializationConfig::default());
    inkwell::support::error_handling::install_stack_error_handler(stack_error);
    let llvm = inkwell::context::Context::create();
    let module = llvm
        .create_module_from_ir(MemoryBuffer::create_from_memory_range_copy(
            text.as_bytes(),
            "Contract",
        ))
        .map_err(|e| e.to_string())?;
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

fn function(
    module: &Module,
    f: &Function,
    id: usize,
    constructor: bool,
    out: &mut String,
    declarations: &mut BTreeSet<String>,
) -> Result<(), String> {
    let (return_base, args_base) = super::alternative::memory_layout(module);
    let ret = if f.returns.is_empty() { "void" } else { "i256" };
    // define fN(i256 a0, ...) { block0: instructions; terminator }
    writeln!(
        out,
        "define internal {ret} @f{id}({}) #0 {{",
        (0..f.params.len()).map(|i| format!("i256 %a{i}")).collect::<Vec<_>>().join(", ")
    )
    .unwrap();
    for (bid, block) in f.blocks.iter_enumerated() {
        writeln!(out, "b{}:", bid.index()).unwrap();
        if bid.index() == 0 && super::alternative::frame_size(f) != 0 {
            // frame = mload(64); mstore(64, frame + activation_size)
            writeln!(out, "%frame = load i256, ptr addrspace(1) inttoptr (i256 64 to ptr addrspace(1)), align 1\n%frame_end = add i256 %frame, {}\nstore i256 %frame_end, ptr addrspace(1) inttoptr (i256 64 to ptr addrspace(1)), align 1", super::alternative::frame_size(f)).unwrap();
        }
        if bid.index() == 0 && f.params.is_empty() {
            // aN = call llvm.evm.calldataload (4 + 32 * argument_index)
            for arg in f.arg_indices() {
                if f.selector.is_none() {
                    return Err("unsupported lazy LLVM argument".into());
                }
                declarations.insert("declare i256 @llvm.evm.calldataload(ptr addrspace(2))".into());
                writeln!(out, "%a{} = call i256 @llvm.evm.calldataload(ptr addrspace(2) inttoptr (i256 {} to ptr addrspace(2)))", arg.index(), 4 + arg.index() * 32).unwrap();
            }
        }
        for &iid in &block.instructions {
            let inst = f.inst(iid);
            let result = f.inst_result_value(iid).map(|v| format!("%v{}", v.index()));
            if let InstKind::Phi(inputs) = &inst.kind {
                writeln!(
                    out,
                    "{} = phi i256 {}",
                    result.ok_or("phi without result")?,
                    inputs
                        .iter()
                        .map(|&(b, v)| Ok(format!("[ {}, %b{} ]", value(f, v)?, b.index())))
                        .collect::<Result<Vec<_>, String>>()?
                        .join(", ")
                )
                .unwrap();
                continue;
            }
            let args =
                inst.operands().iter().map(|&v| value(f, v)).collect::<Result<Vec<_>, _>>()?;
            match &inst.kind {
                InstKind::ConstructorArgsBase if constructor => {
                    writeln!(
                        out,
                        "{} = add i256 {args_base}, 0",
                        result.ok_or("boundary without result")?
                    )
                    .unwrap();
                }
                InstKind::ConstructorArgsEnd if constructor => {
                    // end = args_base + codesize - datasize(Contract)
                    let result = result.ok_or("boundary without result")?;
                    call(
                        "datasize",
                        Some(format!("{result}end")),
                        vec![("metadata".into(), "!0".into())],
                        out,
                        declarations,
                    );
                    call("codesize", Some(format!("{result}total")), vec![], out, declarations);
                    writeln!(out, "{result}size = sub i256 {result}total, {result}end\n{result} = add i256 {args_base}, {result}size").unwrap();
                }
                InstKind::LoadImmutable(id) if constructor => {
                    // result = mload(staging[id])
                    writeln!(out, "{} = load i256, ptr addrspace(1) inttoptr (i256 {} to ptr addrspace(1)), align 1", result.ok_or("immutable without result")?, immutable_staging_base(module) + id.index() as u64 * 32).unwrap();
                }
                InstKind::LoadImmutable(id) => {
                    call(
                        "loadimmutable",
                        result,
                        vec![("metadata".into(), format!("!{{!\"i{}\"}}", id.index()))],
                        out,
                        declarations,
                    );
                }
                _ => instruction(module, &inst.kind, result, args, out, declarations)?,
            }
        }
        // LLVM branches retain MIR edges; external exits use noreturn EVM intrinsics.
        match block.terminator.as_ref().ok_or("missing terminator")? {
            Terminator::Jump(b) => writeln!(out, "br label %b{}", b.index()).unwrap(),
            Terminator::Branch { condition, then_block, else_block } => writeln!(
                out,
                "%branch{} = icmp ne i256 {}, 0\nbr i1 %branch{}, label %b{}, label %b{}",
                bid.index(),
                value(f, *condition)?,
                bid.index(),
                then_block.index(),
                else_block.index()
            )
            .unwrap(),
            Terminator::Switch { value: v, default, cases } => {
                writeln!(out, "switch i256 {}, label %b{} [", value(f, *v)?, default.index())
                    .unwrap();
                for &(v, b) in cases {
                    writeln!(out, "i256 {}, label %b{}", value(f, v)?, b.index()).unwrap();
                }
                out.push_str("]\n");
            }
            Terminator::Return { values } => {
                if values.len() > 1 {
                    // mstore(return_base + 32 * i, result_i); mstore(32, return_base)
                    for (i, &v) in values.iter().enumerate() {
                        writeln!(out, "store i256 {}, ptr addrspace(1) inttoptr (i256 {} to ptr addrspace(1)), align 1", value(f, v)?, return_base + i as u64 * 32).unwrap();
                    }
                    writeln!(out, "store i256 {return_base}, ptr addrspace(1) inttoptr (i256 32 to ptr addrspace(1)), align 1").unwrap();
                }
                if let Some(&v) = values.first() {
                    writeln!(out, "ret i256 {}", value(f, v)?).unwrap();
                } else {
                    out.push_str("ret void\n");
                }
            }
            Terminator::TailCall { function, args } => writeln!(
                out,
                "call {} @f{}({})\ncall void @llvm.evm.stop()\nunreachable",
                if module.functions[*function].returns.is_empty() { "void" } else { "i256" },
                function.index(),
                args.iter()
                    .map(|&v| Ok(format!("i256 {}", value(f, v)?)))
                    .collect::<Result<Vec<_>, String>>()?
                    .join(", ")
            )
            .unwrap(),
            Terminator::ReturnData { offset, size } | Terminator::Revert { offset, size } => {
                let name = if matches!(block.terminator, Some(Terminator::ReturnData { .. })) {
                    "return"
                } else {
                    "revert"
                };
                writeln!(
                    out,
                    "%exit{} = inttoptr i256 {} to ptr addrspace(1)",
                    bid.index(),
                    value(f, *offset)?
                )
                .unwrap();
                call(
                    name,
                    None,
                    vec![
                        ("ptr addrspace(1)".into(), format!("%exit{}", bid.index())),
                        ("i256".into(), value(f, *size)?),
                    ],
                    out,
                    declarations,
                );
                out.push_str("unreachable\n");
            }
            Terminator::Stop => out.push_str("ret void\n"),
            Terminator::Invalid => {
                call("invalid", None, vec![], out, declarations);
                out.push_str("unreachable\n");
            }
            Terminator::SelfDestruct { recipient } => {
                call(
                    "selfdestruct",
                    None,
                    vec![("i256".into(), value(f, *recipient)?)],
                    out,
                    declarations,
                );
                out.push_str("unreachable\n");
            }
            Terminator::RevertReturndata => {
                // size = returndatasize; memcpy(heap:0, returndata:0, size); revert(heap:0, size)
                let size = format!("%returndata{}", bid.index());
                call("returndatasize", Some(size.clone()), vec![], out, declarations);
                copy_memory(3, false, "0", "0", &size, out, declarations);
                call(
                    "revert",
                    None,
                    vec![("ptr addrspace(1)".into(), "null".into()), ("i256".into(), size)],
                    out,
                    declarations,
                );
                out.push_str("unreachable\n");
            }
        }
    }
    out.push_str("}\n");
    Ok(())
}

fn instruction(
    module: &Module,
    inst: &InstKind,
    result: Option<String>,
    mut args: Vec<String>,
    out: &mut String,
    declarations: &mut BTreeSet<String>,
) -> Result<(), String> {
    // result = arithmetic lhs rhs, or call an EVM intrinsic with typed operands.
    match inst {
        InstKind::InternalFrameAddr(offset) => {
            writeln!(
                out,
                "{} = add i256 %frame, {offset}",
                result.ok_or("frame address without result")?
            )
            .unwrap();
        }
        InstKind::DataCopy(data, ..) => {
            // offset = ptrtoint(getelementptr @dN, data.offset); memcpy(heap:dest, code:offset, size)
            let offset = format!("%data{}", out.len());
            writeln!(out, "{offset} = ptrtoint ptr addrspace(4) getelementptr (i8, ptr addrspace(4) @d{}, i256 {}) to i256", data.id.index(), data.offset).unwrap();
            copy_memory(4, false, &args[0], &offset, &args[1], out, declarations);
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
            copy_memory(space, space == 1, &args[0], &args[1], &args[2], out, declarations);
        }
        InstKind::Select(..) => {
            // c = icmp ne condition, 0; result = select c, yes, no
            let result = result.ok_or("select without result")?;
            writeln!(
                out,
                "{result}c = icmp ne i256 {}, 0\n{result} = select i1 {result}c, i256 {}, i256 {}",
                args[0], args[1], args[2]
            )
            .unwrap();
        }
        InstKind::Clz(..) => {
            declarations.insert("declare i256 @llvm.ctlz.i256(i256, i1 immarg)".into());
            writeln!(
                out,
                "{} = call i256 @llvm.ctlz.i256(i256 {}, i1 false)",
                result.ok_or("clz without result")?,
                args[0]
            )
            .unwrap();
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
            // Convert each memory/code offset to its intrinsic address space before calling the EVM operation.
            let pointers: &[(usize, u32)] = match inst {
                InstKind::Call { .. } | InstKind::CallCode { .. } => &[(3, 1), (5, 1)],
                InstKind::StaticCall { .. } | InstKind::DelegateCall { .. } => &[(2, 1), (4, 1)],
                InstKind::Create(..) | InstKind::Create2(..) => &[(1, 1)],
                InstKind::ExtCodeCopy(..) => &[(1, 1), (2, 4)],
                _ => &[(0, 1)],
            };
            let mut typed = args.into_iter().map(|a| ("i256".into(), a)).collect::<Vec<_>>();
            for &(index, space) in pointers {
                let pointer = format!("%ptr{}", out.len());
                writeln!(
                    out,
                    "{pointer} = inttoptr i256 {} to ptr addrspace({space})",
                    typed[index].1
                )
                .unwrap();
                typed[index] = (format!("ptr addrspace({space})"), pointer);
            }
            call(inst.mnemonic(), result, typed, out, declarations);
        }
        InstKind::Add(..)
        | InstKind::Sub(..)
        | InstKind::Mul(..)
        | InstKind::And(..)
        | InstKind::Or(..)
        | InstKind::Xor(..) => writeln!(
            out,
            "{} = {} i256 {}, {}",
            result.ok_or("arithmetic without result")?,
            inst.mnemonic(),
            args[0],
            args[1]
        )
        .unwrap(),
        InstKind::Not(..) => {
            writeln!(out, "{} = xor i256 {}, -1", result.ok_or("not without result")?, args[0])
                .unwrap()
        }
        InstKind::Lt(..)
        | InstKind::Gt(..)
        | InstKind::SLt(..)
        | InstKind::SGt(..)
        | InstKind::Eq(..)
        | InstKind::IsZero(..) => {
            let result = result.ok_or("comparison without result")?;
            let predicate = match inst {
                InstKind::Lt(..) => "ult",
                InstKind::Gt(..) => "ugt",
                InstKind::SLt(..) => "slt",
                InstKind::SGt(..) => "sgt",
                _ => "eq",
            };
            if matches!(inst, InstKind::IsZero(..)) {
                args.push("0".into());
            }
            writeln!(out,"{result}cmp = icmp {predicate} i256 {}, {}\n{result} = zext i1 {result}cmp to i256",args[0],args[1]).unwrap();
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
                args.insert(0, "64".into());
            }
            let pointer = format!("%ptr{}", out.len());
            writeln!(out, "{pointer} = inttoptr i256 {} to ptr addrspace({space})", args[0])
                .unwrap();
            if let Some(result) = result {
                writeln!(out, "{result} = load i256, ptr addrspace({space}) {pointer}, align 1")
                    .unwrap();
            } else {
                writeln!(out, "store i256 {}, ptr addrspace({space}) {pointer}, align 1", args[1])
                    .unwrap();
            }
        }
        InstKind::CalldataLoad(..) | InstKind::Keccak256(..) | InstKind::MStore8(..) => {
            let space = if matches!(inst, InstKind::CalldataLoad(..)) { 2 } else { 1 };
            let pointer = format!("%ptr{}", out.len());
            writeln!(out, "{pointer} = inttoptr i256 {} to ptr addrspace({space})", args[0])
                .unwrap();
            let mut typed = vec![(format!("ptr addrspace({space})"), pointer)];
            typed.extend(args.into_iter().skip(1).map(|a| ("i256".into(), a)));
            call(
                if matches!(inst, InstKind::Keccak256(..)) { "sha3" } else { inst.mnemonic() },
                result,
                typed,
                out,
                declarations,
            );
        }
        InstKind::ICall { function, .. } => {
            if let Some(result) = &result {
                write!(out, "{result} = ").unwrap();
            }
            writeln!(
                out,
                "call {} @f{}({})",
                if module.functions[*function].returns.is_empty() { "void" } else { "i256" },
                function.index(),
                args.iter().map(|v| format!("i256 {v}")).collect::<Vec<_>>().join(", ")
            )
            .unwrap();
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
            call(
                inst.mnemonic(),
                result,
                args.into_iter().map(|a| ("i256".into(), a)).collect(),
                out,
                declarations,
            );
        }
        InstKind::PrevRandao => call("difficulty", result, vec![], out, declarations),
        other => return Err(format!("unsupported LLVM instruction `{}`", other.mnemonic())),
    }
    Ok(())
}

fn call(
    name: &str,
    result: Option<String>,
    args: Vec<(String, String)>,
    out: &mut String,
    declarations: &mut BTreeSet<String>,
) {
    let ret = if result.is_some() { "i256" } else { "void" };
    // result = call llvm.evm.opcode(typed operands)
    declarations.insert(format!(
        "declare {ret} @llvm.evm.{name}({})",
        args.iter().map(|(ty, _)| ty.as_str()).collect::<Vec<_>>().join(", ")
    ));
    if let Some(result) = result {
        write!(out, "{result} = ").unwrap();
    }
    writeln!(
        out,
        "call {ret} @llvm.evm.{name}({})",
        args.iter().map(|(ty, v)| format!("{ty} {v}")).collect::<Vec<_>>().join(", ")
    )
    .unwrap();
}

fn value(f: &Function, id: ValueId) -> Result<String, String> {
    Ok(match f.value(id) {
        Value::Immediate(imm) => imm.as_u256().ok_or("non-word immediate")?.to_string(),
        Value::Arg(arg) => format!("%a{}", arg.index()),
        Value::Inst(inst) => {
            format!("%v{}", f.inst_result_value(*inst).ok_or("instruction without result")?.index())
        }
        _ => return Err("undefined value in LLVM lowering".into()),
    })
}

fn copy_memory(
    space: u32,
    overlapping: bool,
    dest: &str,
    source: &str,
    size: &str,
    out: &mut String,
    declarations: &mut BTreeSet<String>,
) {
    let name = if overlapping { "memmove" } else { "memcpy" };
    let tmp = out.len();
    // dest = inttoptr heap_offset; source = inttoptr source_offset; memcpy(dest, source, size)
    writeln!(out, "%dest{tmp} = inttoptr i256 {dest} to ptr addrspace(1)\n%source{tmp} = inttoptr i256 {source} to ptr addrspace({space})\ncall void @llvm.{name}.p1.p{space}.i256(ptr addrspace(1) %dest{tmp}, ptr addrspace({space}) %source{tmp}, i256 {size}, i1 false)").unwrap();
    declarations.insert(format!("declare void @llvm.{name}.p1.p{space}.i256(ptr addrspace(1), ptr addrspace({space}), i256, i1 immarg)"));
}
