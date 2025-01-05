#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

use clap::Parser as _;
use solar_config::{ErrorFormat, ImportMap};
use solar_interface::{
    diagnostics::{DiagCtxt, DynEmitter, HumanEmitter, JsonEmitter},
    Result, Session, SourceMap,
};
use std::{path::Path, sync::Arc};

pub use solar_config::{self as config, version, Opts, UnstableOpts};

pub mod utils;

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

pub fn parse_args<I, T>(itr: I) -> Result<Opts, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let mut opts = Opts::try_parse_from(itr)?;
    opts.finish()?;
    Ok(opts)
}

pub fn run_compiler_args(opts: Opts) -> Result<()> {
    run_compiler_with(opts, Compiler::run_default)
}

pub struct Compiler {
    pub sess: Session,
}

impl Compiler {
    pub fn run_default(&self) -> Result<()> {
        let Self { sess } = self;

        if sess.opts.language.is_yul() && !sess.opts.unstable.parse_yul {
            return Err(sess.dcx.err("Yul is not supported yet").emit());
        }

        // Partition arguments into three categories:
        // - `stdin`: `-`, occurrences after the first are ignored
        // - remappings: `path=mapped`
        // - paths: everything else
        let stdin = sess.opts.input.iter().any(|arg| *arg == Path::new("-"));
        let non_stdin_args = sess.opts.input.iter().filter(|arg| *arg != Path::new("-"));
        let arg_remappings = non_stdin_args
            .clone()
            .filter_map(|arg| arg.to_str().unwrap_or("").parse::<ImportMap>().ok());
        let paths =
            non_stdin_args.filter(|arg| !arg.as_os_str().as_encoded_bytes().contains(&b'='));

        let mut pcx = solar_sema::ParsingContext::new(sess);
        let remappings = arg_remappings.chain(sess.opts.import_map.iter().cloned());
        for map in remappings {
            pcx.file_resolver.add_import_map(map.map, map.path);
        }
        for path in &sess.opts.import_path {
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

fn run_compiler_with(opts: Opts, f: impl FnOnce(&Compiler) -> Result + Send) -> Result {
    let ui_testing = opts.unstable.ui_testing;
    let source_map = Arc::new(SourceMap::empty());
    let emitter: Box<DynEmitter> = match opts.error_format {
        ErrorFormat::Human => {
            let color = match opts.color {
                clap::ColorChoice::Always => solar_interface::ColorChoice::Always,
                clap::ColorChoice::Auto => solar_interface::ColorChoice::Auto,
                clap::ColorChoice::Never => solar_interface::ColorChoice::Never,
            };
            let human = HumanEmitter::stderr(color)
                .source_map(Some(source_map.clone()))
                .ui_testing(ui_testing);
            Box::new(human)
        }
        ErrorFormat::Json | ErrorFormat::RustcJson => {
            // `io::Stderr` is not buffered.
            let writer = Box::new(std::io::BufWriter::new(std::io::stderr()));
            let json = JsonEmitter::new(writer, source_map.clone())
                .pretty(opts.pretty_json_err)
                .rustc_like(matches!(opts.error_format, ErrorFormat::RustcJson))
                .ui_testing(ui_testing);
            Box::new(json)
        }
    };
    let dcx = DiagCtxt::new(emitter).set_flags(|flags| {
        flags.deduplicate_diagnostics &= !ui_testing;
        flags.track_diagnostics &= !ui_testing;
        flags.track_diagnostics |= opts.unstable.track_diagnostics;
    });

    let mut sess = Session::builder().dcx(dcx).source_map(source_map).opts(opts).build();
    sess.infer_language();
    sess.validate()?;

    let compiler = Compiler { sess };
    compiler.sess.enter_parallel(|| {
        let mut r = f(&compiler);
        r = compiler.finish_diagnostics().and(r);
        r
    })
}
