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
}

fn spawn(timeout_ms: u64) -> std::io::Result<Z3> {
    let mut child = Command::new(std::env::var("SOLVERUS_Z3").unwrap_or_else(|_| "z3".into()))
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
    Ok(Z3 { child, inp, out: rx })
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

    fn line(&mut self, deadline: Instant) -> Option<String> {
        let left = deadline.saturating_duration_since(Instant::now());
        let z3 = self.z3.as_ref()?;
        match z3.out.recv_timeout(left) {
            Ok(l) => Some(l),
            Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => None,
        }
    }

    fn read_answer(&mut self, deadline: Instant) -> Option<String> {
        loop {
            let l = self.line(deadline)?;
            let l = l.trim();
            if l == "sat" || l == "unsat" || l == "unknown" {
                return Some(l.to_string());
            }
            if l.starts_with("(error") {
                eprintln!("internal: z3 {l}");
                // a rejected command means the query is not the one we meant: no answer counts
                if self.error.is_none() {
                    self.error = Some(l.to_string());
                }
            }
        }
    }

    fn read_sexp(&mut self, deadline: Instant) -> Option<String> {
        let mut acc = String::new();
        let mut depth = 0i32;
        loop {
            let line = self.line(deadline)?;
            for c in line.chars() {
                if c == '(' {
                    depth += 1
                } else if c == ')' {
                    depth -= 1
                }
            }
            acc.push_str(line.trim());
            acc.push(' ');
            if depth <= 0 && acc.contains('(') {
                return Some(acc);
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
        if std::env::var("SOLVERUS_RACE").as_deref() == Ok("off") {
            return None;
        }
        let q = format!("(assert {guard})\n(assert (not {goal}))\n(check-sat)\n");
        let int_script =
            format!("(set-logic ALL)\n{}{q}", self.transcript.replace("bv2int", "bv2nat"));
        let mut jobs: Vec<(String, String, Vec<String>, String)> = vec![];
        let t = self.timeout_ms;
        let cvc5 = std::env::var("SOLVERUS_CVC5").unwrap_or_else(|_| "cvc5".into());
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
            if std::env::var_os("SOLVERUS_DEBUG").is_some() {
                eprintln!("bvlower: width {w}, {dropped} assertions dropped, {} bytes", bv.len());
            }
            if let Some(d) = std::env::var_os("SOLVERUS_BV_DUMP") {
                let _ = std::fs::write(d, bv);
            }
            let quantified = bv.contains("(forall") || bv.contains("(exists");
            let bw = std::env::var("SOLVERUS_BITWUZLA").unwrap_or_else(|_| "bitwuzla".into());
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
                std::env::var("SOLVERUS_Z3").unwrap_or_else(|_| "z3".into()),
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
            if std::env::var_os("SOLVERUS_DEBUG").is_some() {
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
                    let Some(s) = self.read_sexp(deadline) else {
                        break;
                    };
                    let inner = s.trim().trim_start_matches('(').trim_start_matches('(');
                    let inner = inner.trim_end().trim_end_matches(')').trim_end_matches(')');
                    let v = inner.strip_prefix(term.as_str()).unwrap_or(inner).trim().to_string();
                    vals.push((name.clone(), pretty(&v)));
                }
                Res::Sat(vals)
            }
            other => {
                self.raw("(get-info :reason-unknown)\n");
                self.flush();
                let why =
                    self.read_sexp(Instant::now() + Duration::from_secs(2)).unwrap_or_default();
                Res::Unknown(format!("{other} {}", why.trim()))
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

/// Run one solver on a script; Some(true) = unsat, Some(false) = sat, None = unknown/timeout/error.
fn run_solver(
    bin: &str,
    args: &[String],
    script: &str,
    timeout_ms: u64,
    cancel: &std::sync::atomic::AtomicBool,
) -> Option<bool> {
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
    if std::env::var_os("SOLVERUS_DEBUG").is_some()
        && let Some(e) = text.lines().find(|l| l.contains("error"))
    {
        eprintln!("{bin}: {e}");
    }
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
