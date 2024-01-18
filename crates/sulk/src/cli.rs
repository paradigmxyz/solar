use clap::{ColorChoice, Parser, ValueHint};
use std::path::PathBuf;
use sulk_config::{EvmVersion, Language};

const VERSION_MESSAGE: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("VERGEN_GIT_SHA"),
    " ",
    env!("VERGEN_BUILD_DATE"),
    ")"
);

/// Blazingly fast Solidity compiler.
#[derive(Parser)]
#[clap(
    name = "sulk",
    version = VERSION_MESSAGE,
    // after_help = "Find more information in the book: http://book.getfoundry.sh/reference/forge/forge.html",
    next_display_order = None,
)]
pub struct Args {
    /// Files to compile.
    #[arg(value_hint = ValueHint::FilePath, required = true)]
    pub input: Vec<PathBuf>,
    /// Directory to search for files.
    #[arg(long, short = 'I')]
    pub import_path: Vec<PathBuf>,
    /// Map to search for files [format: map=path]
    #[arg(long, short = 'm')]
    pub import_map: Vec<ImportMap>,
    /// Source code language.
    #[arg(long, value_enum, default_value_t)]
    pub language: Language,
    /// EVM version.
    #[arg(long, value_enum, default_value_t)]
    pub evm_version: EvmVersion,

    /// Coloring.
    #[arg(long, value_enum, default_value_t)]
    pub color: ColorChoice,
    /// Use verbose output.
    #[arg(long, short)]
    pub verbose: bool,
    /// How errors and other messages are produced.
    #[arg(long, value_enum, default_value_t)]
    pub error_format: ErrorFormat,

    // ############################################################################################
    // Internal options - WARNING: these are completely unstable, and may change at any time.
    // ############################################################################################
    #[doc(hidden)]
    #[arg(long, hide = true)]
    pub test_mode: Option<TestMode>,
    /// Prints a note for every diagnostic that is emitted with the creation and emission location.
    ///
    /// This is enabled by default on debug builds.
    #[doc(hidden)]
    #[arg(long, hide = true)]
    pub track_diagnostics: bool,
}

#[doc(hidden)]
#[derive(Clone, Debug, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum TestMode {
    Ui,
    Integration,
}

/// How errors and other messages are produced.
#[derive(Clone, Debug, Default, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum ErrorFormat {
    #[default]
    Human,
    Json,
    PrettyJson,
}

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
