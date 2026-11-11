//! Proof certificates for the provers' `unsat` verdicts: cvc5 writes a proof in its CPC format,
//! and Ethos, cvc5's standalone proof checker, checks it against the query. A checked
//! certificate moves trust from the solver that answered to Ethos and CPC's rules, and the
//! certificate files can be checked again without any solver.
//!
//! [`Certifier::certify`] works in three steps.
//!
//! 1. It writes the query as a reference script: the query's logic, declarations and assertions,
//!    with every `define-fun` inlined at its uses, since Ethos reads a reference's definitions with
//!    parameters as constants and rejects their applications. Responses such as `get-value` go,
//!    since a proof of `unsat` has no model.
//! 2. cvc5 proves the reference script and prints the proof.
//! 3. Ethos checks the proof, which names the query with `(reference ...)`, which makes it reject
//!    any assumption that is not an assertion of the reference script, so a proof of another query,
//!    or one that assumes `false`, fails. Ethos reports an empty proof as correct, so the proof's
//!    last step must also conclude `false`.
//!
//! What a certificate still trusts: Ethos, the CPC signature it includes (the path of `Cpc.eo`
//! in cvc5's `proofs/eo/cpc`), and the reference script standing for the
//! query, that is, the inlining in [`reference_script`].
//!
//! [`lean`] certifies the same reference script in Lean 4 instead, for queries cvc5 proves too
//! slowly or not at all.

use crate::{Sexp, sexp};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub mod lean;

/// cvc5 and Ethos, ready to certify verdicts.
#[derive(Clone, Debug)]
pub struct Certifier {
    /// The cvc5 binary.
    pub cvc5: PathBuf,
    /// The Ethos binary.
    pub ethos: PathBuf,
    /// The CPC signature's top file, `Cpc.eo`.
    pub signature: PathBuf,
}

/// What [`Certifier::certify`] found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Certificate {
    /// Ethos checked the proof against the query.
    Checked,
    /// Lean checked a `bv_decide` proof of the query's translation (see [`lean`]).
    CheckedLean,
    /// cvc5 gave no proof: its answer, such as `sat`, `unknown` or `timeout`.
    NoProof(&'static str),
    /// The query or the proof is not one Ethos accepts, and why.
    Rejected(String),
}

impl Certificate {
    /// The certificate's name in reports, with its reason when it has one.
    pub fn describe(&self) -> String {
        match self {
            Self::Checked => "checked".into(),
            Self::CheckedLean => "checked-lean".into(),
            Self::NoProof(answer) => format!("no-proof {answer}"),
            Self::Rejected(reason) => format!("rejected {reason}"),
        }
    }
}

impl Certifier {
    /// The certifier with these binaries and signature; an error says which one is missing.
    pub fn new(cvc5: PathBuf, ethos: PathBuf, signature: PathBuf) -> Result<Self, String> {
        for (binary, flag) in [(&cvc5, "--version"), (&ethos, "--help")] {
            let runs = Command::new(binary)
                .arg(flag)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok();
            if !runs {
                return Err(format!("cannot run {}", binary.display()));
            }
        }
        if !signature.is_file() {
            return Err(format!("no CPC signature at {}", signature.display()));
        }
        Ok(Self { cvc5, ethos, signature })
    }

    /// Proves `query` with cvc5 and checks the proof with Ethos, for `timeout` each. Writes the
    /// reference script to `stem.ref.smt2` and the proof to `stem.cpc`, which names the reference
    /// script, so `ethos stem.cpc` checks it again.
    pub fn certify(&self, query: &str, stem: &Path, timeout: Duration) -> Certificate {
        let reference = match reference_script(query) {
            Ok(reference) => reference,
            Err(reason) => return Certificate::Rejected(reason),
        };
        let with_suffix = |suffix: &str| {
            let mut name = stem.as_os_str().to_owned();
            name.push(suffix);
            PathBuf::from(name)
        };
        let (reference_path, proof_path) = (with_suffix(".ref.smt2"), with_suffix(".cpc"));
        if let Err(error) = fs::write(&reference_path, &reference) {
            return Certificate::Rejected(format!(
                "cannot write {}: {error}",
                reference_path.display()
            ));
        }

        let mut command = Command::new(&self.cvc5);
        command
            .args(["--lang=smt2", "--dump-proofs", "--proof-format-mode=cpc"])
            .arg(format!("--tlimit={}", timeout.as_millis()))
            .arg(&reference_path);
        let Some((_, output)) = run(command, timeout) else {
            return Certificate::NoProof("timeout");
        };
        let proof = match proof_of(&output) {
            Ok(proof) => proof,
            Err(certificate) => return certificate,
        };
        let text = format!(
            "(include \"{}\")\n(reference \"{}\")\n{}",
            self.signature.display(),
            std::path::absolute(&reference_path)
                .unwrap_or_else(|_| reference_path.clone())
                .display(),
            proof.iter().map(|command| format!("{command}\n")).collect::<String>()
        );
        if let Err(error) = fs::write(&proof_path, text) {
            return Certificate::Rejected(format!(
                "cannot write {}: {error}",
                proof_path.display()
            ));
        }

        let mut command = Command::new(&self.ethos);
        command.arg(&proof_path);
        let Some((_, output)) = run(command, timeout) else {
            return Certificate::Rejected("ethos timed out".into());
        };
        match output.lines().map(str::trim).rfind(|line| !line.is_empty()) {
            Some("correct") => Certificate::Checked,
            Some(_) => {
                Certificate::Rejected(format!("ethos: {}", output.trim().replace('\n', " ")))
            }
            None => Certificate::Rejected("ethos printed nothing".into()),
        }
    }
}

/// The proof commands in cvc5's output, without the declarations the reference script makes, or
/// the certificate when there is no proof to check.
fn proof_of(output: &str) -> Result<Vec<Sexp>, Certificate> {
    let mut responses = sexp::parse(output);
    let verdict = loop {
        match responses.next() {
            Some(Ok(Sexp::Atom(atom))) => match atom.as_str() {
                "unsat" => break "unsat",
                "sat" => break "sat",
                "unknown" => break "unknown",
                _ => {}
            },
            Some(Ok(_)) => {}
            Some(Err(error)) => return Err(Certificate::Rejected(format!("cvc5 output: {error}"))),
            // cvc5 prints no verdict when it stops at its time limit.
            None => return Err(Certificate::NoProof("unknown")),
        }
    };
    if verdict != "unsat" {
        return Err(Certificate::NoProof(verdict));
    }
    let Some(Ok(Sexp::List(commands))) = responses.next() else {
        // cvc5 can reach its time limit while it prints the proof.
        return Err(Certificate::Rejected("cvc5's proof is incomplete".into()));
    };
    let proof: Vec<Sexp> = commands
        .into_iter()
        .filter(|command| !matches!(command.head(), Some("declare-const" | "declare-fun")))
        .collect();
    // Ethos accepts a proof that proves nothing, so the last step must refute the assertions.
    let last = proof.iter().rev().find(|command| command.head() == Some("step"));
    let concludes_false = last
        .and_then(Sexp::as_list)
        .is_some_and(|step| step.get(2).and_then(Sexp::as_atom) == Some("false"));
    if !concludes_false {
        return Err(Certificate::Rejected("the proof's last step does not conclude false".into()));
    }
    Ok(proof)
}

/// Runs `command` and returns whether it succeeded and what it printed, stdout then stderr, or
/// `None` when it does not start or runs past `timeout`, which a solver's own time limit does not
/// always keep.
fn run(mut command: Command, timeout: Duration) -> Option<(bool, String)> {
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().ok()?;
    let read = |pipe: Option<Box<dyn Read + Send>>| {
        thread::spawn(move || {
            let mut output = String::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_string(&mut output);
            }
            output
        })
    };
    let stdout = read(child.stdout.take().map(|pipe| Box::new(pipe) as _));
    let stderr = read(child.stderr.take().map(|pipe| Box::new(pipe) as _));
    let deadline = Instant::now() + timeout + Duration::from_secs(2);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let mut output = stdout.join().unwrap_or_default();
    output.push_str(&stderr.join().unwrap_or_default());
    Some((status.success(), output))
}

/// `query` as a script Ethos can take as a reference: its logic, declarations, assertions and
/// first `check-sat`, with every `define-fun` inlined at its uses. Commands after that
/// `check-sat` do not bear on its answer and go. A query with a command that could change which
/// assertions the answer is about, such as `push`, `pop` or `reset`, or with one this function
/// does not know, is refused.
pub fn reference_script(query: &str) -> Result<String, String> {
    let mut definitions: HashMap<String, (Vec<String>, Sexp)> = HashMap::new();
    let mut names = Names::of(query);
    let mut script = String::new();
    for command in sexp::parse(query) {
        // One spelling per symbol, so inlining can compare symbols as text.
        let command = canonical(&command.map_err(|error| format!("query: {error}"))?);
        match command.head() {
            Some("set-logic" | "declare-const" | "declare-fun" | "declare-sort") => {
                script.push_str(&format!("{command}\n"));
            }
            Some("check-sat") => {
                script.push_str(&format!("{command}\n"));
                break;
            }
            Some("assert") => {
                let items = command.as_list().unwrap_or_default();
                let [_, term] = items else { return Err(format!("malformed {command}")) };
                let term = inline(term, &definitions, &HashMap::new(), &mut names);
                script.push_str(&format!("(assert {term})\n"));
            }
            Some("define-fun") => {
                let items = command.as_list().unwrap_or_default();
                let [_, Sexp::Atom(name), Sexp::List(parameters), _, body] = items else {
                    return Err(format!("malformed {command}"));
                };
                let parameters = parameters
                    .iter()
                    .map(|parameter| parameter.head().map(str::to_owned))
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(|| format!("malformed parameters in {command}"))?;
                // Earlier definitions are inlined now, so each body is closed over the
                // declarations and its own parameters, which shadow definitions of their names.
                // The parameters take fresh names first, so a free symbol of an inlined body
                // cannot be mistaken for one.
                let mut visible = definitions.clone();
                let mut renamed = HashMap::new();
                let parameters: Vec<String> = parameters
                    .into_iter()
                    .map(|parameter| {
                        visible.remove(&parameter);
                        let fresh = names.fresh(&parameter);
                        renamed.insert(parameter, Sexp::Atom(fresh.clone()));
                        fresh
                    })
                    .collect();
                let body = inline(body, &visible, &renamed, &mut names);
                definitions.insert(name.clone(), (parameters, body));
            }
            // Responses and options: a proof of `unsat` has no model to print, and Z3's options
            // mean nothing to cvc5.
            Some("set-option" | "set-info" | "get-value" | "get-model" | "get-info" | "echo") => {}
            _ => return Err(format!("cannot certify a query with {command}")),
        }
    }
    Ok(script)
}

/// `term` with each application of a definition replaced by its body, and each variable
/// `substitution` names replaced by its value. Binders shadow both, and a binder that could
/// capture a variable of a substituted value is renamed.
fn inline(
    term: &Sexp,
    definitions: &HashMap<String, (Vec<String>, Sexp)>,
    substitution: &HashMap<String, Sexp>,
    names: &mut Names,
) -> Sexp {
    match term {
        Sexp::Atom(atom) => {
            if let Some(value) = substitution.get(atom) {
                value.clone()
            } else if let Some((parameters, body)) = definitions.get(atom)
                && parameters.is_empty()
            {
                body.clone()
            } else {
                term.clone()
            }
        }
        Sexp::List(items) => match items.as_slice() {
            [Sexp::Atom(binder), Sexp::List(bindings), body]
                if matches!(binder.as_str(), "let" | "forall" | "exists" | "lambda") =>
            {
                let mut inner = substitution.clone();
                let mut definitions = definitions.clone();
                let mut bound = Vec::with_capacity(bindings.len());
                for binding in bindings {
                    let Some([Sexp::Atom(name), value]) = binding.as_list() else {
                        bound.push(inline(binding, &definitions, substitution, names));
                        continue;
                    };
                    // A `let` value is in the outer scope; a quantifier's is a sort.
                    let value = if binder == "let" {
                        inline(value, &definitions, substitution, names)
                    } else {
                        value.clone()
                    };
                    // A binder must not capture a variable of an argument substituted under it,
                    // or a free symbol of a definition's body inlined under it, so it takes a
                    // fresh name when one could.
                    let captures = substitution.values().any(|value| mentions(value, name))
                        || definitions.values().any(|(parameters, body)| {
                            !parameters.contains(name) && mentions(body, name)
                        });
                    definitions.remove(name);
                    let name = if captures {
                        let fresh = names.fresh(name);
                        inner.insert(name.clone(), Sexp::Atom(fresh.clone()));
                        fresh
                    } else {
                        inner.remove(name);
                        name.clone()
                    };
                    bound.push(Sexp::List(vec![Sexp::Atom(name), value]));
                }
                let body = inline(body, &definitions, &inner, names);
                Sexp::List(vec![Sexp::Atom(binder.clone()), Sexp::List(bound), body])
            }
            [Sexp::Atom(head), arguments @ ..]
                if let Some((parameters, body)) = definitions.get(head)
                    && parameters.len() == arguments.len()
                    && !parameters.is_empty()
                    && !substitution.contains_key(head) =>
            {
                let values = parameters
                    .iter()
                    .cloned()
                    .zip(arguments.iter().map(|a| inline(a, definitions, substitution, names)))
                    .collect();
                inline(body, &HashMap::new(), &values, names)
            }
            _ => Sexp::List(
                items.iter().map(|item| inline(item, definitions, substitution, names)).collect(),
            ),
        },
    }
}

/// The symbols of a query, and the fresh names [`inline`] gives binders and parameters, none of
/// which the query uses.
struct Names {
    used: HashSet<String>,
    next: usize,
}

impl Names {
    /// The symbols of `query`, in their [`canonical`] spelling.
    fn of(query: &str) -> Self {
        fn collect(term: &Sexp, used: &mut HashSet<String>) {
            match term {
                Sexp::Atom(atom) => {
                    used.insert(canonical_atom(atom));
                }
                Sexp::List(items) => items.iter().for_each(|item| collect(item, used)),
            }
        }
        let mut used = HashSet::new();
        for command in sexp::parse(query).map_while(Result::ok) {
            collect(&command, &mut used);
        }
        Self { used, next: 0 }
    }

    /// A name based on `base`, a canonical symbol, that neither the query nor an earlier fresh
    /// name uses. It keeps `base`'s bars only where the new name still needs them, so it is
    /// compared in its canonical spelling.
    fn fresh(&mut self, base: &str) -> String {
        loop {
            let name = match base.strip_prefix('|').and_then(|inner| inner.strip_suffix('|')) {
                Some(inner) => canonical_atom(&format!("|{inner}!{}|", self.next)),
                None => format!("{base}!{}", self.next),
            };
            self.next += 1;
            if self.used.insert(name.clone()) {
                return name;
            }
        }
    }
}

/// `term` with each quoted symbol that does not need its bars written without them. SMT-LIB
/// reads `|x|` and `x` as one symbol, and after this they are one atom.
fn canonical(term: &Sexp) -> Sexp {
    match term {
        Sexp::Atom(atom) => Sexp::Atom(canonical_atom(atom)),
        Sexp::List(items) => Sexp::List(items.iter().map(canonical).collect()),
    }
}

/// `atom`'s canonical spelling: without bars when what they quote is a simple symbol.
fn canonical_atom(atom: &str) -> String {
    let Some(inner) = atom.strip_prefix('|').and_then(|inner| inner.strip_suffix('|')) else {
        return atom.to_owned();
    };
    const SPECIAL: &str = "~!@$%^&*_-+=<>.?/";
    // Reserved words are not simple symbols, so `|let|` keeps its bars.
    const RESERVED: &[&str] = &["_", "!", "as", "let", "exists", "forall", "match", "par"];
    let simple = inner.chars().next().is_some_and(|first| !first.is_ascii_digit())
        && inner.chars().all(|c| c.is_ascii_alphanumeric() || SPECIAL.contains(c))
        && !RESERVED.contains(&inner);
    if simple { inner.to_owned() } else { atom.to_owned() }
}

/// Whether `symbol` occurs anywhere in `term`.
fn mentions(term: &Sexp, symbol: &str) -> bool {
    match term {
        Sexp::Atom(atom) => atom == symbol,
        Sexp::List(items) => items.iter().any(|item| mentions(item, symbol)),
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn inlines_definitions() {
        let query = "(set-logic QF_BV)
(set-option :rlimit 5)
(declare-const x (_ BitVec 8))
(define-fun double ((v (_ BitVec 8))) (_ BitVec 8) (bvadd v v))
(define-fun four () (_ BitVec 8) (double #x02))
(assert (= (double x) four))
(assert (let ((v x)) (= (double v) v)))
(check-sat)
(get-value (x))
";
        snapbox::assert_data_eq!(
            reference_script(query).unwrap(),
            snapbox::str![[r#"
(set-logic QF_BV)
(declare-const x (_ BitVec 8))
(assert (= (bvadd x x) (bvadd #x02 #x02)))
(assert (let ((v x)) (= (bvadd v v) v)))
(check-sat)

"#]]
        );
    }

    #[test]
    fn inlining_avoids_capture() {
        // `(exists ((y ...)) ...)` in the body must not capture the argument's free ` y`.
        let query = "(set-logic BV)
(declare-const y (_ BitVec 8))
(define-fun bigger ((v (_ BitVec 8))) Bool (exists ((y (_ BitVec 8))) (bvult v y)))
(define-fun twice ((v (_ BitVec 8))) (_ BitVec 8) (let ((y (bvadd v v))) y))
(assert (bigger y))
(assert (= (twice y) y))
(check-sat)
";
        snapbox::assert_data_eq!(
            reference_script(query).unwrap().replace(char::is_numeric, "N"),
            snapbox::str![[r#"
(set-logic BV)
(declare-const y (_ BitVec N))
(assert (exists ((y!N (_ BitVec N))) (bvult y y!N)))
(assert (= (let ((y!N (bvadd y y))) y!N) y))
(check-sat)

"#]]
        );
    }

    #[test]
    fn refuses_scoped_queries() {
        let query = "(set-logic `F_BV)
(declare-const x (_ BitVec 8))
(push 1)
(assert (= x #x00))
(pop 1)
(assert (= x #x01))
(check-sat)
";
        assert_eq!(reference_script(query), Err("cannot certify a query with (push 1)".into()));
        // Only the first `check-sat` counts.
        let query = "(declare-const x (_ BitVec 8))\n(check-sat)\n(assert false)\n(check-sat)\n";
        assert_eq!(
            reference_script(query).unwrap(),
            "(declare-const x (_ BitVec 8))\n(check-sat)\n"
        );
    }

    #[test]
    fn parameters_shadow_definitions() {
        let query = "(set-logic `F_BV)
(define-fun v () (_ BitVec 8) #x07)
(define-fun id ((v (_ BitVec 8))) (_ BitVec 8) v)
(assert (= (id #x01) v))
(check-sat)
";
        snapbox::assert_data_eq!(
            reference_script(query).unwrap(),
            snapbox::str![[r#"
(set-logic `F_BV)
(assert (= #x01 #x07))
(check-sat)

"#]]
        );
    }

    #[test]
    fn parameters_do_not_capture() {
        // `plus_y`'s body uses the declared ` y`, not `g's` parameter ` y`.
        let query = "(set-logic `F_BV)
(declare-const y (_ BitVec 8))
(define-fun plus_y ((v (_ BitVec 8))) (_ BitVec 8) (bvadd v y))
(define-fun g ((y (_ BitVec 8))) (_ BitVec 8) (plus_y y))
(assert (= (g #x01) #x02))
(check-sat)
";
        snapbox::assert_data_eq!(
            reference_script(query).unwrap(),
            snapbox::str![[r#"
(set-logic `F_BV)
(declare-const y (_ BitVec 8))
(assert (= (bvadd #x01 y) #x02))
(check-sat)

"#]]
        );
    }

    #[test]
    fn fresh_names_are_unused() {
        // The query already uses `y!0` and `|y!1|`, so the renamed binder takes `y!2`.
        let query = "(set-logic BV)
(declare-const y (_ BitVec 8))
(declare-const y!0 (_ BitVec 8))
(declare-const |y!1| (_ BitVec 8))
(define-fun bigger ((v (_ BitVec 8))) Bool (exists ((y (_ BitVec 8))) (bvult v y)))
(assert (and (bigger y) (= y!0 |y!1|)))
(check-sat)
";
        let script = reference_script(query).unwrap();
        assert!(script.contains("(exists ((y!2 (_ BitVec 8))) (bvult y y!2))"), "{script}");
    }

    #[test]
    fn quoted_symbols_are_one_symbol() {
        // `f` returns its argument: `|y|` is the parameter `y`, not the declared constant.
        let query = "(set-logic `F_UF)
(declare-const y Bool)
(define-fun f ((y Bool)) Bool |y|)
(assert (and (not y) (f true)))
(check-sat)
";
        snapbox::assert_data_eq!(
            reference_script(query).unwrap(),
            snapbox::str![[r#"
(set-logic `F_UF)
(declare-const y Bool)
(assert (and (not y) true))
(check-sat)

"#]]
        );
        // A renamed binder that needs bars keeps them.
        let query = "(set-logic BV)
(declare-const |a b| (_ BitVec 8))
(define-fun big ((v (_ BitVec 8))) Bool (exists ((|a b| (_ BitVec 8))) (bvult v |a b|)))
(assert (big |a b|))
(check-sat)
";
        let script = reference_script(query).unwrap();
        assert!(script.contains("(exists ((|a b!"), "{script}");
        // `|let!0|` is the symbol `let!0`, which the query uses, so the binder takes `let!1`.
        let query = "(set-logic BV)
(declare-const |let| (_ BitVec 8))
(declare-const let!0 (_ BitVec 8))
(define-fun big ((v (_ BitVec 8))) Bool (exists ((|let| (_ BitVec 8))) (bvult v |let|)))
(assert (and (big |let|) (= let!0 let!0)))
(check-sat)
";
        let script = reference_script(query).unwrap();
        assert!(script.contains("(exists ((let!1 (_ BitVec 8))) (bvult |let| let!1))"), "{script}");
    }

    #[test]
    fn inlining_keeps_free_symbols() {
        // The body's free ` y` is the declared constant, not the ` let`'s ` y` at the use.
        let query = "(set-logic `F_BV)
(declare-const y (_ BitVec 8))
(define-fun above ((v (_ BitVec 8))) Bool (bvult y v))
(assert (let ((y #x00)) (above y)))
(check-sat)
";
        snapbox::assert_data_eq!(
            reference_script(query).unwrap().replace(char::is_numeric, "N"),
            snapbox::str![[r#"
(set-logic `F_BV)
(declare-const y (_ BitVec N))
(assert (let ((y!N #xNN)) (bvult y y!N)))
(check-sat)

"#]]
        );
    }

    /// Needs cvc5, Ethos and the CPC signature, which the prover's environment variables name
    /// (`SOLAR_PROVE_ANALYSES_CVC5`, `SOLAR_PROVE_ANALYSES_ETHOS`,
    /// `SOLAR_PROVE_ANALYSES_CPC_SIGNATURE`); skips without them.
    #[test]
    fn certifies() {
        let var = |name: &str, default: &str| {
            std::env::var_os(name).map_or_else(|| PathBuf::from(default), PathBuf::from)
        };
        let certifier = Certifier::new(
            var("SOLAR_PROVE_ANALYSES_CVC5", "cvc5"),
            var("SOLAR_PROVE_ANALYSES_ETHOS", "ethos"),
            var("SOLAR_PROVE_ANALYSES_CPC_SIGNATURE", ""),
        );
        let certifier = match certifier {
            Ok(certifier) => certifier,
            Err(reason) => {
                eprintln!("skipping: {reason}");
                return;
            }
        };
        let dir = tempfile::tempdir().unwrap();
        let timeout = Duration::from_secs(60);
        let unsat = "(set-logic QF_BV)
(declare-const x (_ BitVec 8))
(define-fun double ((v (_ BitVec 8))) (_ BitVec 8) (bvadd v v))
(assert (= (double x) #x01))
(check-sat)
(get-value (x))
";
        assert_eq!(
            certifier.certify(unsat, &dir.path().join("unsat"), timeout),
            Certificate::Checked
        );
        let sat =
            "(set-logic QF_BV)\n(declare-const x (_ BitVec 8))\n(assert (= x #x01))\n(check-sat)\n";
        assert_eq!(
            certifier.certify(sat, &dir.path().join("sat"), timeout),
            Certificate::NoProof("sat")
        );

        // A proof checked against another query fails: its assumption is not in the reference.
        let stem = dir.path().join("unsat");
        let other = dir.path().join("other.smt2");
        fs::write(&other, "(set-logic QF_BV)\n(declare-const x (_ BitVec 8))\n(assert (= (bvadd x x) #x02))\n(check-sat)\n").unwrap();
        let proof = fs::read_to_string(stem.with_extension("cpc")).unwrap();
        let reference = std::path::absolute(stem.with_extension("ref.smt2")).unwrap();
        let swapped = proof.replace(&reference.display().to_string(), &other.display().to_string());
        assert_ne!(swapped, proof);
        let tampered = dir.path().join("tampered.cpc");
        fs::write(&tampered, swapped).unwrap();
        let output = Command::new(&certifier.ethos).arg(&tampered).output().unwrap();
        let output = String::from_utf8_lossy(&output.stdout);
        assert_ne!(output.lines().last().map(str::trim), Some("correct"), "{output}");
    }
}
