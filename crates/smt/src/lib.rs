//! SMT solver processes shared by the compiler's provers: the analysis-fact prover
//! (`tools/prove-analyses`) and the deductive verifier (`solar-verify`).
//!
//! [`Solver`] finds Z3, cvc5 or Bitwuzla from one set of environment variables,
//! `SOLAR_Z3`, `SOLAR_CVC5` and `SOLAR_BITWUZLA` (a path, or `off` to disable
//! one), and otherwise from `PATH`. [`Solver::run`] runs one script under
//! [`Limits`]: a wall-clock deadline, which is only a backstop, Z3's deterministic resource
//! limit, and a memory cap. [`race`] runs several solvers on their scripts and returns the
//! first answer its caller accepts, cancelling the rest. [`require_z3`] is the skip
//! convention for tests that need Z3: skip with a message, or fail when
//! `SOLAR_REQUIRE_Z3=1`. [`sexp`] reads the S-expressions of scripts and solver responses.

use std::{
    ffi::OsString,
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

pub mod sexp;
pub use sexp::Sexp;

/// A solver the provers can run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SolverKind {
    Z3,
    Cvc5,
    Bitwuzla,
}

impl SolverKind {
    /// The solver's name, as in its default binary.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Z3 => "z3",
            Self::Cvc5 => "cvc5",
            Self::Bitwuzla => "bitwuzla",
        }
    }

    /// The environment variable that names or disables the solver's binary.
    pub const fn env(self) -> &'static str {
        match self {
            Self::Z3 => "SOLAR_Z3",
            Self::Cvc5 => "SOLAR_CVC5",
            Self::Bitwuzla => "SOLAR_BITWUZLA",
        }
    }
}

/// A solver binary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Solver {
    pub kind: SolverKind,
    pub path: PathBuf,
}

/// What a run may use.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Wall-clock deadline, after which the process is killed.
    pub wall: Duration,
    /// Z3's deterministic resource limit; other solvers ignore it.
    pub rlimit: Option<u64>,
    /// Memory cap in MB, for the solvers that take one.
    pub memory_mb: Option<u64>,
}

impl Limits {
    /// A wall-clock deadline alone.
    pub const fn wall(wall: Duration) -> Self {
        Self { wall, rlimit: None, memory_mb: None }
    }
}

/// A solver's answer to the script's first `check-sat`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Sat,
    Unsat,
    Unknown,
    /// Killed at the wall-clock deadline, or cancelled.
    Timeout,
    /// The solver ran out of memory.
    Memout,
    /// The solver failed or reported an error.
    Error,
}

impl Verdict {
    /// The verdict's name in reports.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Sat => "sat",
            Self::Unsat => "unsat",
            Self::Unknown => "unknown",
            Self::Timeout => "timeout",
            Self::Memout => "memout",
            Self::Error => "error",
        }
    }

    /// The verdict a `check-sat` response line gives.
    pub fn from_response(line: &str) -> Option<Self> {
        match line.trim() {
            "sat" => Some(Self::Sat),
            "unsat" => Some(Self::Unsat),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }

    /// Whether the solver decided the query.
    pub const fn is_definite(self) -> bool {
        matches!(self, Self::Sat | Self::Unsat)
    }
}

/// A finished run.
#[derive(Clone, Debug)]
pub struct Outcome {
    pub verdict: Verdict,
    /// Everything the solver printed, such as `get-value` and `get-model` answers.
    pub stdout: String,
    pub elapsed: Duration,
}

impl Outcome {
    /// The responses after the verdict line, such as `get-value` and `get-model` answers.
    pub fn after_verdict(&self) -> sexp::Parser<'_> {
        let mut rest = self.stdout.as_str();
        while let Some((line, next)) = rest.split_once('\n') {
            rest = next;
            if Verdict::from_response(line).is_some() {
                return sexp::parse(rest);
            }
        }
        sexp::parse("")
    }
}

impl Solver {
    /// The solver `kind`'s environment variable names, or else `kind` on `PATH`;
    /// `None` when the variable is `off` or the binary does not run.
    pub fn find(kind: SolverKind) -> Option<Self> {
        let path = match std::env::var_os(kind.env()) {
            Some(value) if value == "off" => return None,
            Some(value) => PathBuf::from(value),
            None => PathBuf::from(kind.name()),
        };
        Self::at(kind, path)
    }

    /// The solver at `path`, when it runs.
    pub fn at(kind: SolverKind, path: impl Into<PathBuf>) -> Option<Self> {
        let path = path.into();
        let ok = Command::new(&path)
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok();
        ok.then_some(Self { kind, path })
    }

    /// The command that reads an SMT-LIB script on stdin under `limits`, with models.
    pub fn command(&self, limits: &Limits) -> Command {
        let mut command = Command::new(&self.path);
        let wall = limits.wall.as_millis();
        match self.kind {
            SolverKind::Z3 => {
                command.args(["-in", "-smt2"]);
                if let Some(memory) = limits.memory_mb {
                    command.arg(format!("-memory:{memory}"));
                }
            }
            SolverKind::Cvc5 => {
                command.args(["--lang=smt2", "--incremental", "--produce-models"]);
                command.arg(format!("--tlimit={wall}"));
            }
            SolverKind::Bitwuzla => {
                command.args(["--lang", "smt2", "-m"]).arg(format!("--time-limit={wall}"));
                if let Some(memory) = limits.memory_mb {
                    command.arg(format!("--memory-limit={memory}"));
                }
            }
        }
        command
    }

    /// Runs `script` under `limits`.
    pub fn run(&self, script: &str, limits: &Limits) -> Outcome {
        self.run_cancellable(script, limits, &AtomicBool::new(false))
    }

    /// Runs `script` under `limits`, killing the solver once `cancel` is set.
    pub fn run_cancellable(&self, script: &str, limits: &Limits, cancel: &AtomicBool) -> Outcome {
        let start = Instant::now();
        let mut command = self.command(limits);
        command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        let Ok(mut child) = command.spawn() else {
            return Outcome {
                verdict: Verdict::Error,
                stdout: String::new(),
                elapsed: start.elapsed(),
            };
        };
        let mut stdin = child.stdin.take().expect("piped stdin");
        let script = match (self.kind, limits.rlimit) {
            (SolverKind::Z3, Some(rlimit)) => format!(
                "(set-option :rlimit {rlimit})
{script}"
            ),
            _ => script.to_owned(),
        };
        let writer = thread::spawn(move || {
            let _ = stdin.write_all(script.as_bytes());
        });
        let mut stdout = child.stdout.take().expect("piped stdout");
        let reader = thread::spawn(move || {
            let mut output = String::new();
            let _ = stdout.read_to_string(&mut output);
            output
        });
        // Solvers do not always keep their own time limits, so the deadline is enforced here.
        let deadline = start + limits.wall + Duration::from_secs(2);
        let mut killed = false;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline && !cancel.load(Ordering::Relaxed) => {
                    thread::sleep(Duration::from_millis(5));
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    killed = true;
                    break;
                }
            }
        }
        let _ = writer.join();
        let stdout = reader.join().unwrap_or_default();
        let verdict = if killed { Verdict::Timeout } else { parse_verdict(&stdout) };
        Outcome { verdict, stdout, elapsed: start.elapsed() }
    }
}

/// The verdict a solver's output gives for its first `check-sat`. Errors before it, such as an
/// option the solver does not know, do not hide it; without one, an error is `memout` when it
/// is about memory.
pub fn parse_verdict(stdout: &str) -> Verdict {
    let mut verdict = Verdict::Error;
    for line in stdout.lines().map(str::trim) {
        if let Some(answer) = Verdict::from_response(line) {
            return answer;
        }
        if (line.starts_with("(error") || line.starts_with("[error]"))
            && line.to_ascii_lowercase().contains("memory")
        {
            verdict = Verdict::Memout;
        }
    }
    verdict
}

/// One entrant of a [`race`]: a label for reports, the solver and its script.
pub struct Entrant {
    pub label: String,
    pub solver: Solver,
    pub script: String,
}

/// Runs every entrant at once and returns the first outcome `accept` takes, with the label
/// of the entrant that gave it, after cancelling the others; `None` when it takes none.
pub fn race(
    entrants: Vec<Entrant>,
    limits: &Limits,
    accept: impl Fn(&str, &Outcome) -> bool,
) -> Option<(String, Outcome)> {
    let cancel = Arc::new(AtomicBool::new(false));
    let (sender, receiver) = mpsc::channel();
    let count = entrants.len();
    let handles: Vec<_> = entrants
        .into_iter()
        .map(|entrant| {
            let (sender, cancel, limits) = (sender.clone(), cancel.clone(), *limits);
            thread::spawn(move || {
                let outcome = entrant.solver.run_cancellable(&entrant.script, &limits, &cancel);
                let _ = sender.send((entrant.label, outcome));
            })
        })
        .collect();
    drop(sender);
    let mut answer = None;
    for _ in 0..count {
        let Ok((label, outcome)) = receiver.recv() else { break };
        if accept(&label, &outcome) {
            answer = Some((label, outcome));
            break;
        }
    }
    cancel.store(true, Ordering::Relaxed);
    for handle in handles {
        let _ = handle.join();
    }
    answer
}

/// Z3 for a test that needs it: `None` after printing why the test skips, or a panic when
/// `SOLAR_REQUIRE_Z3` is set.
pub fn require_z3() -> Option<Solver> {
    let solver = Solver::find(SolverKind::Z3);
    if solver.is_none() {
        let named: Option<OsString> = std::env::var_os(SolverKind::Z3.env());
        assert!(
            std::env::var_os("SOLAR_REQUIRE_Z3").is_none(),
            "z3 not found (SOLAR_Z3={named:?})"
        );
        eprintln!("skipping: z3 not found; install it or set SOLAR_Z3");
    }
    solver
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdicts() {
        assert_eq!(
            parse_verdict(
                "sat
((x 1))
"
            ),
            Verdict::Sat
        );
        assert_eq!(
            parse_verdict(
                "unsat
"
            ),
            Verdict::Unsat
        );
        assert_eq!(
            parse_verdict(
                "(error \"out of memory\")
"
            ),
            Verdict::Memout
        );
        assert_eq!(
            parse_verdict(
                "(error \"line 1: unknown constant\")
"
            ),
            Verdict::Error
        );
        assert_eq!(parse_verdict(""), Verdict::Error);
        assert_eq!(parse_verdict("(error \"unknown option\")\nunsat\n"), Verdict::Unsat);
    }
}
