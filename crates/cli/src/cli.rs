//! Solar CLI arguments.

use clap::{ColorChoice, Parser, ValueHint};
use solar_config::{CompilerOutput, CompilerStage, Dump, EvmVersion, Language};
use std::path::PathBuf;

/// Blazingly fast Solidity compiler.
#[derive(Parser)]
#[command(
    name = "solar",
    version = crate::version::SHORT_VERSION,
    long_version = crate::version::LONG_VERSION,
    arg_required_else_help = true,
)]
#[non_exhaustive]
pub struct Args {
    /// Files to compile or import remappings.
    #[arg(value_hint = ValueHint::FilePath)]
    pub input: Vec<PathBuf>,
    /// Directory to search for files.
    #[arg(help_heading = "Input options", long, short = 'I', visible_alias = "base-path", value_hint = ValueHint::FilePath)]
    pub import_path: Vec<PathBuf>,
    /// Map to search for files. Can also be provided as a positional argument.
    #[arg(help_heading = "Input options", long, short = 'm', value_name = "MAP=PATH")]
    pub import_map: Vec<ImportMap>,
    /// Source code language. Only Solidity is currently implemented.
    #[arg(help_heading = "Input options", long, value_enum, default_value_t, hide = true)]
    pub language: Language,

    /// Number of threads to use. Zero specifies the number of logical cores.
    #[arg(long, short = 'j', visible_alias = "jobs", default_value = "8")]
    pub threads: usize,
    /// EVM version.
    #[arg(long, value_enum, default_value_t)]
    pub evm_version: EvmVersion,
    /// Stop execution after the given compiler stage.
    #[arg(long, value_enum)]
    pub stop_after: Option<CompilerStage>,

    /// Directory to write output files.
    #[arg(long, value_hint = ValueHint::DirPath)]
    pub out_dir: Option<PathBuf>,
    /// Comma separated list of types of output for the compiler to emit.
    #[arg(long, value_delimiter = ',')]
    pub emit: Vec<CompilerOutput>,

    /// Coloring.
    #[arg(help_heading = "Display options", long, value_enum, default_value = "auto")]
    pub color: ColorChoice,
    /// Use verbose output.
    #[arg(help_heading = "Display options", long, short)]
    pub verbose: bool,
    /// Pretty-print JSON output.
    ///
    /// Does not include errors. See `--pretty-json-err`.
    #[arg(help_heading = "Display options", long)]
    pub pretty_json: bool,
    /// Pretty-print error JSON output.
    #[arg(help_heading = "Display options", long)]
    pub pretty_json_err: bool,
    /// How errors and other messages are produced.
    #[arg(help_heading = "Display options", long, value_enum, default_value_t)]
    pub error_format: ErrorFormat,

    /// Unstable flags. WARNING: these are completely unstable, and may change at any time.
    ///
    /// See `-Zhelp` for more details.
    #[doc(hidden)]
    #[arg(id = "unstable-features", value_name = "FLAG", short = 'Z')]
    pub _unstable: Vec<String>,

    /// Parsed unstable flags.
    #[arg(skip)]
    pub unstable: UnstableFeatures,
}

impl Args {
    /// Finishes argument parsing.
    ///
    /// This currently only parses the `-Z` arguments into the `unstable` field, but may be extended
    /// in the future.
    pub fn finish(&mut self) -> Result<(), clap::Error> {
        if !self._unstable.is_empty() {
            let hack = self._unstable.iter().map(|s| format!("--{s}"));
            self.unstable =
                UnstableFeatures::try_parse_from(std::iter::once(String::new()).chain(hack))?;
        }
        Ok(())
    }
}

/// Internal options.
#[derive(Clone, Debug, Default, Parser)]
#[clap(
    disable_help_flag = true,
    before_help = concat!(
        "List of all unstable flags.\n",
        "WARNING: these are completely unstable, and may change at any time!\n",
        // TODO: This is pretty hard to fix, as we don't have access to the `Command` in the derive macros.
        "   NOTE: the following flags should be passed on the command-line using `-Z`, not `--`",
    ),
    help_template = "{before-help}{all-args}"
)]
#[non_exhaustive]
#[allow(clippy::manual_non_exhaustive)]
pub struct UnstableFeatures {
    /// Enables UI testing mode.
    #[arg(long)]
    pub ui_testing: bool,

    /// Prints a note for every diagnostic that is emitted with the creation and emission location.
    ///
    /// This is enabled by default on debug builds.
    #[arg(long)]
    pub track_diagnostics: bool,

    /// Enables parsing Yul files for testing.
    #[arg(long)]
    pub parse_yul: bool,

    /// Print additional information about the compiler's internal state.
    ///
    /// Valid kinds are `ast` and `hir`.
    #[arg(long, value_name = "KIND[=PATHS...]")]
    pub dump: Option<Dump>,

    /// Print AST stats.
    #[arg(long)]
    pub ast_stats: bool,

    /// Print help.
    #[arg(long, action = clap::ArgAction::Help)]
    help: (),

    #[cfg(test)]
    #[arg(long)]
    test_bool: bool,
    #[cfg(test)]
    #[arg(long)]
    test_value: Option<usize>,
}

/// How errors and other messages are produced.
#[derive(Clone, Debug, Default, clap::ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum ErrorFormat {
    /// Human-readable output.
    #[default]
    Human,
    /// Solc-like JSON output.
    Json,
    /// Rustc-like JSON output.
    RustcJson,
}

/// A single import map, AKA remapping: `map=path`.
#[derive(Clone, Debug)]
pub struct ImportMap {
    pub map: PathBuf,
    pub path: PathBuf,
}

impl std::str::FromStr for ImportMap {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some((a, b)) = s.split_once('=') {
            Ok(Self { map: a.into(), path: b.into() })
        } else {
            Err("missing '='")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn verify_cli() {
        Args::command().debug_assert();
        UnstableFeatures::command().debug_assert();
    }

    #[test]
    fn unstable_features() {
        fn parse(args: &[&str]) -> Result<UnstableFeatures, impl std::fmt::Debug> {
            struct UnwrapDisplay<T>(T);
            impl<T: std::fmt::Display> std::fmt::Debug for UnwrapDisplay<T> {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "\n{}", self.0)
                }
            }
            (|| {
                let mut args = Args::try_parse_from(args)?;
                args.finish()?;
                Ok::<_, clap::Error>(args.unstable)
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
