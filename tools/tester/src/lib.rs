//! Solar test runner.
//!
//! This crate is invoked in `crates/solar/tests.rs` with the path to the `solar` binary.

#![allow(unreachable_pub)]

use eyre::{Result, eyre};
use std::path::Path;
use ui_test::{color_eyre::eyre, spanned::Spanned};

mod errors;
mod solc;
mod utils;

/// Runs all the tests with the given `solar` command path.
pub fn run_tests(cmd: &'static Path) -> Result<()> {
    ui_test::color_eyre::install()?;

    let mut args = ui_test::Args::test()?;

    // Fast path for `--list`, invoked by `cargo-nextest`.
    {
        let mut dummy_config = ui_test::Config::dummy();
        dummy_config.with_args(&args);
        if ui_test::nextest::emulate(&mut vec![dummy_config]) {
            return Ok(());
        }
    }

    // Condense output if not explicitly requested.
    let requested_pretty = || std::env::args().any(|x| x.contains("--format"));
    if matches!(args.format, ui_test::Format::Pretty) && !requested_pretty() {
        args.format = ui_test::Format::Terse;
    }

    let mut modes = &[Mode::Ui, Mode::SolcSolidity, Mode::SolcYul][..];
    let mode_tmp;
    if let Ok(mode) = std::env::var("TESTER_MODE") {
        mode_tmp = Mode::parse(&mode).ok_or_else(|| eyre!("invalid mode: {mode}"))?;
        modes = std::slice::from_ref(&mode_tmp);
    }

    let tmp_dir = tempfile::tempdir()?;
    let tmp_dir = &*Box::leak(tmp_dir.path().to_path_buf().into_boxed_path());
    for &mode in modes {
        let cfg = MyConfig::<'static> { mode, tmp_dir };
        let config = config(cmd, &args, mode);

        let text_emitter: Box<dyn ui_test::status_emitter::StatusEmitter> = args.format.into();
        let gha_emitter = ui_test::status_emitter::Gha { name: mode.to_string(), group: true };
        let status_emitter = (text_emitter, gha_emitter);

        ui_test::run_tests_generic(
            vec![config],
            move |path, config| file_filter(path, config, cfg),
            move |config, contents| per_file_config(config, contents, cfg),
            status_emitter,
        )?;
    }

    Ok(())
}

fn config(cmd: &'static Path, args: &ui_test::Args, mode: Mode) -> ui_test::Config {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap();

    let path = match mode {
        Mode::Ui => "tests/ui/",
        Mode::SolcSolidity => "testdata/solidity/test/",
        Mode::SolcYul => "testdata/solidity/test/libyul/",
    };
    let tests_root = root.join(path);
    assert!(
        tests_root.exists(),
        "tests root directory does not exist: {path};\n\
         you may need to initialize submodules: `git submodule update --init --checkout`"
    );

    let mut config = ui_test::Config {
        // `host` and `target` are used for `//@ignore-...` comments.
        host: Some(get_host().to_string()),
        target: None,
        root_dir: tests_root,
        program: ui_test::CommandBuilder {
            program: cmd.into(),
            args: {
                let args =
                    vec!["-j1", "--error-format=rustc-json", "-Zui-testing", "-Zparse-yul"];
                args.into_iter().map(Into::into).collect()
            },
            out_dir_flag: None,
            input_file_flag: None,
            envs: vec![],
            cfg_flag: None,
        },
        output_conflict_handling: ui_test::error_on_output_conflict,
        bless_command: Some("cargo uibless".into()),
        out_dir: root.join("target/ui"),
        comment_start: "//",
        diagnostic_extractor: ui_test::diagnostics::rustc::rustc_diagnostics_extractor,
        ..ui_test::Config::dummy()
    };

    macro_rules! register_custom_flags {
        ($($ty:ty),* $(,)?) => {
            $(
                config.custom_comments.insert(<$ty>::NAME, <$ty>::parse);
                if let Some(default) = <$ty>::DEFAULT {
                    config.comment_defaults.base().add_custom(<$ty>::NAME, default);
                }
            )*
        };
    }
    register_custom_flags![];

    config.comment_defaults.base().exit_status = None.into();
    config.comment_defaults.base().require_annotations = Spanned::dummy(true).into();
    config.comment_defaults.base().require_annotations_for_level =
        Spanned::dummy(ui_test::diagnostics::Level::Warn).into();

    let filters = [
        (ui_test::Match::PathBackslash, b"/".to_vec()),
        #[cfg(windows)]
        (ui_test::Match::Exact(vec![b'\r']), b"".to_vec()),
        #[cfg(windows)]
        (ui_test::Match::Exact(br"\\?\".to_vec()), b"".to_vec()),
        (root.into(), b"ROOT".to_vec()),
    ];
    config.comment_defaults.base().normalize_stderr.extend(filters.iter().cloned());
    config.comment_defaults.base().normalize_stdout.extend(filters);

    let filters: &[(&str, &str)] = &[
        // Erase line and column info.
        (r"\.(\w+):[0-9]+:[0-9]+(: [0-9]+:[0-9]+)?", ".$1:LL:CC"),
    ];
    for &(pattern, replacement) in filters {
        config.filter(pattern, replacement);
    }
    let stdout_filters: &[(&str, &str)] = &[
        //
        (&env!("CARGO_PKG_VERSION").replace(".", r"\."), "VERSION"),
    ];
    for &(pattern, replacement) in stdout_filters {
        config.stdout_filter(pattern, replacement);
    }
    let stderr_filters: &[(&str, &str)] = &[];
    for &(pattern, replacement) in stderr_filters {
        config.stderr_filter(pattern, replacement);
    }

    config.with_args(args);

    if mode.is_solc() {
        // Override `bless` handler, since we don't want to write Solc tests.
        config.output_conflict_handling = ui_test::ignore_output_conflict;
        // Skip parsing comments since they result in false positives.
        config.comment_start = "\0";
        config.comment_defaults.base().require_annotations = Spanned::dummy(false).into();
    }

    config
}

fn get_host() -> &'static str {
    static CACHE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let mut config = ui_test::Config::dummy();
        config.program = ui_test::CommandBuilder::rustc();
        config.fill_host_and_target().unwrap();
        config.host.unwrap()
    })
}

fn file_filter(path: &Path, config: &ui_test::Config, cfg: MyConfig<'_>) -> Option<bool> {
    path.extension().filter(|&ext| ext == "sol" || (cfg.mode.allows_yul() && ext == "yul"))?;
    if !ui_test::default_any_file_filter(path, config) {
        return Some(false);
    }
    let skip = match cfg.mode {
        Mode::Ui => false,
        Mode::SolcSolidity => solc::solidity::should_skip(path).is_err(),
        Mode::SolcYul => solc::yul::should_skip(path).is_err(),
    };
    Some(!skip)
}

fn per_file_config(config: &mut ui_test::Config, file: &Spanned<Vec<u8>>, cfg: MyConfig<'_>) {
    let Ok(src) = std::str::from_utf8(&file.content) else {
        return;
    };
    let path = file.span.file.as_path();

    if cfg.mode.is_solc() {
        return solc_per_file_config(config, src, path, cfg);
    }

    assert_eq!(config.comment_start, "//");
    let has_annotations = src.contains("//~");
    // TODO: https://github.com/oli-obk/ui_test/issues/341
    let is_check_fail = src.contains("check-fail");
    config.comment_defaults.base().require_annotations =
        Spanned::dummy(is_check_fail || has_annotations).into();
    let code = if is_check_fail || (has_annotations && src.contains("ERROR:")) { 1 } else { 0 };
    config.comment_defaults.base().exit_status = Spanned::dummy(code).into();
}

// For solc tests, we can't expect errors normally since we have different diagnostics.
// Instead, we check just the error code and ignore other output.
fn solc_per_file_config(config: &mut ui_test::Config, src: &str, path: &Path, cfg: MyConfig<'_>) {
    let expected_errors = errors::Error::load_solc(src);
    let expected_error = expected_errors.iter().find(|e| e.is_error());

    // Enable type checking for tests that expect type errors but no parser errors.
    let has_type_error =
        expected_errors.iter().any(|e| e.solc_kind.is_some_and(|k| k.is_type_error()));
    let has_parser_error =
        expected_errors.iter().any(|e| e.solc_kind.is_some_and(|k| k.is_parser_error()));
    let enable_typeck = has_type_error && !has_parser_error;

    let code = if let Some(expected_error) = expected_error {
        // Expect failure only for parser errors, otherwise ignore exit code.
        if expected_error.solc_kind.is_some_and(|kind| kind.is_parser_error()) {
            Some(1)
        } else {
            None
        }
    } else {
        Some(0)
    };
    config.comment_defaults.base().exit_status = code.map(Spanned::dummy).into();

    let flags = &mut config.comment_defaults.base().compile_flags;
    if enable_typeck {
        flags.push("-Ztypeck".into());
    } else {
        flags.push("--stop-after=parsing".into());
    }

    if matches!(cfg.mode, Mode::SolcSolidity) {
        let has_delimiters = solc::solidity::handle_delimiters(src, path, cfg.tmp_dir, |arg| {
            flags.push(arg.into_string().unwrap())
        });
        if has_delimiters {
            // HACK: skip the input file argument by using a dummy flag.
            config.program.input_file_flag = Some("-I".into());
        }
    }
}

#[derive(Clone, Copy)]
enum Mode {
    Ui,
    SolcSolidity,
    SolcYul,
}

impl Mode {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "ui" => Self::Ui,
            "solc-solidity" => Self::SolcSolidity,
            "solc-yul" => Self::SolcYul,
            _ => return None,
        })
    }

    fn to_str(self) -> &'static str {
        match self {
            Self::Ui => "ui",
            Self::SolcSolidity => "solc-solidity",
            Self::SolcYul => "solc-yul",
        }
    }

    fn is_solc(self) -> bool {
        matches!(self, Self::SolcSolidity | Self::SolcYul)
    }

    fn allows_yul(self) -> bool {
        !matches!(self, Self::SolcSolidity)
    }
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.to_str())
    }
}

#[derive(Clone, Copy)]
struct MyConfig<'a> {
    mode: Mode,
    tmp_dir: &'a Path,
}
