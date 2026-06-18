//! Solar test runner.
//!
//! This crate is invoked in `crates/solar/tests.rs` with the path to the `solar` binary.

#![allow(unreachable_pub)]

use eyre::{Result, eyre};
use std::{ffi::OsString, path::Path};
use ui_test::{color_eyre::eyre, spanned::Spanned};

mod errors;
mod solc;
mod utils;

/// Runs all the tests.
///
/// `cmd` is the path to the `solar` binary used by all modes. `Mode::Mir`
/// invokes it as `solar mir-opt …`; `Mode::EvmIr` invokes it as
/// `solar evm-opt …`.
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

    let mut modes = DEFAULT_MODES.to_vec();
    modes.insert(1, Mode::Mir);
    modes.insert(2, Mode::EvmIr);

    // TESTER_MODE can be a single mode or a comma-separated list.
    // The "ui" alias also implicitly runs the "mir" mode, since they share the
    // same `tests/ui/` tree and users typically want `cargo uitest` to cover
    // both.
    if let Ok(mode_str) = std::env::var("TESTER_MODE") {
        let mut requested = Vec::new();
        for name in mode_str.split(',') {
            let m = Mode::parse(name.trim()).ok_or_else(|| eyre!("invalid mode: {name}"))?;
            requested.push(m);
            if name.trim() == "ui" && !requested.contains(&Mode::Mir) {
                requested.push(Mode::Mir);
            }
            if name.trim() == "ui" && !requested.contains(&Mode::EvmIr) {
                requested.push(Mode::EvmIr);
            }
        }
        modes = requested;
    }

    let tmp_dir = tempfile::tempdir()?;
    let tmp_dir = &*Box::leak(tmp_dir.path().to_path_buf().into_boxed_path());
    let configs = modes.iter().copied().map(|mode| config(cmd, &args, mode)).collect();

    let text_emitter: Box<dyn ui_test::status_emitter::StatusEmitter> = args.format.into();
    let gha_name = if modes.len() == 1 { modes[0].to_string() } else { "ui-tests".to_string() };
    let gha_emitter = ui_test::status_emitter::Gha { name: gha_name, group: true };
    let status_emitter = (text_emitter, gha_emitter);

    ui_test::run_tests_generic(
        configs,
        move |path, config| {
            let cfg = MyConfig::<'static> { mode: mode_from_config(config), tmp_dir };
            file_filter(path, config, cfg)
        },
        move |config, contents| {
            let cfg = MyConfig::<'static> { mode: mode_from_config(config), tmp_dir };
            per_file_config(config, contents, cfg)
        },
        status_emitter,
    )?;

    Ok(())
}

fn config(cmd: &'static Path, args: &ui_test::Args, mode: Mode) -> ui_test::Config {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap();

    let path = match mode {
        Mode::Ui | Mode::StandardJson => "tests/ui/",
        Mode::Mir => "tests/ui/codegen/mir/",
        Mode::EvmIr => "tests/ui/codegen/evm-ir/",
        Mode::SolcSolidity => "testdata/solidity/test/",
        Mode::SolcYul => "testdata/solidity/test/libyul/",
    };
    let tests_root = root.join(path);
    assert!(
        tests_root.exists(),
        "tests root directory does not exist: {path};\n\
         you may need to initialize submodules: `git submodule update --init --checkout`"
    );

    let standard_json_script = root.join("scripts/standard-json-filecheck.py");

    let mut config = ui_test::Config {
        // `host` and `target` are used for `//@ ignore-...` comments.
        host: Some(get_host().to_string()),
        target: None,
        root_dir: tests_root,
        program: ui_test::CommandBuilder {
            program: if matches!(mode, Mode::StandardJson) {
                if cfg!(windows) { "python".into() } else { "python3".into() }
            } else {
                cmd.into()
            },
            args: {
                let mut args: Vec<OsString> = match mode {
                    // `Mir` and `EvmIr` modes run subcommands which don't
                    // accept the normal compiler flags.
                    Mode::Mir => vec!["mir-opt".into()],
                    Mode::EvmIr => vec!["evm-opt".into()],
                    Mode::StandardJson => vec![
                        standard_json_script.into_os_string(),
                        "solar-standard-json".into(),
                        cmd.as_os_str().to_os_string(),
                    ],
                    _ => vec!["-j1", "--error-format=rustc-json", "-Zui-testing", "-Zparse-yul"]
                        .into_iter()
                        .map(Into::into)
                        .collect(),
                };
                if mode.is_solc() {
                    args.push("--stop-after=parsing".into());
                }
                args
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

    // `mut` is only used under `#[cfg(windows)]` below.
    #[allow(unused_mut)]
    let mut filters = vec![
        (root.into(), b"ROOT".to_vec()),
        (ui_test::Match::PathBackslash, b"/".to_vec()),
        (
            ui_test::Match::Exact(root.to_string_lossy().replace('\\', "/").into_bytes()),
            b"ROOT".to_vec(),
        ),
    ];
    // Windows paths carry `\\?\` long-path prefixes and `\r` line endings; strip
    // both, ahead of the rewrites above.
    #[cfg(windows)]
    {
        filters.insert(0, (ui_test::Match::Exact(vec![b'\r']), b"".to_vec()));
        filters.insert(0, (ui_test::Match::Exact(br"\\?\".to_vec()), b"".to_vec()));
    }
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
        (r"\\\\", "/"),
        (r"\\/", "/"),
        //
        (&env!("CARGO_PKG_VERSION").replace(".", r"\."), "VERSION"),
    ];
    add_root_stdout_filters(&mut config, root);
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

fn add_root_stdout_filters(config: &mut ui_test::Config, root: &Path) {
    let native = root.to_string_lossy();
    let slash = native.replace('\\', "/");
    let escaped = native.replace('\\', r"\\");
    let mut roots = vec![native.into_owned(), slash.clone(), escaped];
    if let Some((drive, rest)) = slash.split_once(':') {
        roots.push(format!("{}:{rest}", drive.to_ascii_uppercase()));
        roots.push(format!("{}:{rest}", drive.to_ascii_lowercase()));
    }
    roots.sort();
    roots.dedup();
    for root in roots {
        config.stdout_filter(&regex::escape(&root), "ROOT");
    }
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

fn mode_from_config(config: &ui_test::Config) -> Mode {
    if config.program.args.get(1).is_some_and(|arg| arg == "solar-standard-json") {
        Mode::StandardJson
    } else if config.root_dir.ends_with("tests/ui/codegen/mir") {
        Mode::Mir
    } else if config.root_dir.ends_with("tests/ui/codegen/evm-ir") {
        Mode::EvmIr
    } else if config.root_dir.ends_with("testdata/solidity/test/libyul") {
        Mode::SolcYul
    } else if config.root_dir.ends_with("testdata/solidity/test") {
        Mode::SolcSolidity
    } else {
        Mode::Ui
    }
}

fn file_filter(path: &Path, config: &ui_test::Config, cfg: MyConfig<'_>) -> Option<bool> {
    match cfg.mode {
        Mode::Mir => {
            path.extension().filter(|&ext| ext == "mir")?;
        }
        Mode::EvmIr => {
            path.extension().filter(|&ext| ext == "evmir")?;
        }
        Mode::StandardJson => {
            path.extension().filter(|&ext| ext == "jsonc")?;
        }
        _ => {
            path.extension()
                .filter(|&ext| ext == "sol" || (cfg.mode.allows_yul() && ext == "yul"))?;
        }
    }
    if !ui_test::default_any_file_filter(path, config) {
        return Some(false);
    }
    let skip = match cfg.mode {
        Mode::Ui | Mode::Mir | Mode::EvmIr | Mode::StandardJson => false,
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
    if matches!(cfg.mode, Mode::StandardJson) {
        config.comment_defaults.base().require_annotations = Spanned::dummy(false).into();
        config.comment_defaults.base().exit_status = Spanned::dummy(0).into();
        return;
    }

    assert_eq!(config.comment_start, "//");
    let has_annotations = src.contains("//~");
    config.comment_defaults.base().require_annotations = Spanned::dummy(has_annotations).into();
    let code = if has_annotations && src.contains("ERROR:") { 1 } else { 0 };
    config.comment_defaults.base().exit_status = Spanned::dummy(code).into();

    if src.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with("//@")
            && line.contains("compile-flags")
            && (line.contains("-j") || line.contains("--threads"))
    }) {
        config.program.args.retain(|arg| arg != "-j1");
    }
}

// For solc tests, we can't expect errors normally since we have different diagnostics.
// Instead, we check just the error code and ignore other output.
fn solc_per_file_config(config: &mut ui_test::Config, src: &str, path: &Path, cfg: MyConfig<'_>) {
    let expected_errors = errors::Error::load_solc(src);
    let expected_error = expected_errors.iter().find(|e| e.is_error());
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

    if matches!(cfg.mode, Mode::SolcSolidity) {
        let flags = &mut config.comment_defaults.base().compile_flags;
        let has_delimiters = solc::solidity::handle_delimiters(src, path, cfg.tmp_dir, |arg| {
            flags.push(arg.into_string().unwrap())
        });
        if has_delimiters {
            // HACK: skip the input file argument by using a dummy flag.
            config.program.input_file_flag = Some("-I".into());
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Ui,
    /// MIR-level tests: runs `solar mir-opt` on `.mir` files under
    /// `tests/ui/codegen/mir/`.
    Mir,
    /// EVM-IR-level tests: runs `solar evm-opt` on `.evmir` files under
    /// `tests/ui/codegen/evm-ir/`.
    EvmIr,
    StandardJson,
    SolcSolidity,
    SolcYul,
}

const DEFAULT_MODES: &[Mode] = &[Mode::Ui, Mode::StandardJson, Mode::SolcSolidity, Mode::SolcYul];

impl Mode {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "ui" => Self::Ui,
            "mir" => Self::Mir,
            "evm-ir" => Self::EvmIr,
            "standard-json" => Self::StandardJson,
            "solc-solidity" => Self::SolcSolidity,
            "solc-yul" => Self::SolcYul,
            _ => return None,
        })
    }

    fn to_str(self) -> &'static str {
        match self {
            Self::Ui => "ui",
            Self::Mir => "mir",
            Self::EvmIr => "evm-ir",
            Self::StandardJson => "standard-json",
            Self::SolcSolidity => "solc-solidity",
            Self::SolcYul => "solc-yul",
        }
    }

    fn is_solc(self) -> bool {
        matches!(self, Self::SolcSolidity | Self::SolcYul)
    }

    fn allows_yul(self) -> bool {
        !matches!(self, Self::SolcSolidity | Self::Mir | Self::EvmIr)
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
