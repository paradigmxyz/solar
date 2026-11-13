//! Differential fuzzer for Solar's pipeline.
//!
//! Each case is a Solidity contract and a call sequence. Every call runs on:
//!
//! - Solar's bytecode on revm (the reference the interpreters are compared with),
//! - Solar's final lowered MIR on `solar-mir-interpreter`,
//! - Solar's scheduled and final runtime EVM IR on `solar-evmir-interpreter`,
//! - solc's bytecode on revm,
//! - optionally SolidCore, solidity-lean's source semantics (see `lean.rs`).
//!
//! Outcomes, return and revert data, logs and storage are compared byte for byte. Every
//! divergence is reduced and written as a case file that `replay` runs again.

// The tool reads its own case files and corpus, not compiler sources.
#![allow(clippy::disallowed_methods)]

mod case;
mod compile;
mod evm;
mod grammar;
mod interp;
mod lean;
mod mutate;
mod obs;
mod reduce;
mod spec;

use std::{path::PathBuf, time::Duration};

fn usage() -> ! {
    eprintln!(
        "usage: solar-difffuzz <run|replay|reduce|corpus|specs> [options]\n\
         \n  run      --seed N --iters N --out DIR     generated contracts\
         \n  corpus   --out DIR FILES...               mutations of existing contracts\
         \n  specs    --seed N --iters N --out DIR     mine, verify and runtime-check specs\
         \n  replay   CASE.json                        run a saved case again\
         \n  reduce   CASE.json --out FILE             reduce a saved case\
         \n\nOptions: --solar PATH --solc PATH --verify PATH --lean DIR --lean-solc PATH --solar-arg A --solc-arg A\
         \n         --calls N --timeout SECS --no-reduce"
    );
    std::process::exit(2)
}

#[derive(Clone, Debug)]
pub(crate) struct Cli {
    pub(crate) cmd: String,
    pub(crate) seed: u64,
    pub(crate) iters: u64,
    pub(crate) out: PathBuf,
    pub(crate) files: Vec<PathBuf>,
    pub(crate) tools: compile::Tools,
    pub(crate) verify: Option<String>,
    pub(crate) lean: Option<PathBuf>,
    /// The solc that solidity-lean's importer reads the AST from.
    pub(crate) lean_solc: String,
    pub(crate) calls: usize,
    pub(crate) reduce: bool,
}

fn parse() -> Cli {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_else(|| usage());
    let mut cli = Cli {
        cmd,
        seed: 0,
        iters: 100,
        out: PathBuf::from("difffuzz-out"),
        files: vec![],
        tools: compile::Tools {
            solar: "solar".into(),
            solar_args: vec![],
            solc: "solc".into(),
            solc_args: vec![],
            timeout: Duration::from_secs(60),
        },
        verify: None,
        lean: None,
        lean_solc: "solc-0.8.35".into(),
        calls: 12,
        reduce: true,
    };
    let mut solar_args = vec![];
    let mut solc_args = vec![];
    while let Some(a) = args.next() {
        let mut val = || args.next().unwrap_or_else(|| usage());
        match a.as_str() {
            "--seed" => cli.seed = val().parse().unwrap(),
            "--iters" => cli.iters = val().parse().unwrap(),
            "--out" => cli.out = val().into(),
            "--solar" => cli.tools.solar = val(),
            "--solc" => cli.tools.solc = val(),
            "--verify" => cli.verify = Some(val()),
            "--lean" => cli.lean = Some(val().into()),
            "--lean-solc" => cli.lean_solc = val(),
            "--solar-arg" => solar_args.push(val()),
            "--solc-arg" => solc_args.push(val()),
            "--calls" => cli.calls = val().parse().unwrap(),
            "--timeout" => cli.tools.timeout = Duration::from_secs(val().parse().unwrap()),
            "--no-reduce" => cli.reduce = false,
            f if !f.starts_with("--") => cli.files.push(f.into()),
            _ => usage(),
        }
    }
    cli.tools.solar_args =
        if solar_args.is_empty() { vec!["--evm-version=osaka".into()] } else { solar_args };
    cli.tools.solc_args = if solc_args.is_empty() {
        vec!["--evm-version".into(), "osaka".into(), "--via-ir".into()]
    } else {
        solc_args
    };
    cli
}

fn main() {
    let cli = parse();
    std::fs::create_dir_all(&cli.out).unwrap();
    match cli.cmd.as_str() {
        "run" => case::campaign(&cli),
        "corpus" => mutate::campaign(&cli),
        "specs" => spec::campaign(&cli),
        "replay" => {
            for f in &cli.files {
                case::replay(&cli, f);
            }
        }
        "reduce" => {
            for f in &cli.files {
                reduce::reduce_file(&cli, f);
            }
        }
        _ => usage(),
    }
}
