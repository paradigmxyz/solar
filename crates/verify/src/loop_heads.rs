//! The loop-head manifest: `--emit-loop-heads FILE` writes, for every loop the verification of the
//! file reaches, what each SSA term at the loop's head denotes in the source: a state variable
//! (with its storage slot and offset from the compiler's own storage layout), a parameter, a
//! return variable or a local. `--check-loop-heads FILE` builds the manifest again and rejects one
//! that differs, naming the first field: a manifest of another source (its sha256), another
//! compiler, other options, or another slot is stale.
//!
//! The manifest binds one source file, the compiler build (its version names the commit, with
//! `-dev` for a build from a modified tree) and the options that change the terms (SSA numbers
//! depend on what the run declares, so on `--strict` and on inference). A file that imports
//! others is refused: its storage layout would need them.

use crate::vc::LoopHead;
use serde_json::{Value, json};
use solar_parse::interface::{Session, Span};
use std::collections::BTreeMap;

/// The manifest format, which a reader checks first.
pub(crate) const FORMAT: &str = "solar-verify loop heads 1";

/// One verified function's loop heads, as the run reported them.
pub(crate) struct FnHeads {
    pub name: String,
    pub contract: String,
    pub status: String,
    pub heads: Vec<LoopHead>,
}

/// The storage layout of each contract of `src` (at `path`), label to entry, from the compiler's
/// Standard JSON output.
pub(crate) fn storage_layouts(
    path: &str,
    src: &str,
) -> Result<BTreeMap<String, BTreeMap<String, Value>>, String> {
    let input = json!({
        "language": "Solidity",
        "sources": { path: { "content": src } },
        "settings": { "outputSelection": { "*": { "*": ["storageLayout"] } } },
    })
    .to_string();
    // The compiler keeps its session in thread-local state: give it a thread of its own.
    let out = std::thread::spawn(move || {
        let mut out = Vec::new();
        solar_cli::standard_json::compile_standard_json(
            &input,
            solar_cli::CompileOpts::default(),
            None,
            &mut out,
        )
        .map(|()| out)
        .map_err(|e| e.to_string())
    })
    .join()
    .map_err(|_| "the compiler panicked".to_string())??;
    let out: Value = serde_json::from_slice(&out).map_err(|e| e.to_string())?;
    if let Some(e) = out["errors"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|e| e["severity"].as_str() == Some("error"))
    {
        return Err(format!("the compiler rejects the file: {}", e["message"]));
    }
    let mut layouts = BTreeMap::new();
    for (name, c) in out["contracts"][path].as_object().into_iter().flatten() {
        let mut vars = BTreeMap::new();
        for e in c["storageLayout"]["storage"].as_array().into_iter().flatten() {
            let label = e["label"].as_str().unwrap_or_default().to_string();
            if vars.insert(label.clone(), e.clone()).is_some() {
                return Err(format!("contract {name} has two state variables named {label}"));
            }
        }
        layouts.insert(name.clone(), vars);
    }
    Ok(layouts)
}

/// A term's SSA name: the symbol without its bars, or None for a compound term.
fn ssa(t: &str) -> Option<String> {
    let inner = t.strip_prefix('|')?.strip_suffix('|')?;
    (!inner.contains('|')).then(|| inner.to_string())
}

/// A term as the manifest writes it: `{"ssa": name}` for a symbol, else `{"term": text}`.
fn term(t: &str) -> Value {
    match ssa(t) {
        Some(n) => json!({ "ssa": n }),
        None => json!({ "term": t }),
    }
}

/// The 1-based line and column of `sp`'s start.
fn line_col(sess: &Session, sp: Span) -> (usize, usize) {
    let p = sess.source_map().lookup_char_pos(sp.lo());
    (p.data.line, p.data.col.0 + 1)
}

/// The manifest of one run.
pub(crate) fn manifest(
    sess: &Session,
    path: &str,
    src: &str,
    options: Value,
    fns: &[FnHeads],
) -> Result<Value, String> {
    use sha2::Digest;
    let layouts = storage_layouts(path, src)?;
    let digest: String =
        sha2::Sha256::digest(src.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
    let mut functions = vec![];
    for f in fns {
        let mut loops = vec![];
        let mut seen = BTreeMap::<(usize, usize), usize>::new();
        for h in &f.heads {
            let (line, col) = line_col(sess, h.span);
            let occurrence = seen.entry((line, col)).or_insert(0);
            let mut vars = vec![];
            for v in &h.vars {
                let mut o = json!({
                    "name": v.name,
                    "kind": v.kind,
                    "havocked": v.havocked,
                    "sort": v.sort,
                    "head": term(&v.head),
                    "entry": term(&v.entry),
                });
                if let Some(l) = &v.head_len {
                    o["head_length"] = term(l);
                }
                if let Some(l) = &v.entry_len {
                    o["entry_length"] = term(l);
                }
                if matches!(v.kind, "storage" | "transient") {
                    let e = layouts
                        .get(&f.contract)
                        .and_then(|l| l.get(&v.name))
                        .filter(|_| v.kind == "storage")
                        .ok_or_else(|| {
                            format!(
                                "{}: the compiler's storage layout of {} has no slot for {} ({})",
                                f.name, f.contract, v.name, v.kind
                            )
                        })?;
                    o["slot"] = e["slot"].clone();
                    o["offset"] = e["offset"].clone();
                    o["type"] = e["type"].clone();
                }
                vars.push(o);
            }
            loops.push(
                json!({ "line": line, "column": col, "occurrence": *occurrence, "vars": vars }),
            );
            *occurrence += 1;
        }
        functions.push(json!({ "function": f.name, "status": f.status, "loops": loops }));
    }
    Ok(json!({
        "format": FORMAT,
        "source": { "path": path, "sha256": digest },
        "compiler": solar_cli::version::solc_compat_version(),
        "options": options,
        "functions": functions,
    }))
}

/// The first path at which `a` and `b` differ, with both values, or None.
fn first_diff(a: &Value, b: &Value, path: &str) -> Option<(String, Value, Value)> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let keys: std::collections::BTreeSet<&String> = x.keys().chain(y.keys()).collect();
            keys.into_iter().find_map(|k| {
                let n = |v: Option<&Value>| v.cloned().unwrap_or(Value::Null);
                first_diff(&n(x.get(k)), &n(y.get(k)), &format!("{path}.{k}"))
            })
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => x
            .iter()
            .zip(y)
            .enumerate()
            .find_map(|(i, (u, v))| first_diff(u, v, &format!("{path}[{i}]"))),
        _ => (a != b).then(|| (path.to_string(), a.clone(), b.clone())),
    }
}

/// Why the manifest `given` is not the one this run builds (`built`), or None. The format and
/// the source digest are checked first, so a manifest of another file says so.
pub(crate) fn check(given: &Value, built: &Value) -> Option<String> {
    for key in ["format", "source", "compiler", "options", "functions"] {
        if let Some((p, g, b)) = first_diff(&given[key], &built[key], key) {
            return Some(format!("{p} is {g} in the manifest, {b} for this source and compiler"));
        }
    }
    first_diff(given, built, "").map(|(p, g, b)| format!("{p} is {g} in the manifest, {b} here"))
}
