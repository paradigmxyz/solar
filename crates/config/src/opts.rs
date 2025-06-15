//! Solar CLI arguments.

use crate::{CompilerOutput, CompilerStage, Dump, ErrorFormat, EvmVersion, Language, Threads};
use std::{num::NonZeroUsize, path::PathBuf};

#[cfg(feature = "clap")]
use clap::{ColorChoice, Parser, ValueHint};

/// Blazingly fast Solidity compiler.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "clap", derive(Parser))]
#[cfg_attr(feature = "clap", command(
    name = "solar",
    version = crate::version::SHORT_VERSION,
    long_version = crate::version::LONG_VERSION,
    arg_required_else_help = true,
))]
#[allow(clippy::manual_non_exhaustive)]
pub struct Opts {
    /// Files to compile, or import remappings.
    ///
    /// `-` specifies standard input.
    ///
    /// Import remappings are specified as `[context:]prefix=path`.
    /// See <https://docs.soliditylang.org/en/latest/path-resolution.html#import-remapping>.
    #[cfg_attr(feature = "clap", arg(value_hint = ValueHint::FilePath))]
    pub input: Vec<String>,
    /// Directory to search for files.
    ///
    /// Can be used multiple times.
    #[cfg_attr(
        feature = "clap",
        arg(
            help_heading = "Input options",
            long,
            short = 'I',
            alias = "import-path",
            value_hint = ValueHint::DirPath,
        )
    )]
    pub include_path: Vec<PathBuf>,
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

    /// Directory to write output files.
    #[cfg_attr(feature = "clap", arg(long, value_hint = ValueHint::DirPath))]
    pub out_dir: Option<PathBuf>,
    /// Comma separated list of types of output for the compiler to emit.
    #[cfg_attr(feature = "clap", arg(long, value_delimiter = ','))]
    pub emit: Vec<CompilerOutput>,

    /// Coloring.
    #[cfg(feature = "clap")] // TODO
    #[cfg_attr(
        feature = "clap",
        arg(help_heading = "Display options", long, value_enum, default_value = "auto")
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
    /// Whether to disable warnings.
    #[cfg_attr(feature = "clap", arg(help_heading = "Display options", long))]
    pub no_warnings: bool,

    /// Unstable flags. WARNING: these are completely unstable, and may change at any time.
    ///
    /// See `-Zhelp` for more details.
    #[doc(hidden)]
    #[cfg_attr(feature = "clap", arg(id = "unstable-features", value_name = "FLAG", short = 'Z'))]
    pub _unstable: Vec<String>,

    /// Parsed unstable flags.
    #[cfg_attr(feature = "clap", arg(skip))]
    pub unstable: UnstableOpts,

    // Allows `Opts { x: y, ..Default::default() }`.
    #[doc(hidden)]
    #[cfg_attr(feature = "clap", arg(skip))]
    pub _non_exhaustive: (),
}

impl Opts {
    /// Returns the number of threads to use.
    #[inline]
    pub fn threads(&self) -> NonZeroUsize {
        self.threads.0
    }

    /// Finishes argument parsing.
    ///
    /// This currently only parses the `-Z` arguments into the `unstable` field, but may be extended
    /// in the future.
    #[cfg(feature = "clap")]
    pub fn finish(&mut self) -> Result<(), clap::Error> {
        if !self._unstable.is_empty() {
            let hack = self._unstable.iter().map(|s| format!("--{s}"));
            self.unstable =
                UnstableOpts::try_parse_from(std::iter::once(String::new()).chain(hack))?;
        }
        Ok(())
    }
}

/// Internal options.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "clap", derive(Parser))]
#[cfg_attr(feature = "clap", clap(
    disable_help_flag = true,
    before_help = concat!(
        "List of all unstable flags.\n",
        "WARNING: these are completely unstable, and may change at any time!\n",
        // TODO: This is pretty hard to fix, as we don't have access to the `Command` in the derive macros.
        "   NOTE: the following flags should be passed on the command-line using `-Z`, not `--`",
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

    /// Print additional information about the compiler's internal state.
    ///
    /// Valid kinds are `ast` and `hir`.
    #[cfg_attr(feature = "clap", arg(long, value_name = "KIND[=PATHS...]"))]
    pub dump: Option<Dump>,

    /// Print AST stats.
    #[cfg_attr(feature = "clap", arg(long))]
    pub ast_stats: bool,

    /// Run the span visitor after parsing.
    #[cfg_attr(feature = "clap", arg(long))]
    pub span_visitor: bool,

    /// Enable warnings for unused imports and declarations.
    #[cfg_attr(feature = "clap", arg(long))]
    pub warn_unused: bool,

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
        Opts::command().debug_assert();
        let _ = Opts::default();
        let _ = Opts { evm_version: EvmVersion::Berlin, ..Default::default() };

        UnstableOpts::command().debug_assert();
        let _ = UnstableOpts::default();
        let _ = UnstableOpts { ast_stats: false, ..Default::default() };
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
                let mut opts = Opts::try_parse_from(args)?;
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

        let unstable = parse(&["solar", "-Zast-stats", "a.sol"]).unwrap();
        assert!(unstable.ast_stats);
    }
}
