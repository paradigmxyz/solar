use crate::sexp::{self, Sexp};
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
    let mut child = Command::new(std::env::var("SOLAR_VERIFY_Z3").unwrap_or_else(|_| "z3".into()))
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

    /// The answer to a `check-sat`: `sat`, `unsat` or `unknown`.
    fn read_answer(&mut self, deadline: Instant) -> Option<String> {
        loop {
            let response = self.response(deadline)?;
            if let Some(answer @ ("sat" | "unsat" | "unknown")) = response.as_atom() {
                return Some(answer.to_string());
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
        // Under quantified assumptions, z3's SAT-based core first, briefly: it settles most such
        // queries at once, where the default core may spend the whole timeout instantiating.
        let quick = self.timeout_ms.min(QUICK_MS);
        if self.quantified(guard, goal)
            && let Some(r) = self.quantified_answer(guard, goal, cex, quick, true)
        {
            return r;
        }
        let r = self.check_z3(guard, goal, cex);
        if let Some(e) = &self.error {
            return Res::Unknown(format!("internal solver error: {e}"));
        }
        let Res::Unknown(why) = r else { return r };
        if self.quantified(guard, goal)
            && let Some(r) = self.quantified_answer(guard, goal, cex, self.timeout_ms, false)
        {
            return r;
        }
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
        let mut jobs: Vec<(String, String, Vec<String>, String)> = vec![];
        let t = self.timeout_ms;
        let cvc5 = std::env::var("SOLAR_VERIFY_CVC5").unwrap_or_else(|_| "cvc5".into());
        if cvc5 != "off" {
            jobs.push((
                "cvc5".into(),
                cvc5.clone(),
                vec!["--lang=smt2".into(), "--incremental".into(), format!("--tlimit={t}")],
                int_script,
            ));
        }
        let lowered = crate::bvlower::lower(&format!("{}{q}", self.transcript));
        if let Some((bv, dropped, w)) = &lowered {
            if std::env::var_os("SOLAR_VERIFY_DEBUG").is_some() {
                eprintln!("bvlower: width {w}, {dropped} assertions dropped, {} bytes", bv.len());
            }
            if let Some(d) = std::env::var_os("SOLAR_VERIFY_BV_DUMP") {
                let _ = std::fs::write(d, bv);
            }
            let quantified = bv.contains("(forall") || bv.contains("(exists");
            let bw = std::env::var("SOLAR_VERIFY_BITWUZLA").unwrap_or_else(|_| "bitwuzla".into());
            if bw != "off" && !quantified {
                jobs.push((
                    "bitwuzla(bv)".into(),
                    bw,
                    vec![format!("--time-limit={t}")],
                    bv.clone(),
                ));
            }
            jobs.push((
                "z3(bv)".into(),
                std::env::var("SOLAR_VERIFY_Z3").unwrap_or_else(|_| "z3".into()),
                vec!["-in".into(), "-smt2".into(), format!("-t:{t}")],
                bv.clone(),
            ));
            if quantified && cvc5 != "off" {
                jobs.push((
                    "cvc5(bv)".into(),
                    cvc5,
                    vec!["--lang=smt2".into(), format!("--tlimit={t}")],
                    format!("(set-logic ALL)\n{bv}"),
                ));
            }
        }
        let exact = matches!(&lowered, Some((_, 0, _)));
        let t0 = Instant::now();
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (tx, rx) = channel::<(String, Option<bool>)>();
        let n = jobs.len();
        let mut hs = vec![];
        for (name, bin, args, script) in jobs {
            let (tx, cancel) = (tx.clone(), cancel.clone());
            hs.push(std::thread::spawn(move || {
                let r = run_solver(&bin, &args, &script, t + 2000, &cancel);
                let _ = tx.send((name, r));
            }));
        }
        drop(tx);
        let mut answer = None;
        for _ in 0..n {
            let Ok((name, r)) = rx.recv() else { break };
            if std::env::var_os("SOLAR_VERIFY_DEBUG").is_some() {
                eprintln!("race: {name} -> {r:?} after {} ms", t0.elapsed().as_millis());
            }
            match r {
                Some(true) => {
                    answer = Some((true, name));
                    break;
                }
                Some(false) if answer.is_none() && (name == "cvc5" || exact) => {
                    answer = Some((false, name));
                    break;
                }
                _ => {}
            }
        }
        cancel.store(true, std::sync::atomic::Ordering::SeqCst);
        for h in hs {
            let _ = h.join();
        }
        self.stats.cvc5_queries += 1;
        self.stats.solver_ms += t0.elapsed().as_millis();
        answer
    }

    /// An answer for a query z3's default core left unknown under quantified assumptions, such as
    /// a contract invariant over every address. Two attempts, each in a fresh z3 with its newer
    /// SAT-based core (`sat.euf`), whose model-based quantifier instantiation completes array
    /// models the default core keeps refining forever:
    ///
    /// 1. The query as it is; its answer, sat or unsat, stands.
    /// 2. A candidate model of the query with each quantifier cut down to its ground instances (see
    ///    [`crate::inst`]), kept only if the original query is satisfiable with every constant
    ///    pinned to the candidate's value.
    ///
    /// A z3 sat answer is a model that z3 checked against every quantifier, so a reported
    /// counterexample is never an artefact of the instantiation. `check` runs both attempts
    /// briefly before the default core, and the first in full after it.
    fn quantified_answer(
        &mut self,
        guard: &str,
        goal: &str,
        cex: &[(String, String)],
        ms: u64,
        candidate: bool,
    ) -> Option<Res> {
        let q = format!("(assert {guard})\n(assert (not {goal}))\n");
        let t0 = Instant::now();
        let r = match self.euf(&q, "", cex, ms) {
            Some(r) => Some(r),
            None if candidate => self.pinned_candidate(guard, goal, &q, cex, ms),
            None => None,
        };
        self.stats.solver_ms += t0.elapsed().as_millis();
        if std::env::var_os("SOLAR_VERIFY_DEBUG").is_some() {
            let what = match &r {
                Some(Res::Sat(_)) => "sat",
                Some(Res::Unsat) => "unsat",
                _ => "unknown",
            };
            eprintln!("quantified: {what} after {} ms", t0.elapsed().as_millis());
        }
        if r.is_some() {
            *self.stats.wins.entry("z3(euf)".into()).or_default() += 1;
        }
        r
    }

    /// Whether the query or the assertions before it quantify (and the attempts are on).
    fn quantified(&self, guard: &str, goal: &str) -> bool {
        let q = |s: &str| s.contains("(forall") || s.contains("(exists");
        std::env::var("SOLAR_VERIFY_QUANT").as_deref() != Ok("off")
            && (q(&self.transcript) || q(guard) || q(goal))
    }

    /// The second attempt of [`Self::quantified_answer`]: a ground candidate, confirmed.
    fn pinned_candidate(
        &mut self,
        guard: &str,
        goal: &str,
        q: &str,
        cex: &[(String, String)],
        ms: u64,
    ) -> Option<Res> {
        let relaxed = crate::inst::relax(
            &format!("{}{q}(check-sat)\n", self.transcript),
            &format!("{guard} {goal}"),
        )?;
        let model = self.solve(&format!("{EUF}{}", relaxed.script), ms);
        if let Some(d) = std::env::var_os("SOLAR_VERIFY_QUANT_DUMP") {
            let n = self.stats.queries;
            let d = std::path::Path::new(&d);
            let _ = std::fs::write(d.join(format!("{n}.relaxed.smt2")), &relaxed.script);
            let _ = std::fs::write(d.join(format!("{n}.model")), model.as_deref().unwrap_or(""));
            let _ = std::fs::write(
                d.join(format!("{n}.query.smt2")),
                format!("{EUF}{}{q}(check-sat)\n", self.transcript),
            );
        }
        let model = model?;
        // the candidate with zeroed input arrays first, then as the model has it
        for zero in [true, false] {
            let Some(pins) = crate::inst::pins(&model, &relaxed, zero) else { continue };
            if let Some(sat @ Res::Sat(_)) = self.euf(q, &pins, cex, ms) {
                return Some(sat);
            }
        }
        None
    }

    /// The query `q` (its assertions) with `extra` assertions on top, in a fresh z3 with the
    /// SAT-based core; `None` unless it answers sat (with `cex`'s values) or unsat.
    fn euf(&mut self, q: &str, extra: &str, cex: &[(String, String)], ms: u64) -> Option<Res> {
        let mut script = format!("{EUF}{}{q}{extra}(check-sat)\n", self.transcript);
        for (_, term) in cex {
            script.push_str(&format!("(get-value ({term}))\n"));
        }
        let out = self.solve(&script, ms)?;
        let mut responses = sexp::parse(&out).filter_map(Result::ok);
        match responses.next()?.as_atom()? {
            "unsat" => Some(Res::Unsat),
            "sat" => {
                let mut vals = vec![];
                for ((name, _), response) in cex.iter().zip(responses) {
                    let Some([pair]) = response.as_list() else { return None };
                    let Some([_, value]) = pair.as_list() else { return None };
                    vals.push((name.clone(), pretty(&value.to_string())));
                }
                Some(Res::Sat(vals))
            }
            _ => None,
        }
    }

    /// What a fresh z3 prints for `script`, or `None` when it reports an error.
    fn solve(&mut self, script: &str, ms: u64) -> Option<String> {
        self.stats.queries += 1;
        let z3 = std::env::var("SOLAR_VERIFY_Z3").unwrap_or_else(|_| "z3".into());
        let t = ms;
        let args = ["-in".into(), "-smt2".into(), format!("-t:{t}")];
        let never = std::sync::atomic::AtomicBool::new(false);
        let out = run_solver_text(&z3, &args, script, t + 2000, &never)?;
        if out.contains("(error") {
            return None;
        }
        Some(out)
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
        let res = match ans.as_str() {
            "unsat" => Res::Unsat,
            "sat" => {
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
                Res::Unknown(format!("{other} {why}"))
            }
        };
        self.raw("(pop 1)\n");
        self.stats.solver_ms += t0.elapsed().as_millis();
        res
    }
}

/// How long z3's SAT-based core gets on a quantified query before the default core does.
const QUICK_MS: u64 = 1000;

/// The option that switches z3 to its SAT-based core.
const EUF: &str = "(set-option :sat.euf true)\n";

fn pretty(v: &str) -> String {
    let v = v.trim();
    if let Some(n) = v.strip_prefix("(- ").and_then(|x| x.strip_suffix(')')) {
        return format!("-{n}");
    }
    if v.len() > 120 { format!("{}…", &v[..120]) } else { v.to_string() }
}

/// Run one solver on a script; Some(true) = unsat, Some(false) = sat, None = unknown/timeout/error.
fn run_solver(
    bin: &str,
    args: &[String],
    script: &str,
    timeout_ms: u64,
    cancel: &std::sync::atomic::AtomicBool,
) -> Option<bool> {
    let text = run_solver_text(bin, args, script, timeout_ms, cancel)?;
    let last = text
        .lines()
        .rev()
        .find(|l| matches!(l.trim(), "sat" | "unsat" | "unknown"))?
        .trim()
        .to_string();
    match last.as_str() {
        "unsat" => Some(true),
        "sat" => Some(false),
        _ => None,
    }
}

/// Run one solver on a script and return everything it printed.
fn run_solver_text(
    bin: &str,
    args: &[String],
    script: &str,
    timeout_ms: u64,
    cancel: &std::sync::atomic::AtomicBool,
) -> Option<String> {
    let mut child = Command::new(bin)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut inp = child.stdin.take()?;
    let script = script.to_string();
    let w = std::thread::spawn(move || {
        let _ = inp.write_all(script.as_bytes());
    });
    let mut out = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut out, &mut s);
        s
    });
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if cancel.load(std::sync::atomic::Ordering::SeqCst) || Instant::now() > deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break;
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            Err(_) => break,
        }
    }
    let _ = w.join();
    let text = reader.join().unwrap_or_default();
    if std::env::var_os("SOLAR_VERIFY_DEBUG").is_some()
        && let Some(e) = text.lines().find(|l| l.contains("error"))
    {
        eprintln!("{bin}: {e}");
    }
    Some(text)
}
