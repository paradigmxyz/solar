//! Mutations of existing contracts: Solar's UI tests and solar-verify's examples, with their
//! operators and literals changed, called with random sequences built from their ABI.

use crate::{
    Cli,
    case::{self, Case, Stats},
    grammar::{self, Call, Rng, Ty},
};
use std::collections::BTreeMap;

fn ty(s: &str) -> Option<Ty> {
    Some(match s {
        "bool" => Ty::Bool,
        "address" => Ty::Addr,
        "uint" => Ty::U(256),
        "int" => Ty::I(256),
        _ if s.starts_with("uint") => Ty::U(s[4..].parse().ok()?),
        _ if s.starts_with("int") => Ty::I(s[3..].parse().ok()?),
        _ => return None,
    })
}

const OPS: &[(&str, &str)] = &[
    (" + ", " - "),
    (" - ", " + "),
    (" * ", " / "),
    (" / ", " * "),
    (" % ", " / "),
    (" < ", " <= "),
    (" <= ", " < "),
    (" > ", " >= "),
    (" >= ", " > "),
    (" == ", " != "),
    (" != ", " == "),
    (" && ", " || "),
    (" || ", " && "),
    (" << ", " >> "),
    (" >> ", " << "),
    (" & ", " | "),
    (" | ", " ^ "),
    ("+= ", "-= "),
    ("-= ", "+= "),
];

/// One mutation of `src`: an operator swapped or a literal changed, outside comments.
fn mutate(src: &str, rng: &mut Rng) -> Option<String> {
    let lines: Vec<&str> = src.lines().collect();
    let code: Vec<usize> = (0..lines.len())
        .filter(|&i| {
            let t = lines[i].trim_start();
            !t.starts_with("//")
                && !t.starts_with("/*")
                && !t.starts_with('*')
                && !t.starts_with("pragma")
        })
        .collect();
    for _ in 0..20 {
        let i = *rng.pick(&code);
        let line = lines[i];
        let new = if rng.chance(60) {
            let (a, b) = *rng.pick(OPS);
            let hits: Vec<usize> = line.match_indices(a).map(|(p, _)| p).collect();
            if hits.is_empty() {
                continue;
            }
            let p = *rng.pick(&hits);
            format!("{}{b}{}", &line[..p], &line[p + a.len()..])
        } else {
            // A decimal literal not part of an identifier.
            let bytes = line.as_bytes();
            let mut spans = vec![];
            let mut j = 0;
            while j < bytes.len() {
                if bytes[j].is_ascii_digit()
                    && (j == 0 || !(bytes[j - 1].is_ascii_alphanumeric() || bytes[j - 1] == b'_'))
                {
                    let s = j;
                    while j < bytes.len() && bytes[j].is_ascii_digit() {
                        j += 1;
                    }
                    if j < bytes.len()
                        && (bytes[j].is_ascii_alphabetic() || bytes[j] == b'.' || bytes[j] == b'x')
                    {
                        continue;
                    }
                    spans.push((s, j));
                } else {
                    j += 1;
                }
            }
            if spans.is_empty() {
                continue;
            }
            let (s, e) = *rng.pick(&spans);
            let v: u128 = line[s..e].parse().unwrap_or(0);
            let nv = match rng.below(5) {
                0 => v.saturating_add(1),
                1 => v.saturating_sub(1),
                2 => 0,
                3 => 255,
                _ => 1u128 << rng.below(127),
            };
            format!("{}{nv}{}", &line[..s], &line[e..])
        };
        let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        out[i] = new;
        return Some(out.join("\n") + "\n");
    }
    None
}

/// Calls of the functions whose parameters are all value types.
fn calls(abi: &alloy_json_abi::JsonAbi, rng: &mut Rng, n: usize) -> Vec<Call> {
    let fns: Vec<(String, Vec<Ty>)> = abi
        .functions()
        .filter_map(|f| {
            let tys: Option<Vec<Ty>> = f.inputs.iter().map(|p| ty(&p.ty)).collect();
            Some((f.name.clone(), tys?))
        })
        .collect();
    if fns.is_empty() {
        return vec![];
    }
    (0..n)
        .map(|_| {
            let (name, tys) = rng.pick(&fns).clone();
            Call { func: name, args: tys.iter().map(|t| grammar::arg(rng, *t)).collect() }
        })
        .collect()
}

pub(crate) fn campaign(cli: &Cli) {
    let mut stats = Stats::default();
    let mut rng = Rng::new(cli.seed);
    let work = cli.out.join("work");
    let mut per_file = BTreeMap::<String, u64>::new();
    let mut accepted = BTreeMap::<String, u64>::new();
    // The deployable contracts of each file, by solc.
    let mut targets = vec![];
    for f in &cli.files {
        let Ok(src) = std::fs::read_to_string(f) else { continue };
        if src.contains("import ") {
            continue;
        }
        std::fs::create_dir_all(&work).unwrap();
        let probe = work.join("Probe.sol");
        std::fs::write(&probe, &src).unwrap();
        let mut cmd = std::process::Command::new(&cli.tools.solc);
        cmd.args(&cli.tools.solc_args).args(["--combined-json", "abi,bin"]).arg(&probe);
        let Ok(out) = crate::compile::run(cmd, cli.tools.timeout) else { continue };
        let Ok(json) = serde_json::from_slice::<serde_json::Value>(&out) else { continue };
        for (name, c) in json["contracts"].as_object().into_iter().flatten() {
            let short = name.rsplit(':').next().unwrap().to_string();
            let bin = c["bin"].as_str().unwrap_or("");
            let abi: alloy_json_abi::JsonAbi = match &c["abi"] {
                serde_json::Value::String(s) => serde_json::from_str(s).unwrap_or_default(),
                v => serde_json::from_value(v.clone()).unwrap_or_default(),
            };
            let ctor_ok = abi.constructor.as_ref().is_none_or(|c| c.inputs.is_empty());
            if !bin.is_empty() && ctor_ok && abi.functions().next().is_some() {
                targets.push((f.display().to_string(), src.clone(), short, abi));
            }
        }
    }
    eprintln!("corpus: {} deployable contracts", targets.len());
    if targets.is_empty() {
        return;
    }
    for it in 0..cli.iters {
        let (file, src, name, abi) = rng.pick(&targets).clone();
        let mut mutant = src.clone();
        for _ in 0..rng.below(3) {
            if let Some(m) = mutate(&mutant, &mut rng) {
                mutant = m;
            }
        }
        let case = Case {
            contract: None,
            source: Some(mutant),
            name: name.clone(),
            calls: calls(&abi, &mut rng, cli.calls),
            divergence: None,
        };
        let out = case::run(cli, &case, &work);
        stats.merge(&out.stats);
        *per_file.entry(format!("{file}:{name}")).or_default() += 1;
        let mut seen: Vec<&case::Divergence> = vec![];
        for d in &out.divergences {
            if seen.iter().any(|s| s.same(d)) {
                continue;
            }
            if let Some(why) = &d.accepted {
                *accepted.entry(format!("{} {}: {why}", d.legs, d.kind)).or_default() += 1;
                continue;
            }
            seen.push(d);
            let tag = format!("corpus{}-{it}-{}", cli.seed, d.legs.replace('/', "_"));
            let p = case::save(cli, &case, d, &tag);
            eprintln!(
                "{file}:{name} iter {it}: {} {} {}: {}",
                d.legs,
                d.kind,
                p.display(),
                case::short(&d.detail)
            );
        }
        if (it + 1) % 25 == 0 {
            eprintln!(
                "progress: {} cases, {} calls, {} solc rejects, {} divergences",
                stats.cases, stats.calls, stats.solc_rejects, stats.divergences
            );
        }
    }
    let summary = serde_json::json!({"stats": stats, "accepted": accepted, "targets": per_file});
    std::fs::write(cli.out.join("summary.json"), serde_json::to_string_pretty(&summary).unwrap())
        .unwrap();
    println!("{}", serde_json::to_string_pretty(&summary).unwrap());
}
