use solar_smt::{Entrant, Limits, Sexp, SolverKind, Verdict, sexp};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{Receiver, RecvTimeoutError, channel},
    time::{Duration, Instant},
};

pub(crate) enum Res {
    Unsat,
    Sat(Vec<(String, String)>),
    Unknown(String),
}

/// A z3 process fed over a pipe. Z3 does not always honour its own :timeout (nonlinear integer
/// arithmetic in particular), so every answer is read against a wall-clock deadline: on overrun the
/// process is killed, a fresh one is started and the persistent part of the transcript (everything
/// sent outside check) is replayed, so later obligations still run.
///
/// A dry-run `Smt` has no process: it records the transcript and answers every query unknown.
pub(crate) struct Smt {
    z3: Option<Z3>,
    timeout_ms: u64,
    transcript: String,
    pub log: Option<String>,
    pub stats: Stats,
    /// the first error the solver reported for a command (cleared per function)
    error: Option<String>,
}

#[derive(Default, Clone, Debug)]
pub(crate) struct Stats {
    pub queries: usize,
    pub kills: usize,
    pub solver_ms: u128,
    pub cvc5_queries: usize,
    pub wins: std::collections::BTreeMap<String, usize>,
}

struct Z3 {
    child: Child,
    inp: ChildStdin,
    out: Receiver<String>,
    /// The output lines read so far that do not yet make a whole response.
    responses: sexp::Reader,
}

fn spawn(timeout_ms: u64) -> std::io::Result<Z3> {
    let z3 = solar_smt::Solver::find(SolverKind::Z3)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "z3 not found"))?;
    let mut child = Command::new(z3.path)
        .args(["-in", "-smt2"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut inp = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            match line {
                Ok(l) => {
                    if tx.send(l).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    let _ = inp.write_all(
        format!("(set-option :produce-models true)\n(set-option :timeout {timeout_ms})\n(set-option :smt.mbqi true)\n").as_bytes(),
    );
    Ok(Z3 { child, inp, out: rx, responses: sexp::Reader::new() })
}

impl Drop for Smt {
    fn drop(&mut self) {
        if let Some(z3) = &mut self.z3 {
            let _ = z3.child.kill();
            let _ = z3.child.wait();
        }
    }
}

impl Smt {
    pub(crate) fn new(timeout_ms: u64, log: bool) -> std::io::Result<Self> {
        let mut smt = Self::dry(log);
        smt.z3 = Some(spawn(timeout_ms)?);
        smt.timeout_ms = timeout_ms;
        Ok(smt)
    }

    /// An `Smt` that runs no solver (`--dry-run`).
    pub(crate) fn dry(log: bool) -> Self {
        Self {
            z3: None,
            timeout_ms: 0,
            transcript: String::new(),
            log: if log { Some(String::new()) } else { None },
            stats: Stats::default(),
            error: None,
        }
    }

    fn raw(&mut self, s: &str) {
        if let Some(l) = &mut self.log {
            l.push_str(s);
        }
        if let Some(z3) = &mut self.z3 {
            let _ = z3.inp.write_all(s.as_bytes());
        }
    }

    /// Persistent commands (declarations, assertions, push/pop of a function scope).
    pub(crate) fn send(&mut self, s: &str) {
        self.transcript.push_str(s);
        self.raw(s);
    }

    fn restart(&mut self) {
        self.stats.kills += 1;
        let Some(z3) = &mut self.z3 else { return };
        let _ = z3.child.kill();
        let _ = z3.child.wait();
        if let Ok(mut fresh) = spawn(self.timeout_ms) {
            let _ = fresh.inp.write_all(self.transcript.as_bytes());
            *z3 = fresh;
        }
    }

    fn flush(&mut self) {
        if let Some(z3) = &mut self.z3 {
            let _ = z3.inp.flush();
        }
    }

    /// The solver's next response, or `None` past `deadline`.
    fn response(&mut self, deadline: Instant) -> Option<Sexp> {
        let z3 = self.z3.as_mut()?;
        loop {
            match z3.responses.read() {
                Some(Ok(response)) => return Some(response),
                // An unmatched `)`: the reader drops it and goes on.
                Some(Err(_)) => continue,
                None => {}
            }
            let left = deadline.saturating_duration_since(Instant::now());
            match z3.out.recv_timeout(left) {
                Ok(line) => {
                    z3.responses.push(&line);
                    z3.responses.push("\n");
                }
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return None,
            }
        }
    }

    fn read_answer(&mut self, deadline: Instant) -> Option<Verdict> {
        loop {
            let response = self.response(deadline)?;
            if let Some(verdict) = response.as_atom().and_then(Verdict::from_response) {
                return Some(verdict);
            }
            if response.head() == Some("error") {
                eprintln!("internal: z3 {response}");
                // a rejected command means the query is not the one we meant: no answer counts
                if self.error.is_none() {
                    self.error = Some(response.to_string());
                }
            }
        }
    }

    /// Is goal valid under guard (and everything asserted so far)?
    pub(crate) fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }

    pub(crate) fn check_with_timeout(&mut self, guard: &str, goal: &str, ms: u64) -> Res {
        let old = self.timeout_ms;
        self.timeout_ms = ms;
        self.raw(&format!("(set-option :timeout {ms})\n"));
        let r = self.check(guard, goal, &[]);
        self.timeout_ms = old;
        self.raw(&format!("(set-option :timeout {old})\n"));
        r
    }

    /// Forget solver errors seen so far (at the start of a function's scope).
    pub(crate) fn clear_error(&mut self) {
        self.error = None;
    }

    pub(crate) fn check(&mut self, guard: &str, goal: &str, cex: &[(String, String)]) -> Res {
        if self.z3.is_none() {
            self.stats.queries += 1;
            self.raw(&format!("(push 1)\n(assert {guard})\n(assert (not {goal}))\n(check-sat)\n"));
            self.raw("(pop 1)\n");
            return Res::Unknown("unknown (--dry-run)".into());
        }
        let r = self.check_z3(guard, goal, cex);
        if let Some(e) = &self.error {
            return Res::Unknown(format!("internal solver error: {e}"));
        }
        let Res::Unknown(why) = r else { return r };
        match self.race(guard, goal) {
            Some((true, who)) => {
                *self.stats.wins.entry(who).or_default() += 1;
                Res::Unsat
            }
            Some((false, who)) => Res::Unknown(format!("{why}; {who} found a model of the query")),
            None => Res::Unknown(why),
        }
    }

    /// Like Foundry's symbolic engine: when z3 gives up, race other solvers and encodings on the
    /// same query and take the first definite answer. cvc5 gets the integer query; bitwuzla and z3
    /// get its exact bitvector lowering (bvlower), which settles the bit-twiddling and wide
    /// fixed-point arithmetic integer solvers struggle with.
    fn race(&mut self, guard: &str, goal: &str) -> Option<(bool, String)> {
        if std::env::var("SOLAR_VERIFY_RACE").as_deref() == Ok("off") {
            return None;
        }
        let q = format!("(assert {guard})\n(assert (not {goal}))\n(check-sat)\n");
        let int_script =
            format!("(set-logic ALL)\n{}{q}", self.transcript.replace("bv2int", "bv2nat"));
        let mut entrants = vec![];
        let entrant = |label: &str, kind, script: String| {
            solar_smt::Solver::find(kind).map(|solver| Entrant {
                label: label.into(),
                solver,
                script,
            })
        };
        entrants.extend(entrant("cvc5", SolverKind::Cvc5, int_script));
        let lowered = crate::bvlower::lower(&format!("{}{q}", self.transcript));
        if let Some((bv, dropped, w)) = &lowered {
            if std::env::var_os("SOLAR_VERIFY_DEBUG").is_some() {
                eprintln!("bvlower: width {w}, {dropped} assertions dropped, {} bytes", bv.len());
            }
            if let Some(d) = std::env::var_os("SOLAR_VERIFY_BV_DUMP") {
                let _ = std::fs::write(d, bv);
            }
            let quantified = bv.contains("(forall") || bv.contains("(exists");
            if !quantified {
                entrants.extend(entrant("bitwuzla(bv)", SolverKind::Bitwuzla, bv.clone()));
            }
            entrants.extend(entrant("z3(bv)", SolverKind::Z3, bv.clone()));
            if quantified {
                entrants.extend(entrant(
                    "cvc5(bv)",
                    SolverKind::Cvc5,
                    format!("(set-logic ALL)\n{bv}"),
                ));
            }
        }
        let exact = matches!(&lowered, Some((_, 0, _)));
        let t0 = Instant::now();
        let debug = std::env::var_os("SOLAR_VERIFY_DEBUG").is_some();
        let limits = Limits::wall(Duration::from_millis(self.timeout_ms));
        // A model of the bitvector lowering is a model of the query only when it dropped nothing.
        let answer = solar_smt::race(entrants, &limits, |label, outcome| {
            if debug {
                eprintln!(
                    "race: {label} -> {:?} after {} ms",
                    outcome.verdict,
                    t0.elapsed().as_millis()
                );
            }
            match outcome.verdict {
                Verdict::Unsat => true,
                Verdict::Sat => label == "cvc5" || exact,
                _ => false,
            }
        })
        .map(|(label, outcome)| (outcome.verdict == Verdict::Unsat, label));
        self.stats.cvc5_queries += 1;
        self.stats.solver_ms += t0.elapsed().as_millis();
        answer
    }

    fn check_z3(&mut self, guard: &str, goal: &str, cex: &[(String, String)]) -> Res {
        self.stats.queries += 1;
        let t0 = Instant::now();
        let deadline = t0 + Duration::from_millis(self.timeout_ms + 2000);
        self.raw(&format!("(push 1)\n(assert {guard})\n(assert (not {goal}))\n(check-sat)\n"));
        self.flush();
        let Some(ans) = self.read_answer(deadline) else {
            self.restart();
            self.stats.solver_ms += t0.elapsed().as_millis();
            return Res::Unknown(format!("timeout after {} ms (solver killed)", self.timeout_ms));
        };
        let res = match ans {
            Verdict::Unsat => Res::Unsat,
            Verdict::Sat => {
                let mut vals = vec![];
                let deadline = Instant::now() + Duration::from_secs(5);
                for (name, term) in cex {
                    self.raw(&format!("(get-value ({term}))\n"));
                    self.flush();
                    let Some(response) = self.response(deadline) else {
                        break;
                    };
                    // `((term value))`
                    let value = match response.as_list() {
                        Some([pair]) => match pair.as_list() {
                            Some([_, value]) => value.to_string(),
                            _ => pair.to_string(),
                        },
                        _ => response.to_string(),
                    };
                    vals.push((name.clone(), pretty(&value)));
                }
                Res::Sat(vals)
            }
            other => {
                self.raw("(get-info :reason-unknown)\n");
                self.flush();
                let why = self
                    .response(Instant::now() + Duration::from_secs(2))
                    .map(|why| why.to_string())
                    .unwrap_or_default();
                Res::Unknown(format!("{} {why}", other.name()))
            }
        };
        self.raw("(pop 1)\n");
        self.stats.solver_ms += t0.elapsed().as_millis();
        res
    }
}

fn pretty(v: &str) -> String {
    let v = v.trim();
    if let Some(n) = v.strip_prefix("(- ").and_then(|x| x.strip_suffix(')')) {
        return format!("-{n}");
    }
    if v.len() > 120 { format!("{}…", &v[..120]) } else { v.to_string() }
}
