use clap::{ColorChoice, Parser, ValueHint};
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
}
