#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

use clap::Parser as _;
use cli::Args;
use solar_interface::{
    diagnostics::{DiagCtxt, DynEmitter, HumanEmitter, JsonEmitter},
    Result, Session, SourceMap,
};
use std::{collections::BTreeSet, num::NonZeroUsize, path::Path, sync::Arc};

pub mod cli;
pub mod utils;
pub mod version;

#[cfg(all(unix, any(target_env = "gnu", target_os = "macos")))]
pub mod sigsegv_handler;

/// Signal handler to extract a backtrace from stack overflow.
///
/// This is a no-op because this platform doesn't support our signal handler's requirements.
#[cfg(not(all(unix, any(target_env = "gnu", target_os = "macos"))))]
pub mod sigsegv_handler {
    /// No-op function.
    pub fn install() {}
}

// `asm` feature.
use alloy_primitives as _;

use tracing as _;

pub fn parse_args<I, T>(itr: I) -> Result<Args, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let mut args = Args::try_parse_from(itr)?;
    args.finish()?;
    Ok(args)
}

pub fn run_compiler_args(args: Args) -> Result<()> {
    run_compiler_with(args, Compiler::run_default)
}

pub struct Compiler {
    pub sess: Session,
    pub args: Args,
}

impl Compiler {
    pub fn run_default(&self) -> Result<()> {
        let Self { sess, args } = self;

        if sess.language.is_yul() && !args.unstable.parse_yul {
            return Err(sess.dcx.err("Yul is not supported yet").emit());
        }

        // Partition arguments into three categories:
        // - `stdin`: `-`, occurrences after the first are ignored
        // - remappings: `path=mapped`
        // - paths: everything else
        let stdin = args.input.iter().any(|arg| *arg == Path::new("-"));
        let non_stdin_args = args.input.iter().filter(|arg| *arg != Path::new("-"));
        let arg_remappings = non_stdin_args
            .clone()
            .filter_map(|arg| arg.to_str().unwrap_or("").parse::<cli::ImportMap>().ok());
        let paths =
            non_stdin_args.filter(|arg| !arg.as_os_str().as_encoded_bytes().contains(&b'='));

        let mut pcx = solar_sema::ParsingContext::new(sess);
        let remappings = arg_remappings.chain(args.import_map.iter().cloned());
        for map in remappings {
            pcx.file_resolver.add_import_map(map.map, map.path);
        }
        for path in &args.import_path {
            let new = pcx.file_resolver.add_import_path(path.clone());
            if !new {
                let msg = format!("import path {} already specified", path.display());
                return Err(sess.dcx.err(msg).emit());
            }
        }

        if stdin {
            pcx.load_stdin()?;
        }
        pcx.load_files(paths)?;

        pcx.parse_and_resolve()?;

        Ok(())
    }

    fn finish_diagnostics(&self) -> Result {
        self.sess.dcx.print_error_count()
    }
}

fn run_compiler_with(args: Args, f: impl FnOnce(&Compiler) -> Result + Send) -> Result {
    utils::run_in_thread_pool_with_globals(args.threads, |jobs| {
        let ui_testing = args.unstable.ui_testing;
        let source_map = Arc::new(SourceMap::empty());
        let emitter: Box<DynEmitter> = match args.error_format {
            cli::ErrorFormat::Human => {
                let color = match args.color {
                    clap::ColorChoice::Always => solar_interface::ColorChoice::Always,
                    clap::ColorChoice::Auto => solar_interface::ColorChoice::Auto,
                    clap::ColorChoice::Never => solar_interface::ColorChoice::Never,
                };
                let human = HumanEmitter::stderr(color)
                    .source_map(Some(source_map.clone()))
                    .ui_testing(ui_testing);
                Box::new(human)
            }
            cli::ErrorFormat::Json | cli::ErrorFormat::RustcJson => {
                let writer = Box::new(std::io::BufWriter::new(std::io::stderr()));
                let json = JsonEmitter::new(writer, source_map.clone())
                    .pretty(args.pretty_json_err)
                    .rustc_like(matches!(args.error_format, cli::ErrorFormat::RustcJson))
                    .ui_testing(ui_testing);
                Box::new(json)
            }
        };
        let dcx = DiagCtxt::new(emitter).set_flags(|flags| {
            flags.deduplicate_diagnostics &= !ui_testing;
            flags.track_diagnostics &= !ui_testing;
            flags.track_diagnostics |= args.unstable.track_diagnostics;
        });

        let mut sess = Session::new(dcx, source_map);
        sess.evm_version = args.evm_version;
        sess.language = args.language;
        sess.stop_after = args.stop_after;
        sess.dump = args.unstable.dump.clone();
        sess.jobs = NonZeroUsize::new(jobs).unwrap();
        if !args.input.is_empty()
            && args.input.iter().all(|arg| arg.extension() == Some("yul".as_ref()))
        {
            sess.language = solar_config::Language::Yul;
        }
        sess.emit = {
            let mut set = BTreeSet::default();
            for &emit in &args.emit {
                if !set.insert(emit) {
                    let msg = format!("cannot specify `--emit {emit}` twice");
                    return Err(sess.dcx.err(msg).emit());
                }
            }
            set
        };
        sess.out_dir = args.out_dir.clone();
        sess.pretty_json = args.pretty_json;

        let compiler = Compiler { sess, args };
        compiler.sess.enter(|| {
            let mut r = f(&compiler);
            r = compiler.finish_diagnostics().and(r);
            r
        })
    })
}
