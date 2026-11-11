//! `--spec-check`: empirical tests of the specs the verifier trusts.
//!
//! The proofs rest on contracts nobody proves: the body of a `@custom:trusted` function (its
//! callers use its requires and ensures), the requires and ensures of an interface (the contract
//! of every external call through it), and the built-in `Math.mulDiv` summary. After Verus's
//! SpecCheck, this mode runs the code behind each such contract that the file contains and
//! compares what it does with what the contract says, on sampled inputs:
//!
//! - a trusted function with a body, or the `Math.mulDiv` the summary stands for;
//! - every implementation of an interface function in a contract that inherits the interface.
//!
//! The file is compiled with Solar after adding a small harness to each tested contract: a setter
//! and a getter for each state variable of a value type and for each mapping from a value type to
//! a value type, and an external wrapper for each internal or private function under test
//! (functions of libraries get a wrapper contract of their own). Each sample deploys the contract
//! in a fresh in-process EVM, with zero constructor arguments, writes random values to that state
//! (mapping entries at the keys the sample uses: its arguments, the caller, the contract and
//! zero), and calls the function with random arguments, mixed with boundary values (0, 1, the
//! type's extremes, powers of two and their neighbours).
//!
//! Specs are evaluated by the verifier's own encoding at the observed values (see
//! `crate::vc::Sample`), so they mean exactly what the proofs assume. A sample whose requires
//! hold (and, for a public entry point, whose contract invariants hold) must satisfy the ensures
//! (and the invariants) when the call returns, and must not revert when its succeeds-if
//! conditions also hold. A sample the observed values do not decide (a spec over ghost state or
//! unobserved storage) counts as undetermined. Each violated spec is reported with the first
//! counterexample found; the counts of samples where the requires held show checks that are
//! vacuous.
//!
//! This is testing, not proof: a contract that passes may still fail on inputs no sample drew,
//! and state the harness does not write (structs, arrays, nested mappings) keeps the values the
//! constructor gave it.
use crate::{
    prog::{Contract, Func, Program, Spec},
    report,
    smt::Smt,
    ty::{Ty, num, pow2},
    vc::{self, Concrete, Sample},
};
use alloy_consensus::{TxLegacy, transaction::Recovered};
use alloy_primitives::{Address, Bytes, TxKind, U256, hex, keccak256};
use evm2::{
    BaseEvmTypes, Evm, Precompiles, SpecId,
    env::BlockEnv,
    ethereum::{TxEnvelope, ethereum_tx_registry},
    evm::{AccountInfo, InMemoryDB},
};
use num_bigint::{BigInt, Sign};
use solar_parse::{
    ast::{self, ElementaryType, ItemKind, TypeKind, Visibility},
    interface::{Session, Span, data_structures::map::FxHashMap},
};
use std::{
    fmt::Write as _,
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};

/// The account that sends every transaction.
const CALLER: Address = Address::repeat_byte(0x22);
/// Gas for each transaction, below Osaka's per-transaction cap.
const GAS: u64 = 10_000_000;
/// The block timestamp of every sample.
const TIMESTAMP: u64 = 1;

pub(crate) struct Opts {
    /// Samples per tested contract.
    pub runs: usize,
    pub seed: u64,
    pub timeout: u64,
    pub only: Option<String>,
}

/// What a file's spec check found.
#[derive(Default)]
pub(crate) struct Totals {
    pub held: usize,
    pub violated: usize,
    pub untested: usize,
}

/// The ABI encoding of a value type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Word {
    Uint(u16),
    Int(u16),
    Bool,
    /// `bytesN`, by bit width: left-aligned in its word.
    Bytes(u16),
}

/// A value type of the harness's interface: the verifier's type and its ABI encoding.
#[derive(Clone, Debug)]
struct VTy {
    ty: Ty,
    word: Word,
    /// The canonical ABI type name, for selectors.
    abi: String,
    /// The type as written in the source, for the harness.
    text: String,
}

/// State the harness reads and writes.
struct Slot {
    name: String,
    /// The key of a mapping.
    key: Option<VTy>,
    val: VTy,
    settable: bool,
}

/// How a sample calls the code under test.
enum Call {
    /// An external or public function of the deployed contract, by signature.
    Direct(String),
    /// The injected wrapper `__sc_call_<i>`.
    Wrapper(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Trusted,
    Summary,
    Interface,
}

/// One trusted contract and the code it is tested against.
struct Target<'p, 'a> {
    kind: Kind,
    /// The function whose specs are tested (the interface's function, for an interface).
    spec_fn: &'p Func<'a>,
    /// The contract whose state and invariants the specs read.
    host: &'p Contract<'a>,
    /// The contract deployed: the host, or the wrapper contract of a library.
    deploy: String,
    call: Call,
    params: Vec<VTy>,
    rets: Vec<VTy>,
    /// Contract invariants are assumed before and owed after: the function is an entry point.
    invariants: bool,
    /// Why the target cannot be tested, when it cannot.
    untestable: Option<String>,
}

impl Target<'_, '_> {
    /// The target's name; `overloaded` adds the parameter types.
    fn label(&self, overloaded: bool) -> String {
        let f = self.spec_fn;
        let n = if overloaded { selector_sig(&f.name, &self.params) } else { f.name.clone() };
        match self.kind {
            Kind::Trusted => format!("{}.{n} (@custom:trusted)", f.contract),
            Kind::Summary => format!("{}.{n} (built-in summary)", f.contract),
            Kind::Interface => format!("{}.{n} (interface {})", self.host.name, f.contract),
        }
    }
}

/// Checks the trusted contracts of `program` (parsed from `file`) against their code.
pub(crate) fn run<'a>(
    sess: &Session,
    unit: &'a ast::SourceUnit<'a>,
    program: &Program<'a>,
    file: &Path,
    o: &Opts,
) -> Totals {
    let mut totals = Totals::default();
    let snippet = |sp: Span| sess.source_map().span_to_snippet(sp).unwrap_or_default();
    let items = contract_items(unit);
    let mut targets = collect(program, &snippet);
    targets.retain(|t| {
        o.only.as_ref().is_none_or(|w| {
            let n = format!("{}.{}", t.spec_fn.contract, t.spec_fn.name);
            let h = format!("{}.{}", t.host.name, t.spec_fn.name);
            *w == n || *w == h || *w == t.spec_fn.contract || *w == t.host.name
        })
    });
    if targets.is_empty() {
        println!("{}: no trusted contract with code to test", file.display());
        return totals;
    }
    let src = sess.source_map().load_file(file).map(|f| f.src.to_string()).unwrap_or_default();
    let (harness, slots) = harness(sess, program, &items, &targets, src, &snippet);
    let name = file.file_name().map_or("spec.sol".into(), |n| n.to_string_lossy().to_string());
    let bins = match compile(&name, &harness) {
        Ok(b) => b,
        Err(e) => {
            sess.dcx.err(format!("spec check: the harness does not compile: {e}")).emit();
            totals.untested += targets.len();
            return totals;
        }
    };
    let none = vec![];
    for (i, t) in targets.iter().enumerate() {
        let overloaded = targets.iter().any(|u| {
            !std::ptr::eq(u, t)
                && u.host.name == t.host.name
                && u.spec_fn.contract == t.spec_fn.contract
                && u.spec_fn.name == t.spec_fn.name
        });
        let label = t.label(overloaded);
        let why = t.untestable.clone().or_else(|| {
            (!bins.contains_key(&t.deploy)).then(|| format!("`{}` has no bytecode", t.deploy))
        });
        if let Some(why) = why {
            println!("- {label}: not tested: {why}");
            totals.untested += 1;
            continue;
        }
        let s = slots.get(&t.deploy).unwrap_or(&none);
        let seed = o.seed ^ (i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        let st = match check_target(program, t, &bins[&t.deploy], s, o, seed) {
            Ok(st) => st,
            Err(why) => {
                println!("- {label}: not tested: {why}");
                totals.untested += 1;
                continue;
            }
        };
        let mut summary = format!(
            "{} samples, requires held in {}, returned in {}",
            st.samples, st.held, st.returned
        );
        if !t.spec_fn.succeeds.is_empty() {
            let _ = write!(summary, ", succeeds-if held in {}", st.succeeds);
        }
        if st.undetermined > 0 {
            let _ = write!(summary, ", {} undetermined", st.undetermined);
        }
        if st.violations.is_empty() {
            totals.held += 1;
            println!("{} {label}: {summary}", crate::mark(true));
            if st.held == 0 {
                let w = format!("{label}: no sample satisfied the requires: the check is vacuous");
                sess.dcx.warn(w).emit();
            } else if st.returned == 0 && !t.spec_fn.ensures.is_empty() {
                let w = format!("{label}: no sample returned normally: the ensures went unchecked");
                sess.dcx.warn(w).emit();
            }
            continue;
        }
        totals.violated += 1;
        println!(
            "{} {label}: {} of its specs violated; {summary}",
            crate::mark(false),
            st.violations.len()
        );
        for v in &st.violations {
            let notes = vec![
                format!("in {label}"),
                format!("failing spec: {}", v.spec.text),
                format!("counterexample: {}", v.cex),
            ];
            report(sess, v.what, v.spec.span, &notes);
        }
    }
    totals
}

/// The contract items of `unit`, by name.
fn contract_items<'a>(unit: &'a ast::SourceUnit<'a>) -> Vec<(String, &'a ast::Item<'a>)> {
    unit.items
        .iter()
        .filter_map(|it| match &it.kind {
            ItemKind::Contract(c) => Some((c.name.as_str().to_string(), it)),
            _ => None,
        })
        .collect()
}

fn ast_contract<'a>(
    items: &[(String, &'a ast::Item<'a>)],
    n: &str,
) -> Option<&'a ast::ItemContract<'a>> {
    items.iter().find(|(m, _)| m == n).and_then(|(_, it)| match &it.kind {
        ItemKind::Contract(c) => Some(c),
        _ => None,
    })
}

fn deployable(c: &Contract<'_>) -> bool {
    c.kind == ast::ContractKind::Contract
}

fn has_specs(f: &Func<'_>) -> bool {
    !f.requires.is_empty() || !f.ensures.is_empty() || !f.succeeds.is_empty()
}

/// The trusted contracts of `program` that have code in the file.
fn collect<'p, 'a>(
    program: &'p Program<'a>,
    snippet: &dyn Fn(Span) -> String,
) -> Vec<Target<'p, 'a>> {
    let mut out: Vec<Target<'p, 'a>> = vec![];
    // Trusted functions: tested in their own contract when it deploys (or is a library), else in
    // the first contract that inherits them and deploys.
    let mut trusted: Vec<(&'p Func<'a>, &'p Contract<'a>)> = vec![];
    for c in program.contracts.iter().filter(|c| !c.kind.is_interface()) {
        for f in &c.funcs {
            if !f.trusted
                || f.synthetic
                || f.item.kind.is_modifier()
                || f.item.kind.is_constructor()
                || !has_specs(f)
            {
                continue;
            }
            let sp = f.item.header.span;
            let runs = |c: &Contract<'_>| deployable(c) || c.kind.is_library();
            match trusted.iter_mut().find(|(g, _)| g.item.header.span == sp) {
                Some(e) => {
                    if !runs(e.1) && runs(c) || runs(c) && c.name == f.contract {
                        e.1 = c;
                    }
                }
                None => trusted.push((f, c)),
            }
        }
    }
    for (f, c) in trusted {
        let i = out.len();
        let (params, rets, mut untestable) = signature(program, f, snippet);
        let vis = f.item.header.visibility();
        let external = matches!(vis, None | Some(Visibility::Public | Visibility::External));
        let lib = c.kind.is_library();
        if f.item.body.is_none() {
            untestable.get_or_insert("it has no body".into());
        } else if !deployable(c) && !lib {
            untestable.get_or_insert(format!(
                "`{}` is abstract and no contract in the file deploys it",
                c.name
            ));
        } else if lib && external {
            untestable.get_or_insert(
                "public library functions run by delegatecall: only internal ones are tested"
                    .into(),
            );
        } else if vis == Some(Visibility::Private) && c.name != f.contract {
            untestable.get_or_insert(format!("it is private to `{}`", f.contract));
        }
        let call = if external && !lib {
            Call::Direct(selector_sig(&f.name, &params))
        } else {
            Call::Wrapper(i)
        };
        out.push(Target {
            kind: if f.summary { Kind::Summary } else { Kind::Trusted },
            spec_fn: f,
            host: c,
            deploy: if lib { format!("__SpecCheck_{}", c.name) } else { c.name.clone() },
            call,
            params,
            rets,
            invariants: f.is_entry() && !lib,
            untestable,
        });
    }
    // Interfaces: every implementation in a deployable contract that inherits the interface.
    for i in program.contracts.iter().filter(|c| c.kind.is_interface()) {
        for f in i.funcs.iter().filter(|f| has_specs(f)) {
            for d in program.contracts.iter().filter(|d| deployable(d) && d.lin.contains(&i.name)) {
                let implemented = d.funcs.iter().any(|g| {
                    g.name == f.name && g.params.len() == f.params.len() && g.item.body.is_some()
                });
                if !implemented {
                    continue;
                }
                let (params, rets, untestable) = signature(program, f, snippet);
                out.push(Target {
                    kind: Kind::Interface,
                    spec_fn: f,
                    host: d,
                    deploy: d.name.clone(),
                    call: Call::Direct(selector_sig(&f.name, &params)),
                    params,
                    rets,
                    invariants: true,
                    untestable,
                });
            }
        }
    }
    out
}

/// The parameter and return types of `f`, and why the harness cannot pass them when it cannot.
fn signature(
    program: &Program<'_>,
    f: &Func<'_>,
    snippet: &dyn Fn(Span) -> String,
) -> (Vec<VTy>, Vec<VTy>, Option<String>) {
    let mut why = None;
    let mut conv = |ps: &[ast::VariableDefinition<'_>], tys: &[(String, Ty)]| -> Vec<VTy> {
        ps.iter()
            .zip(tys)
            .filter_map(|(p, (n, ty))| {
                let v = vty(program, &p.ty, ty, snippet);
                if v.is_none() {
                    why.get_or_insert(format!("`{n}` is not of a value type"));
                }
                v
            })
            .collect()
    };
    let params = conv(&f.item.header.parameters, &f.params);
    let rets = conv(f.item.header.returns(), &f.rets);
    (params, rets, why)
}

/// The value type `t` (the verifier's `ty`), when the harness can pass it.
fn vty(
    program: &Program<'_>,
    t: &ast::Type<'_>,
    ty: &Ty,
    snippet: &dyn Fn(Span) -> String,
) -> Option<VTy> {
    let fixed = fixed_bytes(program, t);
    let (word, abi) = match ty {
        Ty::Bool => (Word::Bool, "bool".to_string()),
        Ty::Uint(n) if fixed => (Word::Bytes(*n), format!("bytes{}", n / 8)),
        Ty::Uint(n) => (Word::Uint(*n), format!("uint{n}")),
        Ty::Int(n) => (Word::Int(*n), format!("int{n}")),
        Ty::Addr | Ty::Contract(_) => (Word::Uint(160), "address".to_string()),
        Ty::Enum(..) => (Word::Uint(8), "uint8".to_string()),
        _ => return None,
    };
    Some(VTy { ty: ty.clone(), word, abi, text: snippet(t.span) })
}

/// Whether `t` is a `bytesN`, directly or under a user-defined value type.
fn fixed_bytes(program: &Program<'_>, t: &ast::Type<'_>) -> bool {
    match &t.kind {
        TypeKind::Elementary(ElementaryType::FixedBytes(_)) => true,
        TypeKind::Custom(p) => program.udvt_bytes.contains(p.last().as_str()),
        _ => false,
    }
}

fn selector_sig(name: &str, params: &[VTy]) -> String {
    let ps: Vec<&str> = params.iter().map(|p| p.abi.as_str()).collect();
    format!("{name}({})", ps.join(","))
}

/// The source with the harness added, and the state each deployed contract exposes.
fn harness<'a>(
    sess: &Session,
    program: &Program<'a>,
    items: &[(String, &'a ast::Item<'a>)],
    targets: &[Target<'_, 'a>],
    src: String,
    snippet: &dyn Fn(Span) -> String,
) -> (String, FxHashMap<String, Vec<Slot>>) {
    let mut slots: FxHashMap<String, Vec<Slot>> = FxHashMap::default();
    // Generated functions, by the contract they go in, in a stable order.
    let mut code: Vec<(String, String)> = vec![];
    let mut add = |deploy: &str, f: String| match code.iter_mut().find(|(d, _)| d == deploy) {
        Some((_, c)) => c.push_str(&f),
        None => code.push((deploy.to_string(), f)),
    };
    for (k, t) in targets.iter().enumerate() {
        if t.untestable.is_some() {
            continue;
        }
        if !t.host.kind.is_library() && !slots.contains_key(&t.deploy) {
            let s = state_slots(program, t.host, items, snippet);
            for sl in &s {
                let (n, v) = (&sl.name, &sl.val.text);
                add(
                    &t.deploy,
                    match &sl.key {
                        None if sl.settable => format!(
                            "\n    function __sc_set_{n}({v} v) external {{ {n} = v; }}\n    function __sc_get_{n}() external view returns ({v}) {{ return {n}; }}"
                        ),
                        None => format!(
                            "\n    function __sc_get_{n}() external view returns ({v}) {{ return {n}; }}"
                        ),
                        Some(key) => format!(
                            "\n    function __sc_set_{n}({k} k, {v} v) external {{ {n}[k] = v; }}\n    function __sc_get_{n}({k} k) external view returns ({v}) {{ return {n}[k]; }}",
                            k = key.text
                        ),
                    },
                );
            }
            slots.insert(t.deploy.clone(), s);
        }
        if let Call::Wrapper(_) = t.call {
            let f = t.spec_fn;
            // A wrapper contract names the library's own types through the library.
            let lib = t.host.kind.is_library().then(|| ast_contract(items, &t.host.name)).flatten();
            let qualify = |text: &str| -> String {
                let local = lib.is_some_and(|l| {
                    l.body.iter().any(|it| match &it.kind {
                        ItemKind::Enum(e) => e.name.as_str() == text,
                        ItemKind::Struct(st) => st.name.as_str() == text,
                        ItemKind::Udvt(u) => u.name.as_str() == text,
                        _ => false,
                    })
                });
                if local { format!("{}.{text}", t.host.name) } else { text.to_string() }
            };
            let ps: Vec<String> = t
                .params
                .iter()
                .enumerate()
                .map(|(j, p)| format!("{} p{j}", qualify(&p.text)))
                .collect();
            let args: Vec<String> = (0..t.params.len()).map(|j| format!("p{j}")).collect();
            let callee = if t.host.kind.is_library() {
                format!("{}.{}", t.host.name, f.name)
            } else {
                f.name.clone()
            };
            let rets: Vec<String> = t.rets.iter().map(|r| qualify(&r.text)).collect();
            let (returns, ret) = if rets.is_empty() {
                (String::new(), "")
            } else {
                (format!(" returns ({})", rets.join(", ")), "return ")
            };
            add(
                &t.deploy,
                format!(
                    "\n    function __sc_call_{k}({}) external{returns} {{ {ret}{callee}({}); }}",
                    ps.join(", "),
                    args.join(", ")
                ),
            );
        }
    }
    let mut ins: Vec<(usize, String)> = vec![];
    let mut tail = String::new();
    for (deploy, f) in &code {
        if let Some((_, it)) = items.iter().find(|(n, _)| n == deploy) {
            // Before the contract's closing brace.
            let hi = sess.source_map().lookup_byte_offset(it.span.hi()).pos.0 as usize;
            ins.push((hi.saturating_sub(1), format!("{f}\n")));
        } else {
            let _ = write!(tail, "\ncontract {deploy} {{{f}\n}}\n");
        }
    }
    ins.sort_by_key(|a| std::cmp::Reverse(a.0));
    let mut out = src;
    for (at, t) in ins {
        out.insert_str(at, &t);
    }
    out.push_str(&tail);
    (out, slots)
}

/// The state of `c` the harness writes and reads.
fn state_slots<'a>(
    program: &Program<'a>,
    c: &Contract<'a>,
    items: &[(String, &'a ast::Item<'a>)],
    snippet: &dyn Fn(Span) -> String,
) -> Vec<Slot> {
    let mut out = vec![];
    for sv in c.state.iter().filter(|s| !s.constant && !s.ghost && !s.transient) {
        // The declaration, in the contract or a base whose variable it can reach.
        let decl = std::iter::once(&c.name).chain(c.lin.iter().rev()).find_map(|b| {
            ast_contract(items, b)?.body.iter().find_map(|it| match &it.kind {
                ItemKind::Variable(v)
                    if v.name.is_some_and(|n| n.as_str() == sv.name)
                        && (b == &c.name || v.visibility != Some(Visibility::Private)) =>
                {
                    Some(v)
                }
                _ => None,
            })
        });
        let Some(v) = decl else { continue };
        match (&v.ty.kind, &sv.ty) {
            (TypeKind::Mapping(m), Ty::Map(kt, vt)) => {
                if let (Some(key), Some(val)) =
                    (vty(program, &m.key, kt, snippet), vty(program, &m.value, vt, snippet))
                {
                    out.push(Slot { name: sv.name.clone(), key: Some(key), val, settable: true });
                }
            }
            _ => {
                if let Some(val) = vty(program, &v.ty, &sv.ty, snippet) {
                    let settable = v.mutability.is_none();
                    out.push(Slot { name: sv.name.clone(), key: None, val, settable });
                }
            }
        }
    }
    out
}

static COMPILES: AtomicUsize = AtomicUsize::new(0);

/// Compiles `src` with Solar; returns each contract's creation code.
fn compile(name: &str, src: &str) -> Result<FxHashMap<String, Vec<u8>>, String> {
    let dir = std::env::temp_dir().join(format!(
        "solar-verify-spec-check-{}-{}",
        std::process::id(),
        COMPILES.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(name);
    std::fs::write(&path, src).map_err(|e| e.to_string())?;
    let opts = solar_cli::CompileOpts {
        input: vec![path.to_string_lossy().to_string()],
        emit: vec![solar_cli::config::CompilerOutput::Bin],
        out_dir: Some(dir.clone()),
        threads: 1.into(),
        // The harness's own warnings (wrappers that could be view) say nothing about the file.
        no_warnings: true,
        ..Default::default()
    };
    // The compiler keeps its session in thread-local state: give it a thread of its own.
    let ok = std::thread::spawn(move || solar_cli::run_compiler_args(opts).is_ok())
        .join()
        .unwrap_or(false);
    // The compiler's output file, outside any source map.
    #[allow(clippy::disallowed_methods)]
    let json = std::fs::read_to_string(dir.join("combined.json"));
    if std::env::var_os("SOLAR_VERIFY_KEEP_HARNESS").is_some() {
        eprintln!("spec check harness: {}", path.display());
    } else {
        let _ = std::fs::remove_dir_all(&dir);
    }
    if !ok {
        return Err("see the compiler's errors above".into());
    }
    let json: serde_json::Value =
        serde_json::from_str(&json.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let mut out = FxHashMap::default();
    for (k, v) in json.get("contracts").and_then(|c| c.as_object()).into_iter().flatten() {
        let Some(bin) = v.get("bin").and_then(|b| b.as_str()) else { continue };
        if let Ok(code) = hex::decode(bin)
            && !code.is_empty()
        {
            out.insert(k.rsplit(':').next().unwrap_or(k).to_string(), code);
        }
    }
    Ok(out)
}

/// SplitMix64.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next() % n }
    }

    /// A uniform value of `bits` bits.
    fn bits(&mut self, bits: u32) -> BigInt {
        let mut v = BigInt::from(0);
        let mut left = bits;
        while left > 0 {
            let k = left.min(64);
            let x = if k == 64 { self.next() } else { self.next() & ((1u64 << k) - 1) };
            v = (v << k) + x;
            left -= k;
        }
        v
    }
}

/// The boundary values of a type with range `[lo, hi]`.
fn boundaries(lo: &BigInt, hi: &BigInt, bits: u32, rng: &mut Rng) -> Vec<BigInt> {
    let k = 1 + rng.below(u64::from(bits.max(2) - 1)) as u32;
    let mut v = vec![
        BigInt::from(0),
        BigInt::from(1),
        hi.clone(),
        hi - 1u32,
        lo.clone(),
        lo + 1u32,
        BigInt::from(-1),
        BigInt::from(2),
        pow2(k),
        pow2(k) - 1u32,
        pow2(k) + 1u32,
        hi / 2u32 + 1u32,
    ];
    v.retain(|x| x >= lo && x <= hi);
    v.sort();
    v.dedup();
    v
}

/// A value of `ty` for argument `j` of sample `i`: boundary values first, then a mix of
/// boundary, small, log-uniform and uniform values.
fn draw(ty: &Ty, i: usize, j: usize, rng: &mut Rng) -> BigInt {
    if matches!(ty, Ty::Bool) {
        return BigInt::from(rng.below(2));
    }
    let Some((lo, hi)) = ty.bounds() else { return BigInt::from(0) };
    let bits = (&hi - &lo).bits().max(1) as u32;
    let bs = boundaries(&lo, &hi, bits, rng);
    if i < 2 * bs.len() {
        return bs[(i + 5 * j) % bs.len()].clone();
    }
    let v = match rng.below(5) {
        0 => bs[rng.below(bs.len() as u64) as usize].clone(),
        1 => BigInt::from(rng.below(17)),
        2 | 3 => {
            let k = 1 + rng.below(u64::from(bits)) as u32;
            let m = rng.bits(k);
            if lo < BigInt::from(0) && rng.below(2) == 0 { -m } else { m }
        }
        _ => rng.bits(bits) + &lo,
    };
    if v < lo || v > hi { BigInt::from(rng.below(17)).min(hi) } else { v }
}

fn encode(w: Word, v: &BigInt) -> [u8; 32] {
    let m = pow2(256);
    let x = match w {
        Word::Bytes(n) => v << (256 - u32::from(n)),
        _ => ((v % &m) + &m) % &m,
    };
    let (_, bytes) = x.to_bytes_be();
    let mut out = [0u8; 32];
    let k = bytes.len().min(32);
    out[32 - k..].copy_from_slice(&bytes[bytes.len() - k..]);
    out
}

fn decode(w: Word, b: &[u8]) -> BigInt {
    let x = BigInt::from_bytes_be(Sign::Plus, b);
    match w {
        Word::Bytes(n) => x >> (256 - u32::from(n)),
        Word::Int(_) if b[0] & 0x80 != 0 => x - pow2(256),
        Word::Bool => BigInt::from(u8::from(x != BigInt::from(0))),
        _ => x,
    }
}

/// The SMT literal of `v` as a value of `ty`.
fn term(ty: &Ty, v: &BigInt) -> String {
    match ty {
        Ty::Bool => (v != &BigInt::from(0)).to_string(),
        _ => num(v),
    }
}

/// `v` as a counterexample shows it.
fn show(t: &VTy, v: &BigInt) -> String {
    match (&t.ty, t.word) {
        (Ty::Bool, _) => (v != &BigInt::from(0)).to_string(),
        (Ty::Addr | Ty::Contract(_), _) => format!("{v:#x}"),
        (_, Word::Bytes(n)) => format!("0x{v:0width$x}", width = usize::from(n) / 4),
        _ => v.to_string(),
    }
}

fn addr_int(a: Address) -> BigInt {
    BigInt::from_bytes_be(Sign::Plus, a.as_slice())
}

/// A fresh in-process chain with one funded sender.
struct Chain {
    evm: Evm<'static, BaseEvmTypes>,
    nonce: u64,
}

impl Chain {
    fn new() -> Self {
        let spec = SpecId::OSAKA;
        let mut db = InMemoryDB::default();
        db.insert_account_info(&CALLER, AccountInfo::default().with_balance(U256::MAX));
        let block =
            BlockEnv::<BaseEvmTypes> { timestamp: U256::from(TIMESTAMP), ..Default::default() };
        let registry = ethereum_tx_registry(spec);
        let evm = Evm::<BaseEvmTypes>::new(spec, block, registry, db, Precompiles::base(spec));
        Self { evm, nonce: 0 }
    }

    /// Sends a transaction; returns whether it succeeded, its output and the created address.
    fn tx(
        &mut self,
        to: TxKind,
        input: Vec<u8>,
    ) -> Result<(bool, Vec<u8>, Option<Address>), String> {
        let tx = Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                nonce: self.nonce,
                to,
                input: Bytes::from(input),
                gas_price: 0,
                value: U256::ZERO,
                chain_id: None,
                gas_limit: GAS,
            }),
            CALLER,
        );
        self.nonce += 1;
        let r = self
            .evm
            .transact(&tx)
            .map(evm2::ExecutedTx::commit)
            .map_err(|e| format!("transaction rejected: {e}"))?;
        Ok((r.status, r.output.to_vec(), r.created_address))
    }

    fn call(
        &mut self,
        at: Address,
        sig: &str,
        words: &[[u8; 32]],
    ) -> Result<(bool, Vec<u8>), String> {
        let mut input = keccak256(sig.as_bytes())[..4].to_vec();
        for w in words {
            input.extend_from_slice(w);
        }
        self.tx(TxKind::Call(at), input).map(|(ok, out, _)| (ok, out))
    }
}

struct Violation<'p, 'a> {
    what: &'static str,
    spec: &'p Spec<'a>,
    cex: String,
}

#[derive(Default)]
struct Stats<'p, 'a> {
    samples: usize,
    held: usize,
    returned: usize,
    succeeds: usize,
    undetermined: usize,
    violations: Vec<Violation<'p, 'a>>,
}

/// The value of a conjunction: false when a conjunct is false, else undetermined when one is.
fn all(v: &[Option<bool>]) -> Option<bool> {
    if v.contains(&Some(false)) {
        Some(false)
    } else if v.contains(&None) {
        None
    } else {
        Some(true)
    }
}

const REVERTS: &str = "call reverts where @custom:succeeds-if says it returns";

fn check_target<'p, 'a>(
    program: &'p Program<'a>,
    t: &Target<'p, 'a>,
    bin: &[u8],
    slots: &[Slot],
    o: &Opts,
    seed: u64,
) -> Result<Stats<'p, 'a>, String> {
    let f = t.spec_fn;
    let smt = Smt::new(o.timeout, false).map_err(|e| format!("could not start z3: {e}"))?;
    let mut v = vc::Vc::new(program, t.host, smt, &vc::Opts { strict: false });
    let invs: Vec<&'p Spec<'a>> =
        if t.invariants { t.host.invariants.iter().collect() } else { vec![] };
    let pre_specs: Vec<&'p Spec<'a>> = f.requires.iter().chain(invs.iter().copied()).collect();
    let succ_specs: Vec<&'p Spec<'a>> = f.succeeds.iter().collect();
    let post_specs: Vec<(&'p Spec<'a>, &'static str)> = f
        .ensures
        .iter()
        .map(|s| (s, "ensures does not hold"))
        .chain(invs.iter().map(|s| (*s, "contract invariant does not hold after the call")))
        .collect();
    let post_only: Vec<&'p Spec<'a>> = post_specs.iter().map(|(s, _)| *s).collect();
    // Counterexamples show the state the specs read.
    let texts: Vec<&str> =
        pre_specs.iter().chain(&succ_specs).chain(&post_only).map(|s| s.text.as_str()).collect();
    let eval = |v: &mut vc::Vc<'p, 'a>, s: &Sample<'_>, specs: &[&'p Spec<'a>]| {
        v.eval_sample(f, s, specs).map_err(|(_, m)| format!("spec not evaluated: {m}"))
    };
    let mut rng = Rng(seed);
    let mut st = Stats::default();
    let nctor = t
        .host
        .funcs
        .iter()
        .find(|g| g.item.kind.is_constructor() && g.contract == t.host.name)
        .map_or(0, |g| g.params.len());
    let mut creation = bin.to_vec();
    if !t.host.kind.is_library() {
        creation.extend(std::iter::repeat_n(0u8, 32 * nctor));
    }
    let sig = match &t.call {
        Call::Direct(sig) => sig.clone(),
        Call::Wrapper(k) => selector_sig(&format!("__sc_call_{k}"), &t.params),
    };
    for i in 0..o.runs {
        let mut chain = Chain::new();
        let (ok, out, addr) = chain.tx(TxKind::Create, creation.clone())?;
        let Some(at) = addr.filter(|_| ok) else {
            return Err(format!(
                "deploying `{}` with zero constructor arguments reverted (0x{})",
                t.deploy,
                hex::encode(out)
            ));
        };
        let args: Vec<BigInt> =
            t.params.iter().enumerate().map(|(j, p)| draw(&p.ty, i, j, &mut rng)).collect();
        let keys = write_state(&mut chain, at, slots, t, &args, i, &mut rng)?;
        let pre = read_state(&mut chain, at, slots, &keys)?;
        let words: Vec<[u8; 32]> =
            t.params.iter().zip(&args).map(|(p, a)| encode(p.word, a)).collect();
        let (ok, out) = chain.call(at, &sig, &words)?;
        st.samples += 1;
        let (caller, this) = (num(&addr_int(CALLER)), num(&addr_int(at)));
        let ctx: Vec<(&'static str, String)> = vec![
            ("msg.sender", caller.clone()),
            ("tx.origin", caller),
            ("this", this),
            ("msg.value", "0".into()),
            ("block.timestamp", TIMESTAMP.to_string()),
            ("block.number", "0".into()),
        ];
        let arg_terms: Vec<String> =
            t.params.iter().zip(&args).map(|(p, a)| term(&p.ty, a)).collect();
        let pre_c = concrete(slots, &pre);
        let sample = Sample { ctx: &ctx, args: &arg_terms, pre: &pre_c, post: None, rets: &[] };
        match all(&eval(&mut v, &sample, &pre_specs)?) {
            Some(true) => st.held += 1,
            Some(false) => continue,
            None => {
                st.undetermined += 1;
                continue;
            }
        }
        let inputs = || {
            let mut s: Vec<String> = f
                .params
                .iter()
                .zip(&t.params)
                .zip(&args)
                .map(|(((n, _), p), a)| format!("{n} = {}", show(p, a)))
                .collect();
            if texts.iter().any(|t| mentions(t, "msg.sender")) {
                s.push(format!("msg.sender = {CALLER:#x}"));
            }
            let state = show_state(slots, &pre, &texts);
            if !state.is_empty() {
                s.push(format!("state before: {state}"));
            }
            s.join(", ")
        };
        if !succ_specs.is_empty() && all(&eval(&mut v, &sample, &succ_specs)?) == Some(true) {
            st.succeeds += 1;
            if !ok && !st.violations.iter().any(|x| x.what == REVERTS) {
                let data = if out.is_empty() {
                    String::new()
                } else {
                    format!(" with 0x{}", hex::encode(&out))
                };
                let cex = format!("{}; reverted{data}", inputs());
                st.violations.push(Violation { what: REVERTS, spec: succ_specs[0], cex });
            }
        }
        if !ok {
            continue;
        }
        st.returned += 1;
        if out.len() < 32 * t.rets.len() {
            return Err(format!("a call returned {} bytes for {} values", out.len(), t.rets.len()));
        }
        let rets: Vec<BigInt> =
            t.rets.iter().enumerate().map(|(j, r)| decode(r.word, &out[32 * j..][..32])).collect();
        let ret_terms: Vec<String> =
            t.rets.iter().zip(&rets).map(|(r, x)| term(&r.ty, x)).collect();
        let post = read_state(&mut chain, at, slots, &keys)?;
        let post_c = concrete(slots, &post);
        let sample = Sample { post: Some(&post_c), rets: &ret_terms, ..sample };
        let ens = eval(&mut v, &sample, &post_only)?;
        for ((sp, what), r) in post_specs.iter().zip(&ens) {
            if *r != Some(false) || st.violations.iter().any(|x| std::ptr::eq(x.spec, *sp)) {
                continue;
            }
            let rs: Vec<String> = f
                .rets
                .iter()
                .zip(&t.rets)
                .zip(&rets)
                .map(|(((n, _), r), x)| {
                    let n = if n.starts_with("__ret") { "result" } else { n.as_str() };
                    format!("{n} = {}", show(r, x))
                })
                .collect();
            let mut cex = inputs();
            if !rs.is_empty() {
                let _ = write!(cex, "; returned {}", rs.join(", "));
            }
            let after = show_state(slots, &post, &texts);
            if after != show_state(slots, &pre, &texts) {
                let _ = write!(cex, "; state after: {after}");
            }
            st.violations.push(Violation { what, spec: sp, cex });
        }
    }
    Ok(st)
}

/// Writes random values to `slots`; returns the keys written of each mapping.
fn write_state(
    chain: &mut Chain,
    at: Address,
    slots: &[Slot],
    t: &Target<'_, '_>,
    args: &[BigInt],
    i: usize,
    rng: &mut Rng,
) -> Result<Vec<Vec<BigInt>>, String> {
    let mut keys_of = vec![];
    for (j, s) in slots.iter().enumerate() {
        let (val, mut keys) = (&s.val, vec![]);
        match &s.key {
            None if s.settable => {
                let x = draw(&val.ty, i, j + 7, rng);
                let sig = format!("__sc_set_{}({})", s.name, val.abi);
                chain.call(at, &sig, &[encode(val.word, &x)])?;
            }
            None => {}
            Some(k) => {
                keys.extend(
                    t.params
                        .iter()
                        .zip(args)
                        .filter(|(p, _)| p.word == k.word)
                        .map(|(_, a)| a.clone()),
                );
                if k.word == Word::Uint(160) {
                    keys.extend([addr_int(CALLER), addr_int(at)]);
                }
                keys.extend([BigInt::from(0), draw(&k.ty, i, j, rng)]);
                keys.sort();
                keys.dedup();
                let sig = format!("__sc_set_{}({},{})", s.name, k.abi, val.abi);
                for key in &keys {
                    let x = if rng.below(4) == 0 {
                        BigInt::from(0)
                    } else {
                        draw(&val.ty, i, j + 11, rng)
                    };
                    chain.call(at, &sig, &[encode(k.word, key), encode(val.word, &x)])?;
                }
            }
        }
        keys_of.push(keys);
    }
    Ok(keys_of)
}

/// Observed values: per slot, (key, value) pairs (one with key 0 for a plain variable).
type Observed = Vec<Vec<(BigInt, BigInt)>>;

fn read_state(
    chain: &mut Chain,
    at: Address,
    slots: &[Slot],
    keys_of: &[Vec<BigInt>],
) -> Result<Observed, String> {
    let mut out = vec![];
    for (s, keys) in slots.iter().zip(keys_of) {
        let mut vals = vec![];
        match &s.key {
            None => {
                let (ok, o) = chain.call(at, &format!("__sc_get_{}()", s.name), &[])?;
                if ok && o.len() >= 32 {
                    vals.push((BigInt::from(0), decode(s.val.word, &o[..32])));
                }
            }
            Some(k) => {
                let sig = format!("__sc_get_{}({})", s.name, k.abi);
                for key in keys {
                    let (ok, o) = chain.call(at, &sig, &[encode(k.word, key)])?;
                    if ok && o.len() >= 32 {
                        vals.push((key.clone(), decode(s.val.word, &o[..32])));
                    }
                }
            }
        }
        out.push(vals);
    }
    Ok(out)
}

fn concrete(slots: &[Slot], obs: &Observed) -> Vec<(String, Concrete)> {
    slots
        .iter()
        .zip(obs)
        .filter_map(|(s, vals)| {
            let c = match &s.key {
                None => Concrete::Term(term(&s.val.ty, &vals.first()?.1)),
                Some(k) => Concrete::Map(
                    vals.iter().map(|(key, x)| (term(&k.ty, key), term(&s.val.ty, x))).collect(),
                ),
            };
            Some((s.name.clone(), c))
        })
        .collect()
}

/// Whether spec text `text` names `name`.
fn mentions(text: &str, name: &str) -> bool {
    let ident = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    text.match_indices(name).any(|(i, _)| {
        !text[..i].ends_with(|c: char| ident(c) || c == '.')
            && !text[i + name.len()..].starts_with(ident)
    })
}

/// The observed state of the slots `texts` mention.
fn show_state(slots: &[Slot], obs: &Observed, texts: &[&str]) -> String {
    let mut out: Vec<String> = vec![];
    for (s, vals) in slots.iter().zip(obs) {
        if !texts.iter().any(|t| mentions(t, &s.name)) {
            continue;
        }
        for (key, x) in vals {
            match &s.key {
                None => out.push(format!("{} = {}", s.name, show(&s.val, x))),
                Some(k) => out.push(format!("{}[{}] = {}", s.name, show(k, key), show(&s.val, x))),
            }
        }
    }
    out.join(", ")
}
