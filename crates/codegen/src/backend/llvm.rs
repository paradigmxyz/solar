//! Lowers MIR to LLVM IR for solx's statically linked EVM target.
//!
//! Word arithmetic uses i256, with EVM intrinsics for operations whose behavior
//! differs from LLVM on zero divisors or oversized shifts. Address spaces keep
//! heap, calldata, code, and persistent storage separate. LLVM keeps SSA phis
//! and CFG edges until its own optimizer and EVM stack scheduler run.
//! Native emission runs in a child of the statically linked CLI so fatal LLVM
//! failures become diagnostics. Relocations, frames, and spills need more support.

use super::evm::EvmArtifact;
use crate::mir::{Function, InstKind, Module, Terminator, Value, ValueId};
use inkwell::{
    OptimizationLevel,
    memory_buffer::MemoryBuffer,
    passes::PassBuilderOptions,
    targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetTriple},
};
use solar_config::OptimizationMode;
use std::{
    collections::BTreeSet,
    fmt::Write,
    io::{Read, Write as IoWrite},
    path::PathBuf,
    process::{Command, ExitCode, Stdio},
    sync::OnceLock,
};

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
        let mut text = String::new();
        std::io::stdin().read_to_string(&mut text).map_err(|e| e.to_string())?;
        let bytes = assemble_native(&text, optimization)?;
        std::io::stdout().write_all(&bytes).map_err(|e| e.to_string())
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
    if module.immutable_count() != 0 || module.data_count() != 0 || module.is_library {
        return Err(
            "LLVM lowering does not yet support immutable, data, or library relocations".into()
        );
    }
    let entry = module.dispatch_entry().ok_or("missing runtime entry")?;
    let mut declarations = BTreeSet::from(["declare void @llvm.evm.stop()".into()]);
    let mut functions = String::new();
    for (id, f) in module.functions.iter_enumerated() {
        function(f, id.index(), &mut functions, &mut declarations)?;
    }
    // __entry: mstore(64, 128); call dispatch; stop
    let runtime = assemble(
        &format!(
            "{}\ndefine void @__entry() noreturn \"evm-entry-function\" #0 {{\nentry:\nstore i256 128, ptr addrspace(1) inttoptr (i256 64 to ptr addrspace(1)), align 1\ncall void @f{}()\ncall void @llvm.evm.stop()\nunreachable\n}}\n{functions}\n{}\nattributes #0 = {{ null_pointer_is_valid \"target-features\"=\"+osaka\" }}\n",
            header(),
            entry.index(),
            declarations.iter().cloned().collect::<Vec<_>>().join("\n")
        ),
        optimization,
    )?;
    let mut init = format!(
        "{}\n@runtime = private addrspace(4) constant [{} x i8] c\"{}\"\ndefine void @__entry() noreturn \"evm-entry-function\" #0 {{\nentry:\nstore i256 128, ptr addrspace(1) inttoptr (i256 64 to ptr addrspace(1)), align 1\n",
        header(),
        runtime.len(),
        runtime.iter().map(|b| format!("\\{b:02X}")).collect::<String>()
    );
    // __entry: constructor(); memcpy(heap:0, code:runtime, size); return(heap:0, size)
    if let Some((id, _)) =
        module.functions.iter_enumerated().find(|(_, f)| f.attributes.is_constructor)
    {
        writeln!(init, "call void @f{}()", id.index()).unwrap();
    } else {
        // callvalue != 0 -> revert; otherwise continue deployment.
        declarations.insert("declare i256 @llvm.evm.callvalue()".into());
        declarations.insert("declare void @llvm.evm.revert(ptr addrspace(1), i256)".into());
        init.push_str("%value = call i256 @llvm.evm.callvalue()\n%payable = icmp eq i256 %value, 0\nbr i1 %payable, label %deploy, label %reject\nreject:\ncall void @llvm.evm.revert(ptr addrspace(1) null, i256 0)\nunreachable\ndeploy:\n");
    }
    writeln!(init,"call void @llvm.memcpy.p1.p4.i256(ptr addrspace(1) null, ptr addrspace(4) @runtime, i256 {}, i1 false)\ncall void @llvm.evm.return(ptr addrspace(1) null, i256 {})\nunreachable\n}}\n{functions}",runtime.len(),runtime.len()).unwrap();
    declarations.insert(
        "declare void @llvm.memcpy.p1.p4.i256(ptr addrspace(1), ptr addrspace(4), i256, i1 immarg)"
            .into(),
    );
    declarations.insert("declare void @llvm.evm.return(ptr addrspace(1), i256)".into());
    writeln!(
        init,
        "{}\nattributes #0 = {{ null_pointer_is_valid \"target-features\"=\"+osaka\" }}",
        declarations.iter().cloned().collect::<Vec<_>>().join("\n")
    )
    .unwrap();
    Ok(EvmArtifact {
        deployment: assemble(&init, optimization)?,
        runtime,
        backend_ir: Some(init),
        ..Default::default()
    })
}

fn header() -> &'static str {
    "target datalayout = \"E-p:256:256-i256:256:256-S256-a:256:256\"\ntarget triple = \"evm-unknown-unknown\""
}

fn assemble(text: &str, optimization: OptimizationMode) -> Result<Vec<u8>, String> {
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
        Ok(output.stdout)
    })
}

fn assemble_native(text: &str, optimization: OptimizationMode) -> Result<Vec<u8>, String> {
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
    let bytecode = object.link_evm(&Default::default()).map_err(|e| e.to_string())?;
    Ok(bytecode.as_slice().to_vec())
}

fn function(
    f: &Function,
    id: usize,
    out: &mut String,
    declarations: &mut BTreeSet<String>,
) -> Result<(), String> {
    if f.returns.len() > 1 {
        return Err(format!("unsupported LLVM frame or return tuple in `{}`", f.name));
    }
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
            instruction(&inst.kind, result, args, out, declarations)?;
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
                if let Some(&v) = values.first() {
                    writeln!(out, "ret i256 {}", value(f, v)?).unwrap();
                } else {
                    out.push_str("ret void\n");
                }
            }
            Terminator::TailCall { function, args } => writeln!(
                out,
                "call void @f{}({})\ncall void @llvm.evm.stop()\nunreachable",
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
            other => return Err(format!("unsupported LLVM terminator: {other:?}")),
        }
    }
    out.push_str("}\n");
    Ok(())
}

fn instruction(
    inst: &InstKind,
    result: Option<String>,
    mut args: Vec<String>,
    out: &mut String,
    declarations: &mut BTreeSet<String>,
) -> Result<(), String> {
    // result = arithmetic lhs rhs, or call an EVM intrinsic with typed operands.
    match inst {
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
        InstKind::ICall { function, returns, .. } if *returns <= 1 => {
            if let Some(result) = &result {
                write!(out, "{result} = ").unwrap();
            }
            writeln!(
                out,
                "call {} @f{}({})",
                if *returns == 0 { "void" } else { "i256" },
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
        | InstKind::Address => {
            call(
                inst.mnemonic(),
                result,
                args.into_iter().map(|a| ("i256".into(), a)).collect(),
                out,
                declarations,
            );
        }
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
