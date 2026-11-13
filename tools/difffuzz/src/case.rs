//! Running one case on every leg, and the campaign over generated cases.

use crate::{
    Cli,
    compile::Artifact,
    evm::{self, Chain},
    grammar::{self, Call, Contract, Lit, Ty},
    interp,
    obs::{self, Obs, Status},
};
use alloy_dyn_abi::{DynSolValue, JsonAbiExt};
use alloy_json_abi::Function;
use alloy_primitives::{I256, U256, hex};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// Runs one call on one interpreter.
type Interpreter = fn(&str, &[u8], &interp::Ctx<'_>) -> Obs;

/// A case: a source (or the generated contract it prints from) and a call sequence.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Case {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) contract: Option<Contract>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) source: Option<String>,
    pub(crate) name: String,
    pub(crate) calls: Vec<Call>,
    /// The divergence the case was saved for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) divergence: Option<Divergence>,
}

impl Case {
    pub(crate) fn source(&self) -> String {
        match (&self.contract, &self.source) {
            (Some(c), _) => c.sol(),
            (None, Some(s)) => s.clone(),
            _ => String::new(),
        }
    }
}

/// A disagreement between two legs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Divergence {
    /// The legs, reference first, such as `solar-evm/solc-evm`.
    pub(crate) legs: String,
    /// What differs: `reject`, `outcome`, `logs`, `storage`, or `abi`.
    pub(crate) kind: String,
    /// The call, or none for compilation and deployment.
    pub(crate) call: Option<usize>,
    pub(crate) detail: String,
    /// Why the divergence is expected, when it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) accepted: Option<String>,
}

impl Divergence {
    pub(crate) fn same(&self, other: &Self) -> bool {
        self.legs == other.legs && self.kind == other.kind
    }
}

/// The legs a run ran, and how often each could not run a call.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Stats {
    pub(crate) cases: u64,
    pub(crate) solc_rejects: u64,
    pub(crate) calls: u64,
    pub(crate) ran: BTreeMap<String, u64>,
    pub(crate) unsupported: BTreeMap<String, u64>,
    pub(crate) unsupported_reasons: BTreeMap<String, u64>,
    pub(crate) divergences: u64,
    /// How solc's calls ended: success, revert, halt.
    pub(crate) outcomes: BTreeMap<String, u64>,
}

impl Stats {
    fn leg(&mut self, leg: &str, o: &Obs) {
        if let Status::Unsupported(r) = &o.status {
            *self.unsupported.entry(leg.into()).or_default() += 1;
            let r: String = r.chars().take(80).collect();
            *self.unsupported_reasons.entry(format!("{leg}: {r}")).or_default() += 1;
        } else {
            *self.ran.entry(leg.into()).or_default() += 1;
        }
    }

    pub(crate) fn merge(&mut self, o: &Self) {
        self.cases += o.cases;
        self.solc_rejects += o.solc_rejects;
        self.calls += o.calls;
        self.divergences += o.divergences;
        for (k, v) in &o.outcomes {
            *self.outcomes.entry(k.clone()).or_default() += v;
        }
        for (k, v) in &o.ran {
            *self.ran.entry(k.clone()).or_default() += v;
        }
        for (k, v) in &o.unsupported {
            *self.unsupported.entry(k.clone()).or_default() += v;
        }
        for (k, v) in &o.unsupported_reasons {
            *self.unsupported_reasons.entry(k.clone()).or_default() += v;
        }
    }
}

/// What running a case found.
#[derive(Debug, Default)]
pub(crate) struct Outcome {
    /// solc rejected the source: the case says nothing.
    pub(crate) solc_reject: Option<String>,
    pub(crate) divergences: Vec<Divergence>,
    pub(crate) stats: Stats,
    /// Per call: the solc EVM observation, for the spec checks.
    pub(crate) solc_obs: Vec<Obs>,
    /// Per call: the Solar EVM observation.
    pub(crate) solar_obs: Vec<Obs>,
    pub(crate) solc_artifact: Option<Artifact>,
}

pub(crate) fn value(l: &Lit) -> DynSolValue {
    match l.ty {
        Ty::Bool => DynSolValue::Bool(l.value != "0"),
        Ty::Addr => {
            let v: U256 = l.value.parse().unwrap();
            DynSolValue::Address(alloy_primitives::Address::from_word(v.into()))
        }
        Ty::U(n) => DynSolValue::Uint(l.value.parse().unwrap(), n as usize),
        Ty::I(n) => DynSolValue::Int(I256::from_dec_str(&l.value).unwrap(), n as usize),
    }
}

/// Finds the ABI function a call names, by name and argument types.
pub(crate) fn function<'a>(abi: &'a alloy_json_abi::JsonAbi, call: &Call) -> Option<&'a Function> {
    let tys: Vec<String> = call.args.iter().map(|a| a.ty.sol()).collect();
    abi.functions().find(|f| {
        f.name == call.func && f.inputs.iter().map(|p| p.ty.clone()).collect::<Vec<_>>() == tys
    })
}

pub(crate) fn calldata(abi: &alloy_json_abi::JsonAbi, call: &Call) -> Option<Vec<u8>> {
    let f = function(abi, call)?;
    let vals: Vec<_> = call.args.iter().map(value).collect();
    f.abi_encode_input(&vals).ok()
}

fn div(out: &mut Outcome, legs: &str, kind: &str, call: Option<usize>, detail: String) {
    out.divergences.push(Divergence {
        legs: legs.into(),
        kind: kind.into(),
        call,
        detail,
        accepted: None,
    });
}

fn kind_of(d: &str) -> String {
    let k = d.split(':').next().unwrap_or("outcome");
    if k.starts_with("slot") { "storage".into() } else { k.into() }
}

/// Runs `case` on every leg in `dir`.
pub(crate) fn run(cli: &Cli, case: &Case, dir: &Path) -> Outcome {
    let mut out = Outcome::default();
    out.stats.cases = 1;
    std::fs::create_dir_all(dir).unwrap();
    let file = dir.join(format!("{}.sol", case.name));
    std::fs::write(&file, case.source()).unwrap();
    let solc = match cli.tools.solc(&file, &case.name) {
        Ok(a) => a,
        Err(e) => {
            out.solc_reject = Some(e);
            out.stats.solc_rejects = 1;
            return out;
        }
    };
    let solar = match cli.tools.solar(&file, &case.name) {
        Ok(a) => a,
        Err(e) => {
            div(&mut out, "solar/solc", "reject", None, e);
            out.stats.divergences = 1;
            return out;
        }
    };
    // Both compilers must agree on the interface.
    let sigs = |a: &Artifact| {
        let mut v: Vec<_> = a.abi.functions().map(|f| f.signature()).collect();
        v.sort();
        v
    };
    if sigs(&solar) != sigs(&solc) {
        div(
            &mut out,
            "solar/solc",
            "abi",
            None,
            format!("{:?} vs {:?}", sigs(&solar), sigs(&solc)),
        );
    }
    let mut a = Chain::new();
    let mut b = Chain::new();
    let da = a.deploy(&solar.bin);
    let db = b.deploy(&solc.bin);
    for d in obs::compare_full(&da, &db) {
        div(&mut out, "solar-evm/solc-evm", &kind_of(&d), None, format!("constructor: {d}"));
    }
    // Immutables and constructors that return other code deploy something else than the
    // runtime the dumps describe: the interpreters then have nothing to compare with.
    let same_runtime = da.status == Status::Success && a.code() == solar.runtime;
    let mut lean_calls = vec![];
    for (i, call) in case.calls.iter().enumerate() {
        let Some(data) = calldata(&solc.abi, call) else {
            continue;
        };
        lean_calls.push(data.clone());
        out.stats.calls += 1;
        let before = a.storage();
        let (oa, heap) = a.call(&data);
        let (ob, _) = b.call(&data);
        out.stats.leg("solar-evm", &oa);
        out.stats.leg("solc-evm", &ob);
        let k = match &ob.status {
            Status::Success => "success".to_string(),
            Status::Revert if ob.output.is_empty() => "revert-empty".into(),
            Status::Revert => {
                format!("revert-{}", hex::encode(&ob.output[..ob.output.len().min(4)]))
            }
            Status::Halt(_) => "halt".into(),
            Status::Unsupported(_) => "unsupported".into(),
        };
        *out.stats.outcomes.entry(k).or_default() += 1;
        for d in obs::compare_full(&oa, &ob) {
            div(&mut out, "solar-evm/solc-evm", &kind_of(&d), Some(i), d);
        }
        let ctx = interp::Ctx {
            storage: &before,
            address: a.address,
            caller: evm::CALLER,
            heap_start: heap,
            code_len: solar.runtime.len(),
        };
        let legs: [(&str, Option<&String>, Interpreter); 3] = [
            ("mir", solar.mir.as_ref(), interp::mir),
            ("evmir-scheduled", solar.evmir_scheduled.as_ref(), interp::evmir),
            ("evmir-final", solar.evmir_final.as_ref(), interp::evmir),
        ];
        let mut gas_dependent = matches!(&oa.status, Status::Halt(r) if r.contains("OutOfGas"))
            || matches!(&ob.status, Status::Halt(r) if r.contains("OutOfGas"));
        for (leg, module, f) in legs {
            if !same_runtime {
                out.stats
                    .leg(leg, &Obs::unsupported("the deployed code is not the dumped runtime"));
                continue;
            }
            let o = match module {
                Some(m) => f(m, &data, &ctx),
                None => Obs::unsupported("no module in the dump"),
            };
            out.stats.leg(leg, &o);
            if let Status::Unsupported(r) = &o.status {
                gas_dependent |= r.contains("reaches `gas`");
                continue;
            }
            // A dump that does not parse back is a finding of its own, not an outcome.
            if let Status::Halt(r) = &o.status
                && (r.starts_with("invalid MIR") || r.starts_with("invalid EVM IR"))
            {
                div(&mut out, &format!("solar/{leg}"), "invalid-dump", Some(i), r.clone());
                continue;
            }
            // The interpreters model neither gas nor the 1024-word stack: an EVM run that
            // exhausts either is not comparable.
            if matches!(&oa.status, Status::Halt(r) if r.contains("OutOfGas") || r.contains("StackOverflow"))
            {
                continue;
            }
            for d in obs::compare_interp(&oa, &o, &before) {
                div(&mut out, &format!("solar-evm/{leg}"), &kind_of(&d), Some(i), d);
            }
        }
        // Gas differs between compilers by design: a call that reads gas or runs out of it
        // may differ in anything.
        if gas_dependent {
            for d in &mut out.divergences {
                if d.call == Some(i) && d.legs == "solar-evm/solc-evm" {
                    d.accepted = Some("gas: the call reads gas or runs out of it".into());
                }
            }
        }
        out.solc_obs.push(ob);
        out.solar_obs.push(oa);
    }
    if let Some(forge) = &cli.forge
        && da.status == Status::Success
        && db.status == Status::Success
    {
        forge_equivalence(&mut out, forge, &mut a, &mut b, dir, cli.tools.timeout);
    }
    if let Some(lean) = &cli.lean {
        match crate::lean::run(cli, lean, &file, &case.name, &lean_calls, dir) {
            Ok(obs) => {
                for (i, o) in obs.iter().enumerate() {
                    out.stats.leg("solidcore", o);
                    if o.is_unsupported() || i >= out.solc_obs.len() {
                        continue;
                    }
                    // SolidCore is the source semantics of the chain that ends in Solar's
                    // bytecode: compare with that first, and with solc's for reference.
                    for d in obs::compare_full(&out.solar_obs[i], o) {
                        div(&mut out, "solar-evm/solidcore", &kind_of(&d), Some(i), d);
                    }
                    for d in obs::compare_full(&out.solc_obs[i], o) {
                        div(&mut out, "solc-evm/solidcore", &kind_of(&d), Some(i), d);
                    }
                }
            }
            Err(e) => {
                let o = Obs::unsupported(e);
                out.stats.leg("solidcore", &o);
            }
        }
    }
    if let Some(why) = known_difference(&case.source()) {
        for d in &mut out.divergences {
            if d.legs == "solar-evm/solc-evm" && d.accepted.is_none() {
                d.accepted = Some(why.into());
            }
        }
    }
    out.stats.divergences = out.divergences.len() as u64;
    out.solc_artifact = Some(solc);
    out
}

/// Compares the deployed codes with `forge equivalence`, Solar's as code A. Forge starts from
/// arbitrary storage, so a distinguishing input is replayed on both chains, from the state the
/// calls left: one that agrees there differs only from storage the calls never reached, such as
/// dirty padding (docs/SOLC_DIVERGENCE.md, CODEGEN-025), and is counted and kept apart.
fn forge_equivalence(
    out: &mut Outcome,
    forge: &str,
    a: &mut Chain,
    b: &mut Chain,
    dir: &Path,
    timeout: std::time::Duration,
) {
    let eq =
        crate::forge::equivalence(forge, &a.code(), &b.code(), dir, timeout).unwrap_or_else(|e| {
            crate::forge::Equivalence { status: "incomplete".into(), calldata: vec![], report: e }
        });
    *out.stats.outcomes.entry(format!("forge-{}", eq.status)).or_default() += 1;
    if eq.status != "not_equivalent" {
        return;
    }
    let (oa, _) = a.call(&eq.calldata);
    let (ob, _) = b.call(&eq.calldata);
    let reached = !obs::compare_full(&oa, &ob).is_empty();
    let detail = format!("calldata 0x{}: {}", hex::encode(&eq.calldata), eq.report);
    div(out, "solar-evm/solc-evm", "forge-equivalence", None, detail);
    if !reached {
        out.divergences.last_mut().unwrap().accepted =
            Some("forge: differs only from storage the calls never reached".into());
    }
}

/// Code whose Solar and solc bytecode may differ by design (docs/SOLC_DIVERGENCE.md).
///
/// Only the comparison with solc is waived: Solar's own legs still have to agree.
pub(crate) fn known_difference(src: &str) -> Option<&'static str> {
    if src.contains(".runtimeCode") || src.contains(".creationCode") {
        return Some("bytecode: the code reads compiled bytecode");
    }
    if src.contains("assembly") {
        return Some("assembly: raw bits and memory layout differ by design");
    }
    // A function type is written `function(`; a declaration names the function first.
    let internal_fn = src.match_indices("function(").any(|(at, _)| {
        let rest = &src[at..];
        let end = rest.find([';', '{']).unwrap_or(rest.len());
        rest[..end].contains(") internal")
    });
    if internal_fn {
        return Some("internal function pointers are code offsets");
    }
    None
}

/// Saves a case that diverged, reduced when asked; returns where it went.
pub(crate) fn save(cli: &Cli, case: &Case, d: &Divergence, tag: &str) -> PathBuf {
    let mut case = case.clone();
    case.divergence = Some(d.clone());
    let dir = cli.out.join("divergences");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{tag}.json"));
    std::fs::write(&path, serde_json::to_string_pretty(&case).unwrap()).unwrap();
    if cli.reduce {
        let reduced = crate::reduce::reduce(cli, &case, d);
        let rpath = dir.join(format!("{tag}.reduced.json"));
        std::fs::write(&rpath, serde_json::to_string_pretty(&reduced).unwrap()).unwrap();
        std::fs::write(dir.join(format!("{tag}.reduced.sol")), reduced.source()).unwrap();
        return rpath;
    }
    path
}

/// The divergences of a run, one per kind and pair of legs.
fn distinct(ds: &[Divergence]) -> Vec<&Divergence> {
    let mut seen: Vec<&Divergence> = vec![];
    for d in ds {
        if !seen.iter().any(|s| s.same(d)) {
            seen.push(d);
        }
    }
    seen
}

/// Runs generated cases `seed..seed+iters`.
pub(crate) fn campaign(cli: &Cli) {
    let cfg = grammar::Config { calls: cli.calls, ..grammar::Config::default() };
    let mut stats = Stats::default();
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(cli.out.join("log.jsonl"))
        .unwrap();
    let mut rejects = BTreeMap::<String, u64>::new();
    let mut accepted = BTreeMap::<String, u64>::new();
    let work = cli.out.join("work");
    for seed in cli.seed..cli.seed + cli.iters {
        let (contract, calls) = grammar::generate(seed, &cfg);
        let case = Case {
            contract: Some(contract),
            source: None,
            name: "C".into(),
            calls,
            divergence: None,
        };
        let out = run(cli, &case, &work);
        stats.merge(&out.stats);
        if let Some(r) = &out.solc_reject {
            let first = r.lines().find(|l| l.contains("Error")).unwrap_or(r).to_string();
            *rejects.entry(first.chars().take(120).collect()).or_default() += 1;
            let rdir = cli.out.join("solc-rejects");
            std::fs::create_dir_all(&rdir).unwrap();
            std::fs::write(rdir.join(format!("{seed}.sol")), case.source()).unwrap();
        }
        for d in distinct(&out.divergences) {
            if let Some(why) = &d.accepted {
                *accepted.entry(format!("{} {}: {why}", d.legs, d.kind)).or_default() += 1;
                // Unreached storage can still show a real difference (CODEGEN-030): keep the
                // case, unreduced, for a look.
                if d.kind == "forge-equivalence" {
                    let fdir = cli.out.join("forge-unreached");
                    std::fs::create_dir_all(&fdir).unwrap();
                    let mut kept = case.clone();
                    kept.divergence = Some(d.clone());
                    let text = serde_json::to_string_pretty(&kept).unwrap();
                    std::fs::write(fdir.join(format!("seed{seed}.json")), text).unwrap();
                }
                continue;
            }
            let path = save(cli, &case, d, &format!("seed{seed}-{}", d.legs.replace('/', "_")));
            eprintln!(
                "seed {seed}: {} {} {}: {}",
                d.legs,
                d.kind,
                path.display(),
                short(&d.detail)
            );
        }
        use std::io::Write;
        writeln!(
            log,
            "{}",
            serde_json::json!({"seed": seed, "calls": out.stats.calls, "solc_reject": out.solc_reject.is_some(),
                "divergences": out.divergences.iter().map(|d| format!("{} {}", d.legs, d.kind)).collect::<Vec<_>>()})
        )
        .unwrap();
        if (seed - cli.seed + 1).is_multiple_of(25) {
            eprintln!(
                "progress: {} cases, {} calls, {} divergences",
                stats.cases, stats.calls, stats.divergences
            );
        }
    }
    let summary =
        serde_json::json!({"stats": stats, "solc_rejects": rejects, "accepted": accepted});
    std::fs::write(cli.out.join("summary.json"), serde_json::to_string_pretty(&summary).unwrap())
        .unwrap();
    println!("{}", serde_json::to_string_pretty(&summary).unwrap());
}

pub(crate) fn short(s: &str) -> String {
    if s.len() > 300 { format!("{}...", &s[..300]) } else { s.to_string() }
}

/// Runs a saved case and prints what it finds.
pub(crate) fn replay(cli: &Cli, path: &Path) {
    let case: Case = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let out = run(cli, &case, &cli.out.join("replay"));
    if let Some(r) = out.solc_reject {
        println!("solc rejects the case: {r}");
        std::process::exit(2);
    }
    for d in &out.divergences {
        println!("{} {} call {:?}: {}", d.legs, d.kind, d.call, d.detail);
    }
    println!("{}", serde_json::to_string(&out.stats).unwrap());
    if !out.divergences.is_empty() {
        std::process::exit(1);
    }
    let _ = hex::encode([]);
}

#[cfg(test)]
mod tests {
    use super::known_difference;

    #[test]
    fn known_differences() {
        assert!(known_difference("contract C { function f() internal {} }").is_none());
        assert!(known_difference("struct E { function() internal returns (uint) cb; }").is_some());
        assert!(known_difference("bytes32 h = keccak256(type(M).runtimeCode);").is_some());
        assert!(known_difference("function f() { assembly { x := 1 } }").is_some());
    }
}
