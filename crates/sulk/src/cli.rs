use clap::{ColorChoice, Parser, ValueEnum, ValueHint};
use std::path::PathBuf;

/// Blazingly fast Solidity compiler.
#[derive(Parser)]
pub struct Args {
    /// Files to compile.
    #[arg(value_hint = ValueHint::FilePath, required = true)]
    pub input: Vec<PathBuf>,
    /// Language.
    #[arg(long, value_enum, default_value = "solidity")]
    pub language: Language,
    /// Directory to search for files.
    #[arg(long, short = 'I')]
    pub import_path: Vec<PathBuf>,
    /// Map to search for files [format: map=path]
    #[arg(long, short = 'm')]
    pub import_map: Vec<ImportMap>,
    /// Coloring.
    #[arg(long, value_enum, default_value = "auto")]
    pub color: ColorChoice,
}

/// Source code language.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum Language {
    /// Solidity.
    #[default]
    Solidity,
    /// Yul.
    Yul,
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
