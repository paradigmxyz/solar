//! Cross-server Solidity LSP benchmark command line tool.

#![allow(clippy::disallowed_methods)]

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::{collections::BTreeSet, path::PathBuf, time::Duration};

mod config;
mod fixture;
mod lifecycle;
mod process;
mod protocol;
mod report;
mod runner;

#[derive(Parser)]
#[command(name = "solar-lsp-bench", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Fetch pinned sources and build or install benchmark dependencies.
    Prepare {
        /// Versioned benchmark manifest.
        #[arg(long, default_value = "tools/lsp-bench/benchmark.yaml")]
        config: PathBuf,
        /// Restrict preparation to these server ids (repeatable).
        #[arg(long = "server", value_name = "ID")]
        servers: Vec<String>,
        /// Restrict preparation to these fixture ids (repeatable).
        #[arg(long = "fixture", value_name = "ID")]
        fixtures: Vec<String>,
        /// Prepare selected fixtures without fetching or installing servers.
        #[arg(long, conflicts_with = "servers")]
        fixtures_only: bool,
    },
    /// Audit manifests, artifacts, fixtures, and the execution environment.
    Doctor {
        /// Versioned benchmark manifest.
        #[arg(long, default_value = "tools/lsp-bench/benchmark.yaml")]
        config: PathBuf,
        /// Require the authoritative Linux/cgroup environment and a clean tree.
        #[arg(long)]
        publish: bool,
    },
    /// Run all selected servers through the same manifest-defined workloads.
    Run {
        /// Versioned benchmark manifest.
        #[arg(long, default_value = "tools/lsp-bench/benchmark.yaml")]
        config: PathBuf,
        /// Independent process runs per server and workload.
        #[arg(long, default_value_t = 0)]
        repeat: usize,
        /// Per-operation and shutdown timeout.
        #[arg(long, default_value_t = 0)]
        timeout_secs: u64,
        /// Sampling profile from the benchmark manifest.
        #[arg(long, default_value = "default")]
        profile: String,
        /// Directory for raw samples and summaries.
        #[arg(long, default_value = "target/lsp-bench/latest")]
        output: PathBuf,
        /// Restrict execution to these server ids (repeatable).
        #[arg(long = "server", value_name = "ID")]
        servers: Vec<String>,
        /// Restrict execution to these workload ids (repeatable).
        #[arg(long = "workload", value_name = "ID")]
        workloads: Vec<String>,
        /// Write all reports but exit successfully when samples fail.
        #[arg(long)]
        allow_failures: bool,
    },
    /// Regenerate Markdown from an existing summary JSON.
    Report {
        /// Summary JSON produced by `run`.
        #[arg(long, default_value = "target/lsp-bench/latest/summary.json")]
        input: PathBuf,
        /// Markdown report destination.
        #[arg(long, default_value = "COMPARISON.md")]
        output: PathBuf,
        /// Refuse to generate a publication comparison from portable results.
        #[arg(long)]
        require_authoritative: bool,
    },
    /// Validate result provenance, matrix completeness, and raw sample consistency.
    ValidateResults {
        /// Versioned benchmark manifest.
        #[arg(long, default_value = "tools/lsp-bench/benchmark.yaml")]
        config: PathBuf,
        /// Directory containing summary.json, samples.json, and samples.jsonl.
        #[arg(long, default_value = "target/lsp-bench/authoritative")]
        input: PathBuf,
        /// Sampling profile whose manifest-defined matrix must be complete.
        #[arg(long, default_value = "publish")]
        profile: String,
        /// Require authoritative process and isolation evidence.
        #[arg(long)]
        require_authoritative: bool,
    },
    /// Compare a candidate summary with a compatible baseline summary.
    Compare {
        /// Summary JSON from the latest successful base-branch run.
        #[arg(long)]
        baseline: PathBuf,
        /// Summary JSON from the candidate run.
        #[arg(long)]
        candidate: PathBuf,
        /// Percentage deadband applied to both p50 and p95.
        #[arg(long, default_value_t = 10.0)]
        threshold_pct: f64,
        /// Minimum samples required for each compared metric.
        #[arg(long, default_value_t = 2)]
        min_samples: usize,
        /// Markdown comparison destination.
        #[arg(long, default_value = "target/lsp-bench/comparison.md")]
        output: PathBuf,
        /// Machine-readable comparison destination.
        #[arg(long, default_value = "target/lsp-bench/comparison.json")]
        json_output: PathBuf,
        /// Return an error after writing reports when regressions are found.
        #[arg(long)]
        fail_on_regression: bool,
    },
}

fn main() -> Result<()> {
    let Cli { command } = Cli::parse();
    match command {
        Command::Prepare { config, servers, fixtures, fixtures_only } => {
            let report = lifecycle::prepare(lifecycle::PrepareOptions {
                config,
                servers: servers.into_iter().collect(),
                fixtures: fixtures.into_iter().collect(),
                prepare_servers: !fixtures_only,
            })?;
            print!("{}", lifecycle::render_doctor(&report));
        }
        Command::Doctor { config, publish } => {
            let report = lifecycle::doctor(lifecycle::DoctorOptions { config, publish })?;
            print!("{}", lifecycle::render_doctor(&report));
        }
        Command::Run {
            config,
            repeat,
            timeout_secs,
            profile,
            output,
            servers,
            workloads,
            allow_failures,
        } => {
            let network_isolation = config::Config::load(&config)?
                .profiles
                .get(&profile)
                .with_context(|| format!("benchmark profile `{profile}` is not defined"))?
                .network_isolation;
            process::ensure_network_namespace(network_isolation)?;
            let outcome = runner::run(runner::RunOptions {
                config,
                repeat,
                timeout: Duration::from_secs(timeout_secs),
                profile,
                output: output.clone(),
                servers: servers.into_iter().collect::<BTreeSet<_>>(),
                workloads: workloads.into_iter().collect::<BTreeSet<_>>(),
            })?;
            print!("{}", report::terminal(&outcome.summary));
            println!("Reports: {}", output.display());
            if outcome.authority_failure {
                anyhow::bail!(
                    "benchmark profile requires authoritative process accounting; reports were retained"
                )
            }
            if outcome.failed_runs != 0 {
                eprintln!(
                    "{} benchmark sample(s) were excluded from performance statistics",
                    outcome.failed_runs
                );
                if !allow_failures {
                    anyhow::bail!("benchmark contains failing samples; reports were retained")
                }
            }
        }
        Command::Report { input, output, require_authoritative } => {
            report::regenerate_markdown(&input, &output, require_authoritative)?;
            println!("Report: {}", output.display());
        }
        Command::ValidateResults { config, input, profile, require_authoritative } => {
            report::validate_results_directory(&config, &input, &profile, require_authoritative)?;
            println!("Validated results: {}", input.display());
        }
        Command::Compare {
            baseline,
            candidate,
            threshold_pct,
            min_samples,
            output,
            json_output,
            fail_on_regression,
        } => {
            let comparison =
                report::compare_files(&baseline, &candidate, threshold_pct, min_samples)?;
            report::write_comparison(&comparison, &output, &json_output)?;
            println!("Comparison: {}", output.display());
            println!("Comparison JSON: {}", json_output.display());
            if fail_on_regression && comparison.has_regressions() {
                anyhow::bail!("benchmark comparison contains regressions; reports were retained")
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_exposes_prepare_doctor_run_and_report() {
        for command in ["prepare", "doctor", "run", "report", "validate-results"] {
            assert!(Cli::try_parse_from(["solar-lsp-bench", command]).is_ok(), "{command}");
        }
        assert!(
            Cli::try_parse_from([
                "solar-lsp-bench",
                "compare",
                "--baseline",
                "baseline.json",
                "--candidate",
                "candidate.json",
            ])
            .is_ok()
        );
        assert!(Cli::try_parse_from(["solar-lsp-bench", "prepare", "--fixtures-only"]).is_ok());
    }
}
