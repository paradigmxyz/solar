//! Solar CLI arguments.

use crate::{
    ColorChoice, CompilerOutput, CompilerStage, Dump, ErrorFormat, EvmVersion, HumanEmitterKind,
    ImportRemapping, Language, LibraryAddress, OptimizationMode, Threads,
};
use std::{num::NonZeroUsize, path::PathBuf};

#[cfg(feature = "clap")]
use clap::{Parser, ValueHint};

// TODO: implement `allow_paths`.

/// Compilation configuration.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "clap", derive(Parser))]
#[cfg_attr(feature = "clap", command(
    name = "solar",
    version = crate::version::short_version(),
    long_version = crate::version::version(),
    arg_required_else_help = true,
))]
#[allow(clippy::manual_non_exhaustive)]
pub struct CompileOpts {
    /// Files to compile, or import remappings.
    ///
    /// `-` specifies standard input.
    ///
    /// In Standard JSON mode, no input or `-` reads from standard input; otherwise, exactly one
    /// input file may be specified.
    ///
    /// Import remappings are specified as `[context:]prefix=path`.
    /// See <https://docs.soliditylang.org/en/latest/path-resolution.html#import-remapping>.
    // NOTE: Remappings are parsed away into the `import_remappings` field. Use that instead.
    #[cfg_attr(feature = "clap", arg(value_hint = ValueHint::FilePath))]
    pub input: Vec<String>,
    /// Import remappings.
    ///
    /// This is either added manually when constructing the session or parsed from `input` into
    /// this field.
    ///
    /// See <https://docs.soliditylang.org/en/latest/path-resolution.html#import-remapping>.
    #[cfg_attr(feature = "clap", arg(skip))]
    pub import_remappings: Vec<ImportRemapping>,
    /// Use the given path as the root of the source tree.
    #[cfg_attr(
        feature = "clap",
        arg(
            help_heading = "Input options",
            long,
            value_hint = ValueHint::DirPath,
        )
    )]
    pub base_path: Option<PathBuf>,
    /// Directory to search for files.
    ///
    /// Can be used multiple times.
    #[cfg_attr(
        feature = "clap",
        arg(
            help_heading = "Input options",
            name = "include-path",
            value_name = "INCLUDE_PATH",
            long,
            short = 'I',
            alias = "import-path",
            value_hint = ValueHint::DirPath,
        )
    )]
    pub include_paths: Vec<PathBuf>,
    /// Allow a given path for imports.
    #[cfg_attr(
        feature = "clap",
        arg(
            help_heading = "Input options",
            long,
            value_delimiter = ',',
            value_hint = ValueHint::DirPath,
        )
    )]
    pub allow_paths: Vec<PathBuf>,
    /// Source code language. Only Solidity is currently implemented.
    #[cfg_attr(
        feature = "clap",
        arg(help_heading = "Input options", long, value_enum, default_value_t, hide = true)
    )]
    pub language: Language,

    /// Number of threads to use. Zero specifies the number of logical cores.
    #[cfg_attr(feature = "clap", arg(long, short = 'j', visible_alias = "jobs", default_value_t))]
    pub threads: Threads,
    /// EVM version.
    #[cfg_attr(feature = "clap", arg(long, value_enum, default_value_t))]
    pub evm_version: EvmVersion,
    /// Stop execution after the given compiler stage.
    #[cfg_attr(feature = "clap", arg(long, value_enum))]
    pub stop_after: Option<CompilerStage>,
    /// MIR optimization objective.
    #[cfg_attr(feature = "clap", arg(short = 'O', long = "optimize", value_enum, default_value_t))]
    pub optimization: OptimizationMode,

    /// Library addresses for linking, as `LibraryName=0xADDRESS`.
    ///
    /// An optional `path.sol:` prefix on the name is accepted and ignored. A
    /// `public`/`external` library function whose library has a linked address
    /// is called through `DELEGATECALL` at that address instead of being
    /// inlined into the caller.
    #[cfg_attr(
        feature = "clap",
        arg(long = "libraries", value_name = "NAME=ADDRESS", value_delimiter = ',')
    )]
    pub libraries: Vec<LibraryAddress>,

    /// Directory to write output files.
    #[cfg_attr(feature = "clap", arg(long, value_hint = ValueHint::DirPath))]
    pub out_dir: Option<PathBuf>,
    /// Comma separated list of types of output for the compiler to emit.
    #[cfg_attr(feature = "clap", arg(long, value_delimiter = ','))]
    pub emit: Vec<CompilerOutput>,

    /// Switch to Standard JSON input/output mode.
    #[cfg_attr(feature = "clap", arg(long))]
    pub standard_json: bool,

    /// Coloring.
    #[cfg_attr(
        feature = "clap",
        arg(help_heading = "Display options", long, value_parser = ColorChoiceValueParser::default(), default_value = "auto")
    )]
    pub color: ColorChoice,
    /// Use verbose output.
    #[cfg_attr(feature = "clap", arg(help_heading = "Display options", long, short))]
    pub verbose: bool,
    /// Pretty-print JSON output.
    ///
    /// Does not include errors. See `--pretty-json-err`.
    #[cfg_attr(feature = "clap", arg(help_heading = "Display options", long))]
    pub pretty_json: bool,
    /// Pretty-print error JSON output.
    #[cfg_attr(feature = "clap", arg(help_heading = "Display options", long))]
    pub pretty_json_err: bool,
    /// How errors and other messages are produced.
    #[cfg_attr(
        feature = "clap",
        arg(help_heading = "Display options", long, value_enum, default_value_t)
    )]
    pub error_format: ErrorFormat,
    /// Human-readable error message style.
    #[cfg_attr(
        feature = "clap",
        arg(
            help_heading = "Display options",
            long,
            value_name = "VALUE",
            value_enum,
            default_value_t
        )
    )]
    pub error_format_human: HumanEmitterKind,
    /// Terminal width for error message formatting.
    #[cfg_attr(
        feature = "clap",
        arg(help_heading = "Display options", long, value_name = "WIDTH")
    )]
    pub diagnostic_width: Option<usize>,
    /// Whether to disable warnings.
    #[cfg_attr(feature = "clap", arg(help_heading = "Display options", long))]
    pub no_warnings: bool,
    /// Comma separated list of diagnostic codes to allow.
    #[cfg_attr(
        feature = "clap",
        arg(help_heading = "Display options", long, value_name = "CODE", value_delimiter = ',')
    )]
    pub allow: Vec<String>,

    /// Unstable flags. WARNING: these are completely unstable, and may change at any time.
    ///
    /// See `-Zhelp` for more details.
    #[doc(hidden)]
    #[cfg_attr(
        feature = "clap",
        arg(id = "unstable-features", value_name = "FLAG", short = 'Z', global = true)
    )]
    pub _unstable: Vec<String>,

    /// Parsed unstable flags.
    #[cfg_attr(feature = "clap", arg(skip))]
    pub unstable: UnstableOpts,

    // Allows `CompileOpts { x: y, ..Default::default() }`.
    #[doc(hidden)]
    #[cfg_attr(feature = "clap", arg(skip))]
    pub _non_exhaustive: (),
}

impl CompileOpts {
    /// Returns the number of threads to use.
    #[inline]
    pub fn threads(&self) -> NonZeroUsize {
        self.threads.0
    }

    /// Finishes argument parsing.
    #[cfg(feature = "clap")]
    pub fn finish(&mut self) -> Result<(), clap::Error> {
        if self.standard_json {
            if self.input.iter().any(|s| s.contains('=')) {
                return Err(make_clap_error(
                    clap::error::ErrorKind::InvalidValue,
                    "Import remappings are not accepted on the command line in Standard JSON mode.\n\
                     Please put them under 'settings.remappings' in the JSON input.",
                ));
            }
            if self.input.len() > 1 {
                return Err(make_clap_error(
                    clap::error::ErrorKind::TooManyValues,
                    "Too many input files for --standard-json.\n\
                     Please either specify a single file name or provide its content on standard input.",
                ));
            }
        }

        self.import_remappings = self
            .input
            .iter()
            .filter(|s| s.contains('='))
            .map(|s| {
                s.parse::<ImportRemapping>().map_err(|e| {
                    make_clap_error(
                        clap::error::ErrorKind::InvalidValue,
                        format!("invalid remapping {s:?}: {e}"),
                    )
                })
            })
            .collect::<Result<_, _>>()?;
        self.input.retain(|s| !s.contains('='));

        if !self._unstable.is_empty() {
            let hack = self._unstable.iter().map(|s| format!("--{s}"));
            let args = std::iter::once(String::new()).chain(hack);
            self.unstable = UnstableOpts::try_parse_from(args).map_err(|e| {
                override_clap_message(e, |s| {
                    s.replace("solar-config", "solar").replace("error:", "").replace("--", "-Z")
                })
            })?;
        }

        Ok(())
    }
}

// Ideally would be clap::Error::raw but it never prints styled text.
#[cfg(feature = "clap")]
fn override_clap_message(e: clap::Error, f: impl FnOnce(String) -> String) -> clap::Error {
    let msg = f(e.render().ansi().to_string());
    let msg = msg.trim();
    make_clap_error(e.kind(), msg)
}

#[cfg(feature = "clap")]
fn make_clap_error(kind: clap::error::ErrorKind, message: impl std::fmt::Display) -> clap::Error {
    <CompileOpts as clap::CommandFactory>::command().error(kind, message)
}

#[cfg(feature = "clap")]
#[derive(Clone, Default)]
struct ColorChoiceValueParser(clap::builder::EnumValueParser<clap::ColorChoice>);

#[cfg(feature = "clap")]
impl clap::builder::TypedValueParser for ColorChoiceValueParser {
    type Value = ColorChoice;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        self.0.parse_ref(cmd, arg, value).map(map_color_choice)
    }

    fn possible_values(
        &self,
    ) -> Option<Box<dyn Iterator<Item = clap::builder::PossibleValue> + '_>> {
        self.0.possible_values()
    }
}

#[cfg(feature = "clap")]
fn map_color_choice(c: clap::ColorChoice) -> ColorChoice {
    match c {
        clap::ColorChoice::Auto => ColorChoice::Auto,
        clap::ColorChoice::Always => ColorChoice::Always,
        clap::ColorChoice::Never => ColorChoice::Never,
    }
}

/// Internal options.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "clap", derive(Parser))]
#[cfg_attr(feature = "clap", clap(
    disable_help_flag = true,
    before_help = concat!(
        "List of all unstable flags.\n",
        "WARNING: these are completely unstable, and may change at any time!",
    ),
    help_template = "{before-help}{all-args}"
))]
#[allow(clippy::manual_non_exhaustive)]
pub struct UnstableOpts {
    /// Enables UI testing mode.
    #[cfg_attr(feature = "clap", arg(long))]
    pub ui_testing: bool,

    /// Prints a note for every diagnostic that is emitted with the creation and emission location.
    ///
    /// This is enabled by default on debug builds.
    #[cfg_attr(feature = "clap", arg(long))]
    pub track_diagnostics: bool,

    /// Enables parsing Yul files for testing.
    #[cfg_attr(feature = "clap", arg(long))]
    pub parse_yul: bool,

    /// Disables import resolution.
    #[cfg_attr(feature = "clap", arg(long))]
    pub no_resolve_imports: bool,

    /// Recovers incomplete input into a partial AST.
    #[cfg_attr(feature = "clap", arg(long))]
    pub recover_incomplete_input: bool,

    /// Print additional information about the compiler's internal state.
    ///
    /// Valid kinds are `ast`, `hir`, `mir`, `mir-cfg`, `evm-ir`, and `evm-ir-runtime`.
    #[cfg_attr(
        feature = "clap",
        arg(long, require_equals = true, value_name = "KIND[,KIND...][=PATHS...]")
    )]
    pub dump: Option<Dump>,

    /// Print AST stats.
    #[cfg_attr(feature = "clap", arg(long))]
    pub ast_stats: bool,

    /// Print HIR stats.
    #[cfg_attr(feature = "clap", arg(long))]
    pub hir_stats: bool,

    /// Print Standard JSON input stats.
    #[cfg_attr(feature = "clap", arg(long))]
    pub standard_json_stats: bool,

    /// Run the span visitor after parsing.
    #[cfg_attr(feature = "clap", arg(long))]
    pub span_visitor: bool,

    /// Print contracts' max storage sizes.
    #[cfg_attr(feature = "clap", arg(long))]
    pub print_max_storage_sizes: bool,

    /// Print resolved NatSpec docs as diagnostics for UI tests.
    #[cfg_attr(feature = "clap", arg(long))]
    pub print_natspec: bool,

    /// Print MIR or EVM IR after every optimization pass.
    #[cfg_attr(feature = "clap", arg(long))]
    pub print_after_each: bool,

    /// Print a before-and-after diff for each pass explicitly selected by `mir-opt` or `evm-opt`.
    #[cfg_attr(feature = "clap", arg(long))]
    pub pass_diff: bool,

    /// Print the time spent in each MIR and EVM IR pass.
    #[cfg_attr(feature = "clap", arg(long))]
    pub time_passes: bool,

    /// Enable the experimental EVM code generator (MIR lowering and backend).
    ///
    /// Off by default: MIR and EVM IR dumps and bytecode output are only produced
    /// when this is set. Codegen is a work in progress and not yet part of the
    /// compiler's stable, solc-compatible behavior.
    #[cfg_attr(feature = "clap", arg(long))]
    pub codegen: bool,

    /// Disable MIR-phase dispatch lowering.
    ///
    /// Generates the dispatcher inside the EVM backend instead of lowering
    /// dispatch and ABI handling as MIR phases. By default the phases run and
    /// the backend consumes them. A module falls back to the backend
    /// dispatcher when `lower-abi` cannot wrap every external function yet —
    /// the wrappers do not implement returndata encoding, so any returning
    /// external function (getters included) keeps the module on the backend
    /// path — or when there is no external interface at all.
    #[cfg_attr(feature = "clap", arg(long))]
    pub no_mir_dispatch: bool,

    // ----------------------------------------
    // Please add new options above this point!
    // ----------------------------------------
    /// Print help.
    #[cfg_attr(feature = "clap", arg(long, action = clap::ArgAction::Help))]
    pub help: (),

    // Allows `UnstableOpts { x: y, ..Default::default() }`.
    #[doc(hidden)]
    #[cfg_attr(feature = "clap", arg(skip))]
    pub _non_exhaustive: (),

    #[cfg(test)]
    #[cfg_attr(feature = "clap", arg(long))]
    pub test_bool: bool,

    #[cfg(test)]
    #[cfg_attr(feature = "clap", arg(long))]
    pub test_value: Option<usize>,
}

#[cfg(all(test, feature = "clap"))]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn verify_cli() {
        CompileOpts::command().debug_assert();
        let _ = CompileOpts::default();
        let _ = CompileOpts { evm_version: EvmVersion::Berlin, ..Default::default() };

        UnstableOpts::command().debug_assert();
        let _ = UnstableOpts::default();
        let _ = UnstableOpts { ast_stats: false, ..Default::default() };
    }

    #[test]
    fn allow() {
        let mut opts =
            CompileOpts::try_parse_from(["solar", "--allow", "1234,5678", "a.sol"]).unwrap();
        opts.finish().unwrap();

        assert_eq!(opts.allow, ["1234", "5678"]);
    }

    #[test]
    fn standard_json_input() {
        let mut opts = CompileOpts::try_parse_from(["solar", "--standard-json"]).unwrap();
        opts.finish().unwrap();
        assert!(opts.input.is_empty());

        let mut opts = CompileOpts::try_parse_from(["solar", "--standard-json", "-"]).unwrap();
        opts.finish().unwrap();
        assert_eq!(opts.input, ["-"]);

        let mut opts =
            CompileOpts::try_parse_from(["solar", "--standard-json", "input.json"]).unwrap();
        opts.finish().unwrap();
        assert_eq!(opts.input, ["input.json"]);
    }

    #[test]
    fn standard_json_rejects_multiple_inputs() {
        let mut opts =
            CompileOpts::try_parse_from(["solar", "--standard-json", "input1.json", "input2.json"])
                .unwrap();
        let error = opts.finish().unwrap_err().render().ansi().to_string();
        assert!(error.contains("Too many input files for --standard-json."));
    }

    #[test]
    fn standard_json_rejects_remappings() {
        let mut opts = CompileOpts::try_parse_from(["solar", "--standard-json", "a=b"]).unwrap();
        let error = opts.finish().unwrap_err().render().ansi().to_string();
        assert!(error.contains("Import remappings are not accepted on the command line"));
    }

    #[test]
    fn unstable_features() {
        fn parse(args: &[&str]) -> Result<UnstableOpts, impl std::fmt::Debug> {
            struct UnwrapDisplay<T>(T);
            impl<T: std::fmt::Display> std::fmt::Debug for UnwrapDisplay<T> {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "\n{}", self.0)
                }
            }
            (|| {
                let mut opts = CompileOpts::try_parse_from(args)?;
                opts.finish()?;
                Ok::<_, clap::Error>(opts.unstable)
            })()
            .map_err(|e| UnwrapDisplay(e.render().ansi().to_string()))
        }

        let unstable = parse(&["solar", "a.sol"]).unwrap();
        assert!(!unstable.test_bool);

        let unstable = parse(&["solar", "-Ztest-bool", "a.sol"]).unwrap();
        assert!(unstable.test_bool);
        let unstable = parse(&["solar", "-Z", "test-bool", "a.sol"]).unwrap();
        assert!(unstable.test_bool);

        assert!(parse(&["solar", "-Ztest-value", "a.sol"]).is_err());
        assert!(parse(&["solar", "-Z", "test-value", "a.sol"]).is_err());
        assert!(parse(&["solar", "-Ztest-value", "2", "a.sol"]).is_err());
        let unstable = parse(&["solar", "-Ztest-value=2", "a.sol"]).unwrap();
        assert_eq!(unstable.test_value, Some(2));
        let unstable = parse(&["solar", "-Z", "test-value=2", "a.sol"]).unwrap();
        assert_eq!(unstable.test_value, Some(2));

        // DON'T ADD ANY MORE TESTS HERE.
    }
}
