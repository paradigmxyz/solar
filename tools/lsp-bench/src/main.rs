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
        /// Restrict the audit to these server ids (repeatable).
        #[arg(long = "server", value_name = "ID")]
        servers: Vec<String>,
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
        #[arg(long, default_value = "pr-smoke")]
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
        /// Use a workflow-built Solar executable instead of the manifest command.
        #[arg(long, value_name = "PATH", requires = "solar_revision")]
        solar_binary: Option<PathBuf>,
        /// Git revision of the workflow-built Solar executable.
        #[arg(long, value_name = "SHA", requires = "solar_binary")]
        solar_revision: Option<String>,
    },
    /// Regenerate Markdown from an existing summary JSON.
    Report {
        /// Summary JSON produced by `run`.
        #[arg(long, default_value = "target/lsp-bench/latest/summary.json")]
        input: PathBuf,
        /// Markdown report destination.
        #[arg(long, default_value = "target/lsp-bench/latest/summary.md")]
        output: PathBuf,
        /// Refuse to generate a report from a non-authoritative run.
        #[arg(long)]
        require_authoritative: bool,
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
        Command::Doctor { config, servers, publish } => {
            let report = lifecycle::doctor(lifecycle::DoctorOptions {
                config,
                servers: servers.into_iter().collect(),
                publish,
            })?;
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
            solar_binary,
            solar_revision,
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
                solar_binary,
                solar_revision,
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
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_exposes_prepare_doctor_run_and_report() {
        for command in ["prepare", "doctor", "run", "report"] {
            assert!(Cli::try_parse_from(["solar-lsp-bench", command]).is_ok(), "{command}");
        }
        for command in ["compare", "validate-results"] {
            assert!(
                Cli::try_parse_from(["solar-lsp-bench", command]).is_err(),
                "removed command `{command}` was accepted"
            );
        }
        assert!(Cli::try_parse_from(["solar-lsp-bench", "prepare", "--fixtures-only"]).is_ok());
        assert!(Cli::try_parse_from(["solar-lsp-bench", "doctor", "--server", "solar"]).is_ok());
        assert!(
            Cli::try_parse_from([
                "solar-lsp-bench",
                "run",
                "--solar-binary",
                "/tmp/solar",
                "--solar-revision",
                &"a".repeat(40),
            ])
            .is_ok()
        );
        assert!(
            Cli::try_parse_from(["solar-lsp-bench", "run", "--solar-binary", "/tmp/solar"])
                .is_err()
        );
    }
}
