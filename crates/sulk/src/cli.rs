use clap::{ColorChoice, Parser, ValueEnum, ValueHint};
use std::path::PathBuf;

/// Blazingly fast Solidity compiler.
#[derive(Parser)]
pub struct Args {
    /// File to compile.
    #[arg(value_hint = ValueHint::FilePath)]
    pub input: PathBuf,
    /// Coloring.
    #[arg(long, value_enum, default_value = "auto")]
    pub color: ColorChoice,
    /// Testing utilities.
    #[arg(long, value_enum, default_value = "solidity")]
    pub language: Language,
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
