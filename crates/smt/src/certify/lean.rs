//! Lean 4 certificates for the provers' `unsat` verdicts: the query becomes a Lean theorem that
//! its assertions imply `False`, proved by `bv_decide`, which bit-blasts the goal, has CaDiCaL
//! refute it with an LRAT proof, and checks that proof with a verified checker. It certifies
//! queries cvc5 cannot: cvc5's full proofs of these queries' wide arrays of bytes often take
//! minutes to produce, or are never found, while the same query answers `unsat` in a second
//! without proofs.
//!
//! [`Lean::certify`] translates the reference script of [`reference_script`], with every
//! `define-fun` inlined, with [`theorem`], writes it to `stem.lean` and runs Lean on it. The
//! certificate holds only when Lean exits cleanly and the theorem depends on no axiom beyond
//! Lean's standard ones and `bv_decide`'s native evaluation of the LRAT checker, so a `sorry`,
//! or any error, fails it.
//!
//! # Translation
//!
//! Each declared constant or function becomes a variable of the theorem and each assertion a
//! hypothesis; the goal is `False`. Bit-vectors map to `BitVec n` with the operations of the
//! same SMT-LIB semantics: `bvudiv` to `BitVec.smtUDiv`, `bvsdiv` to `BitVec.smtSDiv`, `bvurem` to
//! `BitVec.umod`, whose zero-divisor cases agree with SMT-LIB's, and so on. Formulas map to
//! propositions, a declared `Bool` to a `Bool` variable compared with `true`.
//!
//! An array `(Array (_ BitVec a) (_ BitVec b))` becomes a function `BitVec a → BitVec b`: `select`
//! applies it, and `store m i v` is the function `fun j => if j = i then v else m j`. SMT-LIB's
//! arrays are extensional, so they are exactly these functions, and array equality is function
//! equality. The theorem quantifies over every function, so it implies the query is `unsat`.
//! `bv_decide` does not reason about functions: it takes each application of a variable, such as
//! `m j`, as an opaque bit-vector, so the proof gets the congruences it needs as hypotheses
//! proved with `congrArg` (Ackermann's reduction): for each array or function and each pair of
//! argument lists it is applied to, equal arguments give equal results. An array defined by a
//! top-level `(= m t)` is substituted away with `subst`, so reads through it reach the arrays it
//! is built from, and then `store` unfolds to `if`s. Any other array equality, such as a join's
//! `(or (and (= m9 m6) ...) ...)`, is weakened where it occurs under `and` and `or`: the proof
//! derives, with `congrFun`, the hypothesis with `a = b` replaced by `a i = b i` for every index
//! `i` at which an array related to `a` or `b` by such equalities is read, the finite set of
//! indices the query can observe. All of this happens inside Lean's proof, and a weaker hypothesis
//! or a missing congruence can only fail the proof, never prove a false theorem: the theorem's
//! statement is the query's assertions as written.
//!
//! What a certificate trusts: the translation standing for the query (this module and
//! [`reference_script`]'s inlining), Lean's kernel, and `bv_decide`'s LRAT checker, which the
//! kernel does not run but trusts Lean's compiler to evaluate (the theorem's
//! `query._native.bv_decide` axioms, or `Lean.ofReduceBool` in older Lean). The checker itself is
//! verified in Lean, so CaDiCaL is not trusted.
//!
//! Not translated: quantifiers, uninterpreted sorts, arrays of arrays or `Bool`s, and the rarer
//! bit-vector operations (`repeat`, rotations, `bvcomp`); such a query gets no Lean certificate.
//! Congruence hypotheses grow with the square of the reads of one array, so a query with more
//! than [`MAX_CONGRUENCES`] gets none, and probably no proof.

use super::{Certificate, reference_script, run};
use crate::{Sexp, sexp};
use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

/// The most congruence hypotheses a theorem gets.
pub const MAX_CONGRUENCES: usize = 20_000;

/// Axioms a checked theorem may depend on besides `bv_decide`'s native ones.
const STANDARD_AXIOMS: &[&str] =
    &["propext", "Classical.choice", "Quot.sound", "Lean.ofReduceBool", "Lean.trustCompiler"];

/// A Lean 4 toolchain, ready to certify verdicts.
#[derive(Clone, Debug)]
pub struct Lean {
    /// The `lean` binary, or elan's proxy for it.
    pub binary: PathBuf,
    /// The directory Lean runs in, such as a project whose `lean-toolchain` picks elan's
    /// toolchain.
    pub project: Option<PathBuf>,
}

impl Lean {
    /// The toolchain at `binary`, run in `project`; an error when it does not run.
    pub fn new(binary: PathBuf, project: Option<PathBuf>) -> Result<Self, String> {
        let lean = Self { binary, project };
        let mut command = lean.command();
        command.arg("--version");
        match run(command, Duration::from_secs(60)) {
            Some((true, _)) => Ok(lean),
            _ => Err(format!("cannot run {}", lean.binary.display())),
        }
    }

    /// Proves `query` in Lean with `bv_decide`, for `timeout`. Writes the theorem to
    /// `stem.lean`, which `lean stem.lean` checks again.
    pub fn certify(&self, query: &str, stem: &Path, timeout: Duration) -> Certificate {
        let theorem =
            match reference_script(query).and_then(|reference| theorem(&reference, timeout)) {
                Ok(theorem) => theorem,
                Err(reason) => return Certificate::Rejected(format!("lean: {reason}")),
            };
        let mut path = stem.as_os_str().to_owned();
        path.push(".lean");
        let path = std::path::absolute(PathBuf::from(path)).unwrap_or_default();
        if let Err(error) = fs::write(&path, theorem) {
            return Certificate::Rejected(format!("cannot write {}: {error}", path.display()));
        }
        let mut command = self.command();
        command.arg(&path);
        let Some((success, output)) = run(command, timeout) else {
            return Certificate::Rejected("lean timed out".into());
        };
        if !success {
            // Lean prints nothing when it is killed, such as for running out of memory.
            let error = output
                .lines()
                .find(|line| line.contains("error"))
                .or_else(|| output.lines().rfind(|line| !line.trim().is_empty()))
                .unwrap_or("exited with an error and no output");
            return Certificate::Rejected(format!("lean: {error}"));
        }
        match axioms(&output) {
            Ok(()) => Certificate::CheckedLean,
            Err(reason) => Certificate::Rejected(format!("lean: {reason}")),
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.binary);
        if let Some(project) = &self.project {
            command.current_dir(project);
        }
        command
    }
}

/// Whether the `#print axioms query` line of Lean's output names only trusted axioms.
fn axioms(output: &str) -> Result<(), String> {
    if output.contains("'query' does not depend on any axioms") {
        return Ok(());
    }
    let marker = "'query' depends on axioms: [";
    let Some(start) = output.find(marker) else {
        return Err("no axioms report".into());
    };
    let rest = &output[start + marker.len()..];
    let list = &rest[..rest.find(']').ok_or("unterminated axioms report")?];
    for axiom in list.split(',').map(str::trim) {
        if !STANDARD_AXIOMS.contains(&axiom) && !axiom.starts_with("query._native.bv_decide.") {
            return Err(format!("depends on `{axiom}`"));
        }
    }
    Ok(())
}

/// The Lean theorem that `reference`'s assertions are contradictory, with a `bv_decide` proof
/// whose SAT solver may run for `timeout`. `reference` has no `define-fun`s, as
/// [`reference_script`] writes it.
pub fn theorem(reference: &str, timeout: Duration) -> Result<String, String> {
    let mut translator = Translator::default();
    let mut binders = Vec::new();
    let mut assertions = Vec::new();
    for command in sexp::parse(reference) {
        let command = command.map_err(|error| format!("query: {error}"))?;
        match (command.head(), command.as_list().unwrap_or_default()) {
            (Some("assert"), [_, term]) => assertions.push(term.clone()),
            (Some("set-logic" | "set-info" | "set-option" | "check-sat" | "exit"), _) => {}
            _ => binders.push(translator.declare(&command)?),
        }
    }

    // Arrays a top-level `(= m t)` defines: reads through them reach the arrays `t` is built
    // from once Lean substitutes `t` for `m`.
    let mut substitutions = Vec::new();
    for (index, assertion) in assertions.iter().enumerate() {
        if let Some([Sexp::Atom(eq), Sexp::Atom(name), term]) = assertion.as_list()
            && eq == "="
            && let Some((arguments, Sort::Array(..))) = translator.symbols.get(name)
            && arguments.is_empty()
            && !translator.definitions.contains_key(name)
            && !mentions(term, name)
        {
            translator.definitions.insert(name.clone(), term.clone());
            substitutions.push(index);
        }
    }
    let mut hypotheses = Vec::with_capacity(assertions.len());
    for assertion in &assertions {
        let (text, sort) = translator.term(assertion, &HashMap::new())?;
        if sort != Sort::Bool {
            return Err(format!("assertion of sort {sort:?}"));
        }
        hypotheses.push(text);
    }

    // Array equalities the substitutions leave, such as a join's `(or (and (= m9 m6) ...) ...)`,
    // join the arrays they relate into classes, whose members must agree wherever any member is
    // read.
    let mut equalities = Vec::new();
    for (index, assertion) in assertions.iter().enumerate() {
        if !substitutions.contains(&index) {
            translator.positive_equalities(assertion, &mut equalities);
        }
    }
    let mut class: HashMap<String, usize> = HashMap::new();
    let mut members: Vec<Vec<String>> = Vec::new();
    for (left, right) in &equalities {
        let (Some(left), Some(right)) = (translator.roots(left), translator.roots(right)) else {
            continue;
        };
        let mut merged = Vec::new();
        for root in left.into_iter().chain(right) {
            match class.get(&root) {
                Some(&id) => {
                    if !merged.contains(&id) {
                        merged.push(id);
                    }
                }
                None => {
                    let id = members.len();
                    members.push(vec![root.clone()]);
                    class.insert(root, id);
                    merged.push(id);
                }
            }
        }
        let target = merged[0];
        for &id in &merged[1..] {
            for root in std::mem::take(&mut members[id]) {
                class.insert(root.clone(), target);
                members[target].push(root);
            }
        }
    }
    let mut indices: HashMap<usize, Vec<String>> = HashMap::new();
    for (id, roots) in members.iter().enumerate().filter(|(_, roots)| !roots.is_empty()) {
        let mut all = Vec::new();
        for (function, applications) in &translator.reads {
            if roots.contains(function) {
                for arguments in applications {
                    if !all.contains(&arguments[0]) {
                        all.push(arguments[0].clone());
                    }
                }
            }
        }
        for root in roots {
            for index in &all {
                translator.record(root, vec![index.clone()]);
            }
        }
        indices.insert(id, all);
    }
    translator.classes = class.into_iter().map(|(root, id)| (root, indices[&id].clone())).collect();
    let mut weakenings = Vec::new();
    for (index, assertion) in assertions.iter().enumerate() {
        if !substitutions.contains(&index)
            && let Some((weaker, proof)) = translator.weakened(assertion)?
        {
            weakenings.push(format!("have v{index} : {weaker} := {proof} h{index}"));
            weakenings.push(format!("clear h{index}"));
        }
    }

    let mut congruences = Vec::new();
    for (function, applications) in &translator.reads {
        for (i, first) in applications.iter().enumerate() {
            for second in &applications[i + 1..] {
                // Reads at different offsets from one base never meet, so their congruence is
                // useless to the proof, and leaving it out keeps big theorems small.
                if first.iter().zip(second).any(|(a, b)| distinct_offsets(a, b)) {
                    continue;
                }
                congruences.push(congruence(function, first, second, congruences.len()));
            }
        }
    }
    if congruences.len() > MAX_CONGRUENCES {
        congruences.clear();
    }

    let mut out = String::from(
        "import Std.Tactic.BVDecide

def store {a b : Nat} (m : BitVec a → BitVec b) (i : BitVec a) (v : BitVec b) :
    BitVec a → BitVec b :=
  fun j => if j = i then v else m j

set_option maxHeartbeats 0
set_option maxRecDepth 100000
set_option linter.unusedVariables false
",
    );
    out.push('\n');
    let _ = writeln!(out, "theorem query {}", binders.join(" "));
    for (index, hypothesis) in hypotheses.iter().enumerate() {
        let _ = writeln!(out, "    (h{index} : {hypothesis})");
    }
    out.push_str("    : False := by\n");
    for line in weakenings.iter().chain(&congruences) {
        let _ = writeln!(out, "  {line}");
    }
    for index in substitutions {
        let _ = writeln!(out, "  try subst h{index}");
    }
    // The SAT solver's own limit, which is 10 s by default.
    let _ = writeln!(out, "  try simp only [store] at *");
    let _ = writeln!(out, "  all_goals bv_decide (timeout := {})", timeout.as_secs().max(1));
    out.push_str("\n#print axioms query\n");
    Ok(out)
}

/// `have ackN : a₁ = b₁ → … → f a₁ … = f b₁ …`, proved by congruence.
fn congruence(function: &str, first: &[String], second: &[String], index: usize) -> String {
    let function = ident(function).unwrap_or_default();
    let premises: String = first.iter().zip(second).map(|(a, b)| format!("{a} = {b} → ")).collect();
    let names: Vec<String> = (0..first.len()).map(|i| format!("e{i}")).collect();
    let mut proof = format!("congrArg {function} e0");
    for name in &names[1..] {
        proof = format!("congr ({proof}) {name}");
    }
    format!(
        "have ack{index} : {premises}{function} {} = {function} {} := fun {} => {proof}",
        first.join(" "),
        second.join(" "),
        names.join(" ")
    )
}

/// An SMT-LIB sort the translation supports.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Sort {
    Bool,
    BitVec(u32),
    /// An array from bit-vectors of the first width to the second.
    Array(u32, u32),
}

impl Sort {
    fn parse(sort: &Sexp) -> Result<Self, String> {
        let width = |sort: &Sexp| match sort.as_list() {
            Some([Sexp::Atom(underscore), Sexp::Atom(name), Sexp::Atom(width)])
                if underscore == "_" && name == "BitVec" =>
            {
                width.parse::<u32>().ok().filter(|&width| width > 0)
            }
            _ => None,
        };
        if sort.as_atom() == Some("Bool") {
            return Ok(Self::Bool);
        }
        if let Some(width) = width(sort) {
            return Ok(Self::BitVec(width));
        }
        if let Some([Sexp::Atom(array), index, element]) = sort.as_list()
            && array == "Array"
            && let (Some(index), Some(element)) = (width(index), width(element))
        {
            return Ok(Self::Array(index, element));
        }
        Err(format!("unsupported sort {sort}"))
    }

    fn lean(&self) -> String {
        match self {
            Self::Bool => "Bool".into(),
            Self::BitVec(width) => format!("BitVec {width}"),
            Self::Array(index, element) => format!("(BitVec {index} → BitVec {element})"),
        }
    }
}

/// How a name in a term is bound.
#[derive(Clone, Debug)]
enum Local {
    /// A `let`, as a Lean `let` of this sort: a `Bool` one is a proposition.
    Let(Sort),
}

#[derive(Default)]
struct Translator {
    /// Declared constants and functions: argument sorts, then the result's.
    symbols: HashMap<String, (Vec<Sort>, Sort)>,
    /// Arrays defined by top-level equations, by name.
    definitions: HashMap<String, Sexp>,
    /// The argument lists each array or function is applied to, in Lean, in order of appearance.
    reads: Vec<(String, Vec<Vec<String>>)>,
    seen: HashSet<(String, Vec<String>)>,
    /// The indices at which each array an array equality relates is read, with its class's.
    classes: HashMap<String, Vec<String>>,
    /// Names for the proofs of [`Self::weakened`].
    fresh: usize,
}

impl Translator {
    /// `term` in Lean with its sort; `Bool` terms are propositions.
    fn term(
        &mut self,
        term: &Sexp,
        locals: &HashMap<String, Local>,
    ) -> Result<(String, Sort), String> {
        let items = match term {
            Sexp::Atom(atom) => return self.atom(atom, locals),
            Sexp::List(items) => items.as_slice(),
        };
        let unsupported = || format!("unsupported term {term}");
        let (head, arguments) = items.split_first().ok_or_else(unsupported)?;
        if let Sexp::List(indexed) = head {
            let [_, Sexp::Atom(name), indices @ ..] = indexed.as_slice() else {
                return Err(unsupported());
            };
            let indices = indices
                .iter()
                .map(|index| index.as_atom().and_then(|index| index.parse::<u32>().ok()))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(unsupported)?;
            let [argument] = arguments else { return Err(unsupported()) };
            let (value, sort) = self.term(argument, locals)?;
            let Sort::BitVec(width) = sort else { return Err(unsupported()) };
            return match (name.as_str(), indices.as_slice()) {
                ("extract", &[high, low]) if low <= high && high < width => Ok((
                    format!("(BitVec.extractLsb' {low} {} {value})", high - low + 1),
                    Sort::BitVec(high - low + 1),
                )),
                ("zero_extend", &[extra]) => Ok((
                    format!("(BitVec.setWidth {} {value})", width + extra),
                    Sort::BitVec(width + extra),
                )),
                ("sign_extend", &[extra]) => Ok((
                    format!("(BitVec.signExtend {} {value})", width + extra),
                    Sort::BitVec(width + extra),
                )),
                _ => Err(unsupported()),
            };
        }
        let head = head.as_atom().unwrap_or_default();
        if head == "let" {
            return self.let_term(arguments, locals);
        }
        if let Some([underscore, Sexp::Atom(value)]) = items.get(..2)
            && underscore.as_atom() == Some("_")
            && let Some(digits) = value.strip_prefix("bv")
            && let [_, _, Sexp::Atom(width)] = items
            && let (Ok(digits), Ok(width)) = (digits.parse::<u128>(), width.parse::<u32>())
        {
            return Ok((format!("({digits}#{width})"), Sort::BitVec(width)));
        }

        let mut values = Vec::with_capacity(arguments.len());
        let mut sorts = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let (value, sort) = self.term(argument, locals)?;
            values.push(value);
            sorts.push(sort);
        }
        let same = |sorts: &[Sort]| sorts.windows(2).all(|pair| pair[0] == pair[1]);
        let bitvec = |sorts: &[Sort]| match sorts.first() {
            Some(&Sort::BitVec(width)) if same(sorts) => Some(width),
            _ => None,
        };
        let fold = |op: &str| {
            values
                .iter()
                .skip(1)
                .fold(values[0].clone(), |acc, value| format!("({acc} {op} {value})"))
        };
        let all_bool = !sorts.is_empty() && sorts.iter().all(|sort| *sort == Sort::Bool);
        let bool_term = |text: String| Ok((text, Sort::Bool));
        let n = values.len();
        match head {
            "not" if n == 1 && all_bool => bool_term(format!("(¬{})", values[0])),
            // Right-nested, as Lean's own `∧` and `∨`, which [`Self::weakened`]'s proofs follow.
            "and" if all_bool => bool_term(right_fold(&values, "∧")),
            "or" if all_bool => bool_term(right_fold(&values, "∨")),
            "and" | "or" if n == 0 => {
                bool_term((if head == "and" { "True" } else { "False" }).into())
            }
            "=>" if n >= 2 && all_bool => bool_term(
                values
                    .iter()
                    .rev()
                    .skip(1)
                    .fold(values[n - 1].clone(), |acc, value| format!("({value} → {acc})")),
            ),
            "xor" if n >= 2 && all_bool => bool_term(
                values
                    .iter()
                    .skip(1)
                    .fold(values[0].clone(), |acc, value| format!("(¬({acc} ↔ {value}))")),
            ),
            "=" | "distinct" if n >= 2 && same(&sorts) => {
                let op = if sorts[0] == Sort::Bool { "↔" } else { "=" };
                let pairs: Vec<String> = if head == "=" {
                    values
                        .windows(2)
                        .map(|pair| format!("({} {op} {})", pair[0], pair[1]))
                        .collect()
                } else {
                    let mut pairs = Vec::new();
                    for (i, a) in values.iter().enumerate() {
                        for b in &values[i + 1..] {
                            pairs.push(format!("(¬({a} {op} {b}))"));
                        }
                    }
                    pairs
                };
                bool_term(if pairs.len() == 1 {
                    pairs[0].clone()
                } else {
                    format!("({})", pairs.join(" ∧ "))
                })
            }
            "ite" if n == 3 && sorts[0] == Sort::Bool && sorts[1] == sorts[2] => Ok((
                format!("(if {} then {} else {})", values[0], values[1], values[2]),
                sorts[1].clone(),
            )),
            "select"
                if n == 2
                    && matches!(sorts[0], Sort::Array(index, _) if sorts[1] == Sort::BitVec(index)) =>
            {
                let Sort::Array(_, element) = sorts[0] else { unreachable!() };
                self.read_through(&arguments[0], &values[1], locals);
                Ok((format!("({} {})", values[0], values[1]), Sort::BitVec(element)))
            }
            "store"
                if n == 3
                    && matches!(sorts[0], Sort::Array(index, element)
                        if sorts[1] == Sort::BitVec(index) && sorts[2] == Sort::BitVec(element)) =>
            {
                Ok((format!("(store {} {} {})", values[0], values[1], values[2]), sorts[0].clone()))
            }
            // Solvers take `concat` of more than two, as a left fold.
            "concat" if n >= 2 => {
                let mut total = 0;
                for sort in &sorts {
                    let Sort::BitVec(width) = sort else { return Err(unsupported()) };
                    total += width;
                }
                Ok((fold("++"), Sort::BitVec(total)))
            }
            _ if let Some(width) = bitvec(&sorts) => {
                let word = |text: String| Ok((text, Sort::BitVec(width)));
                let call = |function: &str| format!("({function} {} {})", values[0], values[1]);
                match (head, n) {
                    ("bvadd", 2..) => word(fold("+")),
                    ("bvsub", 2..) => word(fold("-")),
                    ("bvmul", 2..) => word(fold("*")),
                    ("bvand", 2..) => word(fold("&&&")),
                    ("bvor", 2..) => word(fold("|||")),
                    ("bvxor", 2..) => word(fold("^^^")),
                    ("bvnot", 1) => word(format!("(~~~{})", values[0])),
                    ("bvneg", 1) => word(format!("(-{})", values[0])),
                    ("bvshl", 2) => word(format!("({} <<< {})", values[0], values[1])),
                    ("bvlshr", 2) => word(format!("({} >>> {})", values[0], values[1])),
                    ("bvashr", 2) => word(call("BitVec.sshiftRight'")),
                    ("bvudiv", 2) => word(call("BitVec.smtUDiv")),
                    ("bvurem", 2) => word(call("BitVec.umod")),
                    ("bvsdiv", 2) => word(call("BitVec.smtSDiv")),
                    ("bvsrem", 2) => word(call("BitVec.srem")),
                    ("bvsmod", 2) => word(call("BitVec.smod")),
                    ("bvult", 2) => bool_term(format!("({} < {})", values[0], values[1])),
                    ("bvule", 2) => bool_term(format!("({} ≤ {})", values[0], values[1])),
                    ("bvugt", 2) => bool_term(format!("({} > {})", values[0], values[1])),
                    ("bvuge", 2) => bool_term(format!("({} ≥ {})", values[0], values[1])),
                    ("bvslt", 2) => bool_term(format!("({} = true)", call("BitVec.slt"))),
                    ("bvsle", 2) => bool_term(format!("({} = true)", call("BitVec.sle"))),
                    ("bvsgt", 2) => {
                        bool_term(format!("((BitVec.slt {} {}) = true)", values[1], values[0]))
                    }
                    ("bvsge", 2) => {
                        bool_term(format!("((BitVec.sle {} {}) = true)", values[1], values[0]))
                    }
                    _ => self.application(head, &values, &sorts, locals).ok_or_else(unsupported),
                }
            }
            _ => self.application(head, &values, &sorts, locals).ok_or_else(unsupported),
        }
    }

    /// A constant, literal or bound name.
    fn atom(
        &mut self,
        atom: &str,
        locals: &HashMap<String, Local>,
    ) -> Result<(String, Sort), String> {
        if let Some(Local::Let(sort)) = locals.get(atom) {
            return Ok((ident(atom)?, sort.clone()));
        }
        match atom {
            "true" => return Ok(("True".into(), Sort::Bool)),
            "false" => return Ok(("False".into(), Sort::Bool)),
            _ => {}
        }
        if let Some(hex) = atom.strip_prefix("#x")
            && !hex.is_empty()
            && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            let width = 4 * hex.len() as u32;
            return Ok((format!("(0x{hex}#{width})"), Sort::BitVec(width)));
        }
        if let Some(bits) = atom.strip_prefix("#b")
            && !bits.is_empty()
            && bits.bytes().all(|byte| matches!(byte, b'0' | b'1'))
        {
            return Ok((format!("(0b{bits}#{})", bits.len()), Sort::BitVec(bits.len() as u32)));
        }
        match self.symbols.get(atom) {
            Some((arguments, sort)) if arguments.is_empty() => {
                let name = ident(atom)?;
                let text = if *sort == Sort::Bool { format!("({name} = true)") } else { name };
                Ok((text, sort.clone()))
            }
            _ => Err(format!("unknown symbol {atom}")),
        }
    }

    /// An application of a declared function, which [`Self::reads`] records.
    fn application(
        &mut self,
        head: &str,
        values: &[String],
        sorts: &[Sort],
        locals: &HashMap<String, Local>,
    ) -> Option<(String, Sort)> {
        if locals.contains_key(head) {
            return None;
        }
        let (arguments, result) = self.symbols.get(head)?;
        if arguments.is_empty() || arguments.as_slice() != sorts {
            return None;
        }
        let result = result.clone();
        let call = format!("({} {})", ident(head).ok()?, values.join(" "));
        // Within a `let`, arguments may name its variables, which a congruence outside it
        // cannot.
        if locals.is_empty() {
            self.record(head, values.to_vec());
        }
        Some(if result == Sort::Bool {
            (format!("({call} = true)"), result)
        } else {
            (call, result)
        })
    }

    /// Records a read at `index` of the array `array` builds on: through its stores and the
    /// arrays it is defined as, down to a declared array.
    fn read_through(&mut self, array: &Sexp, index: &str, locals: &HashMap<String, Local>) {
        if !locals.is_empty() {
            return;
        }
        let mut array = array.clone();
        let mut followed = HashSet::new();
        loop {
            match &array {
                Sexp::List(items) if items.first().and_then(Sexp::as_atom) == Some("store") => {
                    array = items[1].clone();
                }
                Sexp::Atom(name) if self.definitions.contains_key(name) => {
                    if !followed.insert(name.clone()) {
                        return;
                    }
                    array = self.definitions[name].clone();
                }
                Sexp::Atom(name) if self.symbols.contains_key(name) => {
                    let name = name.clone();
                    self.record(&name, vec![index.to_owned()]);
                    return;
                }
                // `ite`s over arrays and the like: the proof gets no congruence for them.
                _ => return,
            }
        }
    }

    /// The array equalities `term` asserts in positive positions, under `and` and `or`.
    fn positive_equalities(&mut self, term: &Sexp, out: &mut Vec<(Sexp, Sexp)>) {
        match term.as_list() {
            Some([Sexp::Atom(op), rest @ ..]) if op == "and" || op == "or" => {
                for term in rest {
                    self.positive_equalities(term, out);
                }
            }
            Some([Sexp::Atom(op), left, right])
                if op == "="
                    && matches!(self.term(left, &HashMap::new()), Ok((_, Sort::Array(..)))) =>
            {
                out.push((left.clone(), right.clone()));
            }
            _ => {}
        }
    }

    /// The declared arrays `array` reads fall through to, past its stores and definitions;
    /// `None` when it is built some other way, such as with `ite`.
    fn roots(&self, array: &Sexp) -> Option<Vec<String>> {
        let mut array = array;
        let mut followed = HashSet::new();
        loop {
            match array {
                Sexp::List(items) if items.first().and_then(Sexp::as_atom) == Some("store") => {
                    array = items.get(1)?;
                }
                Sexp::Atom(name) if self.definitions.contains_key(name) => {
                    if !followed.insert(name.clone()) {
                        return None;
                    }
                    array = &self.definitions[name];
                }
                Sexp::Atom(name) if self.symbols.contains_key(name) => {
                    return Some(vec![name.clone()]);
                }
                _ => return None,
            }
        }
    }

    /// For an assertion with array equalities in positive positions: a weaker proposition, with
    /// each equality `a = b` replaced by `a i = b i` at every index `i` its class is read at,
    /// and a proof of the weaker one from the assertion. `bv_decide` cannot use a function
    /// equality, but the reads it implies are bit-vector equalities; Lean checks the weakening,
    /// and proving `False` from a weaker hypothesis proves it from the assertion.
    fn weakened(&mut self, term: &Sexp) -> Result<Option<(String, String)>, String> {
        let no_locals = HashMap::new();
        match term.as_list() {
            Some([Sexp::Atom(op), rest @ ..])
                if (op == "and" || op == "or") && !rest.is_empty() =>
            {
                let mut weaker = Vec::with_capacity(rest.len());
                let mut proofs = Vec::with_capacity(rest.len());
                let mut changed = false;
                for term in rest {
                    match self.weakened(term)? {
                        Some((text, proof)) => {
                            changed = true;
                            weaker.push(text);
                            proofs.push(proof);
                        }
                        None => {
                            weaker.push(self.term(term, &no_locals)?.0);
                            proofs.push("id".into());
                        }
                    }
                }
                if !changed {
                    return Ok(None);
                }
                if rest.len() == 1 {
                    return Ok(Some((weaker.remove(0), proofs.remove(0))));
                }
                let proof = if op == "and" {
                    let names: Vec<String> = proofs
                        .iter()
                        .map(|_| {
                            self.fresh += 1;
                            format!("w{}", self.fresh)
                        })
                        .collect();
                    let applied: Vec<String> =
                        proofs.iter().zip(&names).map(|(p, n)| format!("{p} {n}")).collect();
                    format!("(fun ⟨{}⟩ => ⟨{}⟩)", names.join(", "), applied.join(", "))
                } else {
                    let last = proofs.pop().unwrap_or_default();
                    proofs.iter().rev().fold(last, |acc, p| format!("(Or.imp {p} {acc})"))
                };
                let op = if op == "and" { "∧" } else { "∨" };
                Ok(Some((right_fold(&weaker, op), proof)))
            }
            Some([Sexp::Atom(op), left, right]) if op == "=" => {
                let (Ok((a, Sort::Array(..))), Ok((b, _))) =
                    (self.term(left, &no_locals), self.term(right, &no_locals))
                else {
                    return Ok(None);
                };
                let Some(roots) = self.roots(left) else { return Ok(None) };
                if self.roots(right).is_none() {
                    return Ok(None);
                }
                let indices = self.classes.get(&roots[0]).cloned().unwrap_or_default();
                if indices.is_empty() {
                    return Ok(Some(("True".into(), "(fun _ => trivial)".into())));
                }
                let reads: Vec<String> =
                    indices.iter().map(|i| format!("(({a} {i}) = ({b} {i}))")).collect();
                let proofs: Vec<String> =
                    indices.iter().map(|i| format!("congrFun e {i}")).collect();
                Ok(Some((
                    right_fold(&reads, "∧"),
                    if proofs.len() == 1 {
                        format!("(fun e => {})", proofs[0])
                    } else {
                        format!("(fun e => ⟨{}⟩)", proofs.join(", "))
                    },
                )))
            }
            _ => Ok(None),
        }
    }

    fn record(&mut self, function: &str, arguments: Vec<String>) {
        if !self.seen.insert((function.to_owned(), arguments.clone())) {
            return;
        }
        match self.reads.iter_mut().find(|(name, _)| name == function) {
            Some((_, applications)) => applications.push(arguments),
            None => self.reads.push((function.to_owned(), vec![arguments])),
        }
    }

    /// SMT-LIB's parallel `let` as nested Lean `let`s, when no value names a variable the same
    /// `let` binds, which sequential `let`s would capture.
    fn let_term(
        &mut self,
        arguments: &[Sexp],
        locals: &HashMap<String, Local>,
    ) -> Result<(String, Sort), String> {
        let [Sexp::List(bindings), body] = arguments else {
            return Err("malformed let".into());
        };
        let mut inner = locals.clone();
        let mut text = String::from("(");
        let names: Vec<&str> =
            bindings.iter().filter_map(|binding| binding.as_list()?.first()?.as_atom()).collect();
        for binding in bindings {
            let Some([Sexp::Atom(name), value]) = binding.as_list() else {
                return Err("malformed let".into());
            };
            if names.iter().any(|bound| mentions(value, bound)) {
                return Err("let value names a variable of its let".into());
            }
            let (value, sort) = self.term(value, locals)?;
            let ty = if sort == Sort::Bool { "Prop".into() } else { sort.lean() };
            let _ = write!(text, "let {} : {ty} := {value}; ", ident(name)?);
            inner.insert(name.clone(), Local::Let(sort));
        }
        let (body, sort) = self.term(body, &inner)?;
        text.push_str(&body);
        text.push(')');
        Ok((text, sort))
    }

    /// Records the symbol a `declare-const` or `declare-fun` command declares, and returns its
    /// Lean binder.
    fn declare(&mut self, command: &Sexp) -> Result<String, String> {
        let items = command.as_list().unwrap_or_default();
        match (command.head(), items) {
            (Some("declare-const"), [_, Sexp::Atom(name), sort]) => {
                let sort = Sort::parse(sort)?;
                let binder = format!("({} : {})", ident(name)?, sort.lean());
                self.symbols.insert(name.clone(), (Vec::new(), sort));
                Ok(binder)
            }
            (Some("declare-fun"), [_, Sexp::Atom(name), Sexp::List(arguments), result]) => {
                if arguments.is_empty() {
                    let sort = Sort::parse(result)?;
                    let binder = format!("({} : {})", ident(name)?, sort.lean());
                    self.symbols.insert(name.clone(), (Vec::new(), sort));
                    return Ok(binder);
                }
                let arguments = arguments.iter().map(Sort::parse).collect::<Result<Vec<_>, _>>()?;
                let result = Sort::parse(result)?;
                if arguments.iter().chain([&result]).any(|sort| matches!(sort, Sort::Array(..)))
                    || arguments.contains(&Sort::Bool)
                {
                    return Err(format!("function sort in {command}"));
                }
                let mut ty: Vec<String> = arguments.iter().map(Sort::lean).collect();
                ty.push(result.lean());
                let binder = format!("({} : {})", ident(name)?, ty.join(" → "));
                self.symbols.insert(name.clone(), (arguments, result));
                Ok(binder)
            }
            _ => Err(format!("unsupported command {command}")),
        }
    }
}

/// Whether the Lean bit-vectors `a` and `b` are syntactically one base plus different
/// literal offsets, such as `(x + (0x01#8))` and `x`, or different literals, so they always
/// differ.
fn distinct_offsets(a: &str, b: &str) -> bool {
    // The base and the literal offset's digits without leading zeros.
    fn literal(term: &str) -> Option<&str> {
        let digits = term.strip_prefix("(0x")?.strip_suffix(')')?.split_once('#')?.0;
        digits.bytes().all(|b| b.is_ascii_hexdigit()).then(|| digits.trim_start_matches('0'))
    }
    fn split(term: &str) -> (&str, &str) {
        if let Some(digits) = literal(term) {
            return ("", digits);
        }
        if let Some(inner) = term.strip_prefix('(').and_then(|t| t.strip_suffix(')'))
            && let Some((base, offset)) = inner.rsplit_once(" + ")
            && let Some(digits) = literal(offset)
        {
            return (base, digits);
        }
        (term, "")
    }
    let (a, b) = (split(a), split(b));
    a.0 == b.0 && a.1 != b.1
}

/// `values` joined by the right-associative `op`, parenthesized.
fn right_fold(values: &[String], op: &str) -> String {
    match values {
        [] => String::new(),
        [value] => value.clone(),
        [first, rest @ ..] => format!("({first} {op} {})", right_fold(rest, op)),
    }
}

/// Whether `term` contains the atom `name`.
fn mentions(term: &Sexp, name: &str) -> bool {
    match term {
        Sexp::Atom(atom) => atom == name,
        Sexp::List(items) => items.iter().any(|item| mentions(item, name)),
    }
}

/// An SMT-LIB symbol as a Lean identifier: `x` becomes `«$x»`, quoted so that keywords and symbol
/// characters stay names, and with a `$` no name of the theorem's own (`h0`, `store`) or of
/// Lean's (`True`, `BitVec`) starts with, so a symbol never shadows one.
pub fn ident(name: &str) -> Result<String, String> {
    let name = name.strip_prefix('|').and_then(|name| name.strip_suffix('|')).unwrap_or(name);
    if name.is_empty() || name.contains(['«', '»']) {
        return Err(format!("symbol {name} has no Lean name"));
    }
    Ok(format!("«${name}»"))
}

/// The SMT-LIB term `term` in Lean, with the symbols it names declared by the `declare-const` and
/// `declare-fun` commands of `declarations`: a `BitVec` term, or a proposition for a `Bool` one.
/// Each symbol becomes its [`ident`], which the caller binds. This is the translation [`theorem`]
/// gives each assertion, for stating facts about single terms, such as that the term an encoder
/// gives an opcode equals the opcode's Lean definition.
pub fn word_term(declarations: &str, term: &str) -> Result<String, String> {
    let mut translator = Translator::default();
    for command in sexp::parse(declarations) {
        let command = command.map_err(|error| format!("declarations: {error}"))?;
        translator.declare(&command)?;
    }
    let mut terms = sexp::parse(term);
    let (Some(parsed), None) = (terms.next(), terms.next()) else {
        return Err(format!("expected one term, found {term}"));
    };
    let parsed = parsed.map_err(|error| format!("term: {error}"))?;
    Ok(translator.term(&parsed, &HashMap::new())?.0)
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn offsets() {
        let x = "«$x»";
        assert!(distinct_offsets(x, "(«$x» + (0x01#8))"));
        assert!(distinct_offsets("(«$x» + (0x02#8))", "(«$x» + (0x01#8))"));
        assert!(distinct_offsets("(0x02#8)", "(0x01#8)"));
        assert!(!distinct_offsets(x, "(«$x» + (0x00#8))"));
        assert!(!distinct_offsets(x, "(«$y» + (0x01#8))"));
        assert!(!distinct_offsets(x, "(0x01#8)"));
        assert!(!distinct_offsets("((«$x» + «$y») + (0x01#8))", "(«$y» + (0x01#8))"));
    }

    #[test]
    fn translates() {
        let query = "(set-logic QF_AUFBV)
(declare-const x (_ BitVec 8))
(declare-const m (Array (_ BitVec 8) (_ BitVec 8)))
(declare-const n (Array (_ BitVec 8) (_ BitVec 8)))
(declare-const k (Array (_ BitVec 8) (_ BitVec 8)))
(declare-fun f ((_ BitVec 8) (_ BitVec 8)) (_ BitVec 8))
(define-fun read2 ((a (Array (_ BitVec 8) (_ BitVec 8))) (p (_ BitVec 8))) (_ BitVec 16) (concat (select a p) (select a (bvadd p #x01))))
(assert (= n (store m x #x2a)))
(assert (or (= k m) (and (= k n) (bvult x #x05))))
(assert (= (select k #x07) #x00))
(assert (bvult (f x #x01) (bvudiv x #x00)))
(assert (not (= ((_ zero_extend 8) (select n x)) (read2 m x))))
(assert (ite (bvslt x #x00) (distinct x (f #x01 x)) ((_ extract 3 0) x)))
(check-sat)
";
        let error = theorem(&reference_script(query).unwrap(), Duration::from_secs(5)).unwrap_err();
        assert_eq!(
            error,
            "unsupported term (ite (bvslt x #x00) (distinct x (f #x01 x)) ((_ extract 3 0) x))"
        );
        let query = query.replace("((_ extract 3 0) x)", "(= ((_ extract 3 0) x) #x0)");
        snapbox::assert_data_eq!(
            theorem(&reference_script(&query).unwrap(), Duration::from_secs(5)).unwrap(),
            snapbox::str![[r#"
import Std.Tactic.BVDecide

def store {a b : Nat} (m : BitVec a → BitVec b) (i : BitVec a) (v : BitVec b) :
    BitVec a → BitVec b :=
  fun j => if j = i then v else m j

set_option maxHeartbeats 0
set_option maxRecDepth 100000
set_option linter.unusedVariables false

theorem query («$x» : BitVec 8) («$m» : (BitVec 8 → BitVec 8)) («$n» : (BitVec 8 → BitVec 8)) («$k» : (BitVec 8 → BitVec 8)) («$f» : BitVec 8 → BitVec 8 → BitVec 8)
    (h0 : («$n» = (store «$m» «$x» (0x2a#8))))
    (h1 : ((«$k» = «$m») ∨ ((«$k» = «$n») ∧ («$x» < (0x05#8)))))
    (h2 : ((«$k» (0x07#8)) = (0x00#8)))
    (h3 : ((«$f» «$x» (0x01#8)) < (BitVec.smtUDiv «$x» (0x00#8))))
    (h4 : (¬((BitVec.setWidth 16 («$n» «$x»)) = ((«$m» «$x») ++ («$m» («$x» + (0x01#8)))))))
    (h5 : (if ((BitVec.slt «$x» (0x00#8)) = true) then (¬(«$x» = («$f» (0x01#8) «$x»))) else ((BitVec.extractLsb' 0 4 «$x») = (0x0#4))))
    : False := by
  have v1 : ((((«$k» (0x07#8)) = («$m» (0x07#8))) ∧ (((«$k» «$x») = («$m» «$x»)) ∧ ((«$k» («$x» + (0x01#8))) = («$m» («$x» + (0x01#8)))))) ∨ ((((«$k» (0x07#8)) = («$n» (0x07#8))) ∧ (((«$k» «$x») = («$n» «$x»)) ∧ ((«$k» («$x» + (0x01#8))) = («$n» («$x» + (0x01#8)))))) ∧ («$x» < (0x05#8)))) := (Or.imp (fun e => ⟨congrFun e (0x07#8), congrFun e «$x», congrFun e («$x» + (0x01#8))⟩) (fun ⟨w1, w2⟩ => ⟨(fun e => ⟨congrFun e (0x07#8), congrFun e «$x», congrFun e («$x» + (0x01#8))⟩) w1, id w2⟩)) h1
  clear h1
  have ack0 : (0x07#8) = «$x» → «$k» (0x07#8) = «$k» «$x» := fun e0 => congrArg «$k» e0
  have ack1 : (0x07#8) = («$x» + (0x01#8)) → «$k» (0x07#8) = «$k» («$x» + (0x01#8)) := fun e0 => congrArg «$k» e0
  have ack2 : «$x» = (0x01#8) → (0x01#8) = «$x» → «$f» «$x» (0x01#8) = «$f» (0x01#8) «$x» := fun e0 e1 => congr (congrArg «$f» e0) e1
  have ack3 : «$x» = (0x07#8) → «$m» «$x» = «$m» (0x07#8) := fun e0 => congrArg «$m» e0
  have ack4 : («$x» + (0x01#8)) = (0x07#8) → «$m» («$x» + (0x01#8)) = «$m» (0x07#8) := fun e0 => congrArg «$m» e0
  try subst h0
  try simp only [store] at *
  all_goals bv_decide (timeout := 5)

#print axioms query

"#]]
        );
    }

    /// Needs Lean 4, which `SOLAR_PROVE_ANALYSES_LEAN` names (and
    /// `SOLAR_PROVE_ANALYSES_LEAN_PROJECT` the directory to run it in); skips without it.
    #[test]
    fn certifies() {
        let binary = std::env::var_os("SOLAR_PROVE_ANALYSES_LEAN")
            .map_or_else(|| PathBuf::from("lean"), PathBuf::from);
        let project = std::env::var_os("SOLAR_PROVE_ANALYSES_LEAN_PROJECT").map(PathBuf::from);
        let lean = match Lean::new(binary, project) {
            Ok(lean) => lean,
            Err(reason) => {
                eprintln!("skipping: {reason}");
                return;
            }
        };
        let dir = tempfile::tempdir().unwrap();
        let timeout = Duration::from_secs(120);
        // Needs the congruence of `m`'s reads: `x = y` but the reads differ.
        let unsat = "(set-logic QF_ABV)
(declare-const x (_ BitVec 256))
(declare-const y (_ BitVec 256))
(declare-const m (Array (_ BitVec 256) (_ BitVec 8)))
(declare-const n (Array (_ BitVec 256) (_ BitVec 8)))
(define-fun bump ((a (Array (_ BitVec 256) (_ BitVec 8))) (p (_ BitVec 256))) (Array (_ BitVec 256) (_ BitVec 8)) (store a (bvadd p #x0000000000000000000000000000000000000000000000000000000000000001) #xff))
(assert (= n (bump m y)))
(assert (= x y))
(assert (not (= (select n x) (select m y))))
(check-sat)
";
        assert_eq!(
            lean.certify(unsat, &dir.path().join("unsat"), timeout),
            Certificate::CheckedLean
        );
        assert!(dir.path().join("unsat.lean").is_file());
        // Needs the array equalities under `or` at the index read: either way `k y = m y`.
        let joined = "(set-logic QF_ABV)
(declare-const x (_ BitVec 8))
(declare-const y (_ BitVec 8))
(declare-const m (Array (_ BitVec 8) (_ BitVec 8)))
(declare-const k (Array (_ BitVec 8) (_ BitVec 8)))
(assert (or (= k m) (and (= k (store m x #x01)) (bvult x y))))
(assert (not (= (select k y) (select m y))))
(check-sat)
";
        assert_eq!(
            lean.certify(joined, &dir.path().join("joined"), timeout),
            Certificate::CheckedLean
        );
        // A satisfiable query fails, as would a mistranslation that made it provable.
        let sat = unsat.replace("(assert (= x y))\n", "");
        assert!(matches!(
            lean.certify(&sat, &dir.path().join("sat"), timeout),
            Certificate::Rejected(_)
        ));
    }
    #[test]
    fn single_terms() {
        let declarations = "(declare-const a (_ BitVec 256))
(declare-fun f ((_ BitVec 256)) (_ BitVec 256))";
        assert_eq!(
            word_term(declarations, "(ite (bvult a (f a)) #x01 (_ bv0 8))").unwrap(),
            "(if («$a» < («$f» «$a»)) then (0x01#8) else (0#8))"
        );
        assert_eq!(word_term(declarations, "(bvadd a b)").unwrap_err(), "unknown symbol b");
        assert!(word_term(declarations, "a a").is_err());
        assert!(word_term("(assert true)", "a").is_err());
    }
}
