use clap::Parser;
use std::path::PathBuf;

/// Blazingly fast Solidity compiler.
#[derive(Parser)]
pub struct Opts {
    /// Files to compile.
    pub paths: Vec<PathBuf>,
}
