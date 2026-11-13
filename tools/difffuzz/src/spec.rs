//! Spec soundness: mine likely specs from runs, prove what solar-verify proves, then check the
//! proved specs at runtime on fresh call sequences against Solar's deployed bytecode (and
//! solc's). A proved spec that a run violates is a soundness bug of the verifier, or of the
//! compiler that built the bytecode.
//!
//! Candidates follow Daikon: templates over the scalar integer state variables, the integer
//! parameters and the single integer result that held on every successful call of a mining run.
//! Houdini then drops every candidate solar-verify cannot prove until the rest verify together;
//! a case counts only if every function, the constructor included, verifies. Specs are
//! evaluated here, directly, over math integers, as the verifier reads them.

use crate::{
    Cli,
    case::{self, Case, Divergence},
    compile::{self, Artifact},
    evm::Chain,
    grammar::{self, Call, Contract, Rng, Stmt, Ty},
    obs::Status,
};
use alloy_dyn_abi::{DynSolValue, FunctionExt, JsonAbiExt};
use num_bigint::BigInt;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path, process::Command, time::Duration};

/// A spec term.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Term {
    State(String),
    Old(String),
    Param(String),
    Result,
    Const(String),
    Add(Box<Self>, Box<Self>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Spec {
    /// `ensures` of a function, or `invariant` of the contract.
    pub(crate) func: Option<String>,
    pub(crate) lhs: Term,
    pub(crate) op: String,
    pub(crate) rhs: Term,
}

impl Term {
    fn sol(&self) -> String {
        match self {
            Self::State(s) | Self::Param(s) => s.clone(),
            Self::Old(s) => format!("old({s})"),
            Self::Result => "result".into(),
            Self::Const(c) => c.clone(),
            Self::Add(a, b) => format!("{} + {}", a.sol(), b.sol()),
        }
    }

    fn eval(&self, env: &Env<'_>) -> Option<BigInt> {
        Some(match self {
            Self::State(s) => env.post.get(s)?.clone(),
            Self::Old(s) => env.pre.get(s)?.clone(),
            Self::Param(p) => env.params.get(p)?.clone(),
            Self::Result => env.result.clone()?,
            Self::Const(c) => c.parse().ok()?,
            Self::Add(a, b) => a.eval(env)? + b.eval(env)?,
        })
    }
}

impl Spec {
    pub(crate) fn sol(&self) -> String {
        format!("{} {} {}", self.lhs.sol(), self.op, self.rhs.sol())
    }

    /// Whether the spec holds in `env`; none when it reads what the run does not show.
    fn holds(&self, env: &Env<'_>) -> Option<bool> {
        let (a, b) = (self.lhs.eval(env)?, self.rhs.eval(env)?);
        Some(match self.op.as_str() {
            "==" => a == b,
            "<=" => a <= b,
            ">=" => a >= b,
            "<" => a < b,
            _ => return None,
        })
    }
}

struct Env<'a> {
    pre: &'a BTreeMap<String, BigInt>,
    post: &'a BTreeMap<String, BigInt>,
    params: BTreeMap<String, BigInt>,
    result: Option<BigInt>,
}

fn int(v: &DynSolValue) -> Option<BigInt> {
    match v {
        DynSolValue::Uint(u, _) => u.to_string().parse().ok(),
        DynSolValue::Int(i, _) => i.to_string().parse().ok(),
        _ => None,
    }
}

/// The integer scalar state variables a spec may name, all made public.
fn scalars(c: &Contract) -> Vec<(String, Ty)> {
    c.state
        .iter()
        .filter_map(|v| match v.kind {
            grammar::StateKind::Scalar(t) if t.is_int() => Some((v.name.clone(), t)),
            _ => None,
        })
        .collect()
}

/// Reads the scalars through their getters.
fn read_state(
    chain: &mut Chain,
    art: &Artifact,
    names: &[(String, Ty)],
) -> BTreeMap<String, BigInt> {
    let mut out = BTreeMap::new();
    for (n, _) in names {
        let Some(f) = art.abi.functions().find(|f| &f.name == n && f.inputs.is_empty()) else {
            continue;
        };
        let data = f.abi_encode_input(&[]).unwrap();
        let (o, _) = chain.call(&data);
        if o.status == Status::Success
            && let Ok(v) = f.abi_decode_output(&o.output)
            && let Some(x) = v.first().and_then(int)
        {
            out.insert(n.clone(), x);
        }
    }
    out
}

/// One observed call: the function, its environment, and whether it returned.
struct Run {
    func: String,
    pre: BTreeMap<String, BigInt>,
    post: BTreeMap<String, BigInt>,
    params: BTreeMap<String, BigInt>,
    result: Option<BigInt>,
    ok: bool,
}

fn execute(
    c: &Contract,
    art: &Artifact,
    calls: &[Call],
) -> Option<(BTreeMap<String, BigInt>, Vec<Run>)> {
    let names = scalars(c);
    let mut chain = Chain::new();
    if chain.deploy(&art.bin).status != Status::Success {
        return None;
    }
    let init = read_state(&mut chain, art, &names);
    let mut runs = vec![];
    for call in calls {
        let f = case::function(&art.abi, call)?;
        let data = case::calldata(&art.abi, call)?;
        let pre = read_state(&mut chain, art, &names);
        let (o, _) = chain.call(&data);
        let post = read_state(&mut chain, art, &names);
        let func = c.funcs.iter().find(|x| x.name == call.func)?;
        let params = func
            .params
            .iter()
            .zip(&call.args)
            .filter(|((t, _), _)| t.is_int())
            .map(|((_, n), a)| (n.clone(), a.value.parse().unwrap()))
            .collect();
        let result =
            if o.status == Status::Success && func.returns.len() == 1 && func.returns[0].is_int() {
                f.abi_decode_output(&o.output).ok().and_then(|v| v.first().and_then(int))
            } else {
                None
            };
        runs.push(Run {
            func: call.func.clone(),
            pre,
            post,
            params,
            result,
            ok: o.status == Status::Success,
        });
    }
    Some((init, runs))
}

/// Candidate specs over the contract.
fn candidates(c: &Contract) -> Vec<Spec> {
    let names = scalars(c);
    let mut out = vec![];
    let consts = ["0", "1", "10", "255"];
    for (i, (a, _)) in names.iter().enumerate() {
        for k in consts {
            for op in ["<=", ">="] {
                out.push(Spec {
                    func: None,
                    lhs: Term::State(a.clone()),
                    op: op.into(),
                    rhs: Term::Const(k.into()),
                });
            }
        }
        for (b, _) in names.iter().skip(i + 1) {
            for op in ["<=", ">=", "=="] {
                out.push(Spec {
                    func: None,
                    lhs: Term::State(a.clone()),
                    op: op.into(),
                    rhs: Term::State(b.clone()),
                });
            }
        }
    }
    for f in &c.funcs {
        let fname = Some(f.name.clone());
        for (s, _) in &names {
            for op in ["==", "<=", ">="] {
                out.push(Spec {
                    func: fname.clone(),
                    lhs: Term::State(s.clone()),
                    op: op.into(),
                    rhs: Term::Old(s.clone()),
                });
            }
            for (t, p) in &f.params {
                if t.is_int() {
                    out.push(Spec {
                        func: fname.clone(),
                        lhs: Term::State(s.clone()),
                        op: "==".into(),
                        rhs: Term::Param(p.clone()),
                    });
                    out.push(Spec {
                        func: fname.clone(),
                        lhs: Term::State(s.clone()),
                        op: "==".into(),
                        rhs: Term::Add(
                            Box::new(Term::Old(s.clone())),
                            Box::new(Term::Param(p.clone())),
                        ),
                    });
                }
            }
        }
        if f.returns.len() == 1 && f.returns[0].is_int() {
            for (s, _) in &names {
                for op in ["==", "<=", ">="] {
                    out.push(Spec {
                        func: fname.clone(),
                        lhs: Term::Result,
                        op: op.into(),
                        rhs: Term::State(s.clone()),
                    });
                }
            }
            for (t, p) in &f.params {
                if t.is_int() {
                    for op in ["==", "<=", ">="] {
                        out.push(Spec {
                            func: fname.clone(),
                            lhs: Term::Result,
                            op: op.into(),
                            rhs: Term::Param(p.clone()),
                        });
                    }
                }
            }
            for k in ["0", "1"] {
                for op in ["==", "<=", ">="] {
                    out.push(Spec {
                        func: fname.clone(),
                        lhs: Term::Result,
                        op: op.into(),
                        rhs: Term::Const(k.into()),
                    });
                }
            }
        }
    }
    out
}

/// The first violation of `specs` in a run, if any.
fn violation(
    specs: &[Spec],
    init: &BTreeMap<String, BigInt>,
    runs: &[Run],
) -> Option<(usize, Spec)> {
    let env0 = Env { pre: init, post: init, params: BTreeMap::new(), result: None };
    for s in specs.iter().filter(|s| s.func.is_none()) {
        if s.holds(&env0) == Some(false) {
            return Some((usize::MAX, s.clone()));
        }
    }
    for (i, r) in runs.iter().enumerate() {
        if !r.ok {
            continue;
        }
        let env =
            Env { pre: &r.pre, post: &r.post, params: r.params.clone(), result: r.result.clone() };
        for s in specs {
            if (s.func.is_none() || s.func.as_ref() == Some(&r.func))
                && s.holds(&env) == Some(false)
            {
                return Some((i, s.clone()));
            }
        }
    }
    None
}

/// The specs that held on every run that could evaluate them, and were evaluated at least once.
fn mine(specs: Vec<Spec>, init: &BTreeMap<String, BigInt>, runs: &[Run]) -> Vec<Spec> {
    specs
        .into_iter()
        .filter(|s| {
            let mut seen = 0;
            if s.func.is_none() {
                let env0 = Env { pre: init, post: init, params: BTreeMap::new(), result: None };
                match s.holds(&env0) {
                    Some(false) => return false,
                    Some(true) => seen += 1,
                    None => {}
                }
            }
            for r in runs.iter().filter(|r| r.ok) {
                if s.func.as_ref().is_some_and(|f| f != &r.func) {
                    continue;
                }
                let env = Env {
                    pre: &r.pre,
                    post: &r.post,
                    params: r.params.clone(),
                    result: r.result.clone(),
                };
                match s.holds(&env) {
                    Some(false) => return false,
                    Some(true) => seen += 1,
                    None => {}
                }
            }
            seen > 0
        })
        .collect()
}

/// Puts `specs` on the contract.
fn annotate(c: &Contract, specs: &[Spec]) -> Contract {
    let mut c = c.clone();
    c.invariants = specs.iter().filter(|s| s.func.is_none()).map(Spec::sol).collect();
    for f in &mut c.funcs {
        // Runtime checks stay: the verifier assumes them and `--instrument` enforces them.
        f.specs.retain(|s| s.starts_with("check "));
        f.specs.extend(
            specs
                .iter()
                .filter(|s| s.func.as_ref() == Some(&f.name))
                .map(|s| format!("ensures {}", s.sol())),
        );
    }
    c
}

/// Adds a `@custom:check` bound on an integer parameter to some functions.
fn add_checks(c: &mut Contract, rng: &mut Rng) -> usize {
    let mut n = 0;
    for f in &mut c.funcs {
        let ints: Vec<_> = f.params.iter().filter(|(t, _)| t.is_int()).cloned().collect();
        if ints.is_empty() || !rng.chance(50) {
            continue;
        }
        let (t, p) = rng.pick(&ints).clone();
        let bound = *rng.pick(&[1, 3, 10, 100, 1000]);
        let check = if matches!(t, Ty::I(_)) && rng.chance(50) {
            format!("check {p} >= -{bound}")
        } else {
            format!("check {p} <= {bound}")
        };
        f.specs.push(check);
        n += 1;
    }
    n
}

#[derive(Debug, Default)]
struct Verdict {
    /// Every function verified.
    all: bool,
    /// Specs a failure names, by text.
    failing: Vec<String>,
    /// Functions that failed without naming a spec, or are unsupported.
    stuck: Vec<String>,
    raw: String,
}

fn verify(cli: &Cli, file: &Path) -> Result<Verdict, String> {
    let mut cmd = Command::new(cli.verify.as_ref().ok_or("no --verify")?);
    cmd.args(["--json", "--timeout", "4000"]).arg(file);
    // Exit 1 means some function failed: read the JSON either way.
    let (_, out, err) = compile::run_status(cmd, Duration::from_secs(600))?;
    let out = String::from_utf8_lossy(&out).into_owned();
    let err = String::from_utf8_lossy(&err).into_owned();
    if err.contains("internal:") {
        return Err(format!(
            "internal error: {}",
            err.lines().find(|l| l.contains("internal:")).unwrap_or("")
        ));
    }
    let json = out
        .lines()
        .find_map(|l| l.strip_prefix("JSON "))
        .ok_or_else(|| format!("no JSON: {}", case::short(&out)))?;
    let rows: Vec<serde_json::Value> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let mut v =
        Verdict { all: !rows.is_empty(), raw: format!("{out}\n{err}"), ..Verdict::default() };
    for r in &rows {
        let status = r["status"].as_str().unwrap_or("");
        if status == "verified" {
            continue;
        }
        v.all = false;
        let mut named = false;
        for m in r["msgs"].as_array().into_iter().flatten() {
            let m = m.as_str().unwrap_or("");
            // `LINE:kind: spec (note)`
            if let Some((_, rest)) = m.split_once(':')
                && let Some((_, spec)) = rest.split_once(": ")
            {
                let spec = spec.split(" (").next().unwrap_or(spec).trim().to_string();
                v.failing.push(spec);
                named = true;
            }
        }
        if !named || status == "unsupported" {
            v.stuck.push(r["fn"].as_str().unwrap_or("").to_string());
        }
    }
    Ok(v)
}

/// Statistics of a spec campaign.
#[derive(Debug, Default, Serialize)]
struct SpecStats {
    cases: u64,
    mined: u64,
    verified_cases: u64,
    proved_specs: u64,
    proved_ensures: u64,
    proved_invariants: u64,
    checked_calls: u64,
    /// Cases whose bytecode is the `--instrument` output, with `@custom:check` claims.
    instrumented_cases: u64,
    spec_evaluations: u64,
    violations: u64,
    gave_up: BTreeMap<String, u64>,
}

/// Proves what it can of the mined specs; returns the specs that verified together.
fn houdini(
    cli: &Cli,
    base: &Contract,
    mut specs: Vec<Spec>,
    dir: &Path,
    stats: &mut SpecStats,
) -> Option<Vec<Spec>> {
    let file = dir.join("C.sol");
    for _ in 0..6 {
        let c = annotate(base, &specs);
        std::fs::write(&file, c.sol()).unwrap();
        let v = match verify(cli, &file) {
            Ok(v) => v,
            Err(e) => {
                if e.starts_with("internal error") {
                    let keep = cli.out.join("verify-internal");
                    std::fs::create_dir_all(&keep).unwrap();
                    let n = std::fs::read_dir(&keep).map(|d| d.count()).unwrap_or(0);
                    std::fs::write(keep.join(format!("{n}.sol")), c.sol()).unwrap();
                    std::fs::write(keep.join(format!("{n}.txt")), &e).unwrap();
                }
                *stats
                    .gave_up
                    .entry(format!("verify error: {}", e.chars().take(60).collect::<String>()))
                    .or_default() += 1;
                return None;
            }
        };
        if v.all {
            return Some(specs);
        }
        if !v.stuck.is_empty() {
            let why = v
                .raw
                .lines()
                .find(|l| l.contains("unsupported") || l.starts_with("error"))
                .unwrap_or("");
            *stats
                .gave_up
                .entry(format!("stuck: {}", why.chars().take(80).collect::<String>()))
                .or_default() += 1;
            return None;
        }
        let before = specs.len();
        specs.retain(|s| !v.failing.iter().any(|f| f == &s.sol()));
        if specs.len() == before {
            *stats.gave_up.entry("failing specs not matched".into()).or_default() += 1;
            return None;
        }
    }
    *stats.gave_up.entry("no fixpoint".into()).or_default() += 1;
    None
}

/// A generated contract fit for the verifier: scalars public, no Solidity `assert`, which is
/// always a proof obligation.
fn for_specs(c: &mut Contract) {
    fn strip(body: &mut Vec<Stmt>) {
        body.retain(|s| !matches!(s, Stmt::Assert(_)));
        for s in body.iter_mut() {
            for b in s.blocks_mut() {
                strip(b);
            }
        }
    }
    for v in &mut c.state {
        if matches!(v.kind, grammar::StateKind::Scalar(_)) {
            v.public = true;
        }
    }
    for f in c.funcs.iter_mut().chain(c.helpers.iter_mut()).chain(c.procs.iter_mut()) {
        strip(&mut f.body);
    }
}

pub(crate) fn campaign(cli: &Cli) {
    let cfg = grammar::Config { calls: cli.calls.max(20), ..grammar::Config::default() };
    let mut stats = SpecStats::default();
    let dir = cli.out.join("spec-work");
    std::fs::create_dir_all(&dir).unwrap();
    for seed in cli.seed..cli.seed + cli.iters {
        stats.cases += 1;
        let (mut c, calls) = grammar::generate(seed, &cfg);
        for_specs(&mut c);
        let mut crng = Rng::new(seed ^ 0xc4ec);
        let checks = add_checks(&mut c, &mut crng);
        if scalars(&c).is_empty() {
            *stats.gave_up.entry("no integer scalars".into()).or_default() += 1;
            continue;
        }
        let file = dir.join("C.sol");
        std::fs::write(&file, c.sol()).unwrap();
        let Ok(art) = cli.tools.solar(&file, "C") else {
            *stats.gave_up.entry("solar rejects".into()).or_default() += 1;
            continue;
        };
        let Some((init, runs)) = execute(&c, &art, &calls) else {
            *stats.gave_up.entry("mining run failed".into()).or_default() += 1;
            continue;
        };
        let mined = mine(candidates(&c), &init, &runs);
        if mined.is_empty() {
            *stats.gave_up.entry("nothing mined".into()).or_default() += 1;
            continue;
        }
        stats.mined += mined.len() as u64;
        let Some(proved) = houdini(cli, &c, mined, &dir, &mut stats) else { continue };
        if proved.is_empty() {
            *stats.gave_up.entry("nothing proved".into()).or_default() += 1;
            continue;
        }
        stats.verified_cases += 1;
        stats.proved_specs += proved.len() as u64;
        stats.proved_ensures += proved.iter().filter(|s| s.func.is_some()).count() as u64;
        stats.proved_invariants += proved.iter().filter(|s| s.func.is_none()).count() as u64;
        let annotated = annotate(&c, &proved);
        std::fs::write(&file, annotated.sol()).unwrap();
        // The bytecode the specs are checked against: Solar's for the annotated file, and solc's.
        // With checks, the file that deploys is the instrumented one, which enforces them.
        let mut subject = file.clone();
        if checks > 0 {
            let inst = dir.join("Instrumented.sol");
            let mut cmd = Command::new(cli.verify.as_ref().unwrap());
            cmd.arg("--instrument").arg(&inst).arg(&file);
            if let Err(e) = compile::run(cmd, Duration::from_secs(120)) {
                *stats
                    .gave_up
                    .entry(format!("instrument: {}", e.chars().take(60).collect::<String>()))
                    .or_default() += 1;
                continue;
            }
            stats.instrumented_cases += 1;
            subject = inst;
        }
        let file = subject;
        let mut arts = vec![];
        match cli.tools.solar(&file, "C") {
            Ok(a) => arts.push(("solar", a)),
            Err(_) => continue,
        }
        if let Ok(a) = cli.tools.solc(&file, "C") {
            arts.push(("solc", a));
        }
        let mut rng = Rng::new(seed ^ 0x5eed);
        'check: for round in 0..8 {
            let seq = grammar::calls(&c, &mut rng, 30);
            for (which, art) in &arts {
                let Some((init, runs)) = execute(&c, art, &seq) else { continue };
                stats.checked_calls += runs.len() as u64;
                stats.spec_evaluations +=
                    (runs.iter().filter(|r| r.ok).count() * proved.len()) as u64;
                if let Some((i, s)) = violation(&proved, &init, &runs) {
                    stats.violations += 1;
                    let d = Divergence {
                        legs: format!("verify/{which}-evm"),
                        kind: "spec".into(),
                        call: (i != usize::MAX).then_some(i),
                        detail: format!(
                            "proved spec {} ({:?}) violated at call {i} of round {round}",
                            s.sol(),
                            s.func
                        ),
                        accepted: None,
                    };
                    let case = Case {
                        contract: Some(annotated),
                        source: None,
                        name: "C".into(),
                        calls: seq,
                        divergence: Some(d.clone()),
                    };
                    let out = cli.out.join("violations");
                    std::fs::create_dir_all(&out).unwrap();
                    let p = out.join(format!("seed{seed}-{which}.json"));
                    std::fs::write(&p, serde_json::to_string_pretty(&case).unwrap()).unwrap();
                    eprintln!("seed {seed}: VIOLATION {} -> {}", d.detail, p.display());
                    break 'check;
                }
            }
        }
        if (seed - cli.seed + 1).is_multiple_of(10) {
            eprintln!("progress: {}", serde_json::to_string(&stats).unwrap());
        }
    }
    let s = serde_json::to_string_pretty(&stats).unwrap();
    std::fs::write(cli.out.join("spec-summary.json"), &s).unwrap();
    println!("{s}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pre: &'a BTreeMap<String, BigInt>, post: &'a BTreeMap<String, BigInt>) -> Env<'a> {
        Env {
            pre,
            post,
            params: [("a0".to_string(), BigInt::from(3))].into(),
            result: Some(BigInt::from(9)),
        }
    }

    #[test]
    fn evaluates_over_math_integers() {
        let pre = [("s0".to_string(), BigInt::from(6))].into();
        let post = [("s0".to_string(), BigInt::from(9))].into();
        let e = env(&pre, &post);
        let grows = Spec {
            func: Some("f0".into()),
            lhs: Term::State("s0".into()),
            op: "==".into(),
            rhs: Term::Add(Box::new(Term::Old("s0".into())), Box::new(Term::Param("a0".into()))),
        };
        assert_eq!(grows.sol(), "s0 == old(s0) + a0");
        assert_eq!(grows.holds(&e), Some(true));
        let frame = Spec {
            func: None,
            lhs: Term::State("s0".into()),
            op: "<=".into(),
            rhs: Term::Old("s0".into()),
        };
        assert_eq!(frame.holds(&e), Some(false));
        // A term the run does not show is undetermined, never true.
        let unknown =
            Spec { func: None, lhs: Term::State("s9".into()), op: "==".into(), rhs: Term::Result };
        assert_eq!(unknown.holds(&e), None);
    }
}
