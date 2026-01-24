#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(docsrs, feature(doc_cfg))]

use clap::Parser as _;
use solar_config::CompilerOutput;
use solar_interface::{Result, Session};
use solar_sema::CompilerRef;
use std::ops::ControlFlow;

pub use solar_config::{self as config, Opts, UnstableOpts, version};

pub mod utils;

pub mod standard_json;

#[cfg(all(unix, any(target_env = "gnu", target_os = "macos")))]
pub mod signal_handler;

/// Signal handler to extract a backtrace from stack overflow.
///
/// This is a no-op because this platform doesn't support our signal handler's requirements.
#[cfg(not(all(unix, any(target_env = "gnu", target_os = "macos"))))]
pub mod signal_handler {
    #[cfg(unix)]
    use libc as _;

    /// No-op function.
    pub fn install() {}
}

// `asm` feature.
use alloy_primitives as _;

use tracing as _;

pub fn parse_args<I, T>(itr: I) -> Result<Opts, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let mut opts = Opts::try_parse_from(itr)?;
    opts.finish()?;
    Ok(opts)
}

pub fn run_compiler_args(opts: Opts) -> Result {
    // Handle --standard-json mode
    if opts.standard_json {
        if let Err(e) = standard_json::run_standard_json() {
            eprintln!("Error in standard-json mode: {e}");
            return Err(solar_interface::diagnostics::ErrorGuaranteed::new_unchecked());
        }
        return Ok(());
    }

    run_compiler_with(opts, run_default)
}

fn run_default(compiler: &mut CompilerRef<'_>) -> Result {
    let sess = compiler.gcx().sess;
    if sess.opts.language.is_yul() && !sess.opts.unstable.parse_yul {
        return Err(sess.dcx.err("Yul is not supported yet").emit());
    }

    let mut pcx = compiler.parse();

    // Partition arguments into three categories:
    // - `stdin`: `-`, occurrences after the first are ignored
    // - remappings: `[context:]prefix=path`, already parsed as part of `Opts`
    // - paths: everything else
    let mut seen_stdin = false;
    let mut paths = Vec::new();
    for arg in sess.opts.input.iter().map(String::as_str) {
        if arg == "-" {
            if !seen_stdin {
                pcx.load_stdin()?;
            }
            seen_stdin = true;
            continue;
        }

        if arg.contains('=') {
            continue;
        }

        paths.push(arg);
    }

    pcx.par_load_files(paths)?;

    pcx.parse();

    if compiler.gcx().sources.is_empty() {
        let msg = "no files found";
        let note = "if you wish to use the standard input, please specify `-` explicitly";
        return Err(sess.dcx.err(msg).note(note).emit());
    }

    let ControlFlow::Continue(()) = compiler.lower_asts()? else { return Ok(()) };
    compiler.drop_asts();
    let ControlFlow::Continue(()) = compiler.analysis()? else { return Ok(()) };

    // Handle bytecode emit if requested
    let needs_bytecode = sess
        .opts
        .emit
        .iter()
        .any(|e| matches!(e, CompilerOutput::Bin | CompilerOutput::BinRuntime));

    if needs_bytecode {
        emit_bytecode(compiler)?;
    }

    Ok(())
}

/// Emit bytecode (and optionally ABI/hashes) for all contracts using solar-codegen.
fn emit_bytecode(compiler: &mut CompilerRef<'_>) -> Result {
    use solar_codegen::{EvmCodegen, FxHashMap, lower};
    use solar_sema::hir::ContractId;
    use std::collections::BTreeMap;

    let gcx = compiler.gcx();
    let sess = gcx.sess;

    let emit_abi = sess.opts.emit.contains(&CompilerOutput::Abi);
    let emit_bin = sess.opts.emit.contains(&CompilerOutput::Bin);
    let emit_bin_runtime = sess.opts.emit.contains(&CompilerOutput::BinRuntime);
    let emit_hashes = sess.opts.emit.contains(&CompilerOutput::Hashes);

    // Two-pass compilation to support `type(Contract).creationCode` and `new Contract()`:
    // Pass 1: Compile all contracts to get their bytecodes
    let mut all_bytecodes: FxHashMap<ContractId, Vec<u8>> = FxHashMap::default();
    for id in gcx.hir.contract_ids() {
        let contract = gcx.hir.contract(id);
        if !contract.kind.is_interface() && !contract.kind.is_abstract_contract() {
            let mut module = lower::lower_contract(gcx, id);
            let mut codegen = EvmCodegen::new();
            let (deployment_bytecode, _) = codegen.generate_deployment_bytecode(&mut module);
            all_bytecodes.insert(id, deployment_bytecode);
        }
    }

    let mut json_output: BTreeMap<String, serde_json::Value> = BTreeMap::new();

    // Pass 2: Recompile with bytecodes available for cross-contract references
    for id in gcx.hir.contract_ids() {
        let contract = gcx.hir.contract(id);
        let name = gcx.contract_fully_qualified_name(id).to_string();
        let mut obj = serde_json::Map::new();

        // Add ABI if requested
        if emit_abi {
            let abi = gcx.contract_abi(id);
            obj.insert("abi".to_string(), serde_json::to_value(&abi).unwrap());
        }

        // Add hashes if requested
        if emit_hashes {
            let mut hashes = BTreeMap::new();
            for f in gcx.interface_functions(id) {
                hashes.insert(
                    gcx.item_signature(f.id.into()).to_string(),
                    alloy_primitives::hex::encode(f.selector),
                );
            }
            obj.insert("hashes".to_string(), serde_json::to_value(&hashes).unwrap());
        }

        // Skip bytecode generation for interfaces and abstract contracts
        if !contract.kind.is_interface() && !contract.kind.is_abstract_contract() {
            // Lower to MIR with all bytecodes available
            let mut module = lower::lower_contract_with_bytecodes(gcx, id, &all_bytecodes);

            // Generate bytecode
            let mut codegen = EvmCodegen::new();
            let (deployment, runtime) = codegen.generate_deployment_bytecode(&mut module);

            if emit_bin {
                obj.insert(
                    "bin".to_string(),
                    serde_json::Value::String(alloy_primitives::hex::encode(&deployment)),
                );
            }

            if emit_bin_runtime {
                obj.insert(
                    "bin-runtime".to_string(),
                    serde_json::Value::String(alloy_primitives::hex::encode(&runtime)),
                );
            }
        }

        json_output.insert(name, serde_json::Value::Object(obj));
    }

    let output_json = serde_json::json!({
        "contracts": json_output,
        "version": solar_config::version::SEMVER_VERSION
    });

    if sess.opts.pretty_json {
        println!("{}", serde_json::to_string_pretty(&output_json).unwrap());
    } else {
        println!("{}", serde_json::to_string(&output_json).unwrap());
    }

    Ok(())
}

fn run_compiler_with(opts: Opts, f: impl FnOnce(&mut CompilerRef<'_>) -> Result + Send) -> Result {
    let mut sess = Session::new(opts);
    sess.infer_language();
    sess.validate()?;

    let mut compiler = solar_sema::Compiler::new(sess);
    compiler.enter_mut(|compiler| {
        let mut r = f(compiler);
        r = r.and(finish_diagnostics(compiler.gcx().sess));
        r
    })
}

fn finish_diagnostics(sess: &Session) -> Result {
    sess.dcx.print_error_count()
}
