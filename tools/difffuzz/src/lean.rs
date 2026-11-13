//! The SolidCore leg: solidity-lean's source semantics, evaluated in a test harness.
//!
//! solidity-lean's importer (`scripts/solc_ast_to_lean_source.py`, which reads the solc AST)
//! turns the source into a SolidCore `SourceUnit`; a generated Lean file then type-checks it,
//! runs the constructor from the empty state, and runs every call through the checked ABI
//! dispatcher (`CheckedContract.callCalldataAtFromWithContext?`) from the storage the previous
//! successful call left, as `SolidCore.Refinement.Source.result?` does for one call: caller
//! `0x2000`, contract `0x1000`, no value. `lake env lean` evaluates it with `#eval`. This is
//! evaluation for testing, not proof: nothing here is kernel-checked.

use crate::{
    Cli, compile,
    obs::{Obs, Status},
};
use alloy_primitives::{U256, hex};
use std::{path::Path, process::Command, time::Duration};

/// Fuel for SolidCore's interpreter; running out shows as an unsupported call.
const FUEL: u32 = 100_000;

const HARNESS: &str = r#"import SolidCore.Solidity.Checked
open SolidCore.Solidity
open SolidCore.Solidity.Source (State CallResult)
@@BODY@@

def fzHex (l : List Nat) : String :=
  String.join (l.map fun b => (if b < 16 then "0" else "") ++ String.ofList (Nat.toDigits 16 b))

def fzCalls : List (List Nat) := @@CALLS@@

open SolidCore.Solidity.TypeCheck in
#eval show IO Unit from do
  match (do
      let p ← CheckedInput.program Fz.importedSourceUnit
      CheckedProgram.contract p "@@NAME@@") with
  | .error _ => IO.println "FZ REJECT"
  | .ok contract =>
    let base := contract.core.context
    let ctx := { base with sender := 0x2000, self := 0x1000, value := 0 }
    match CheckedContract.constructWithContext? @@FUEL@@ contract ctx State.empty 0x1000 0 [] with
    | some (CallResult.returned deployed _) =>
      let mut storage := deployed.storage
      for cd in fzCalls do
        let pre := { deployed with storage := storage }
        match CheckedContract.callCalldataAtFromWithContext? @@FUEL@@ contract ctx pre 0x1000 0x2000 0 cd with
        | none => IO.println "FZ NONE"
        | some r =>
          IO.println s!"FZ CALL {r.success} {fzHex r.output}"
          if r.success then
            for e in r.state.logEntries 0x1000 do
              IO.println s!"FZ LOG {e.topics} {fzHex e.data}"
            storage := r.state.storage
          for (k, v) in storage.toList do
            if v != 0 then IO.println s!"FZ SLOT {k} {v}"
          IO.println "FZ END"
    | _ => IO.println "FZ DEPLOYFAIL"
"#;

fn word(s: &str) -> Result<U256, String> {
    s.trim().parse::<U256>().map_err(|e| format!("bad word {s}: {e}"))
}

/// Runs `calls` (calldata) on SolidCore; one observation per call.
pub(crate) fn run(
    cli: &Cli,
    lean: &Path,
    file: &Path,
    name: &str,
    calls: &[Vec<u8>],
    dir: &Path,
) -> Result<Vec<Obs>, String> {
    let mut cmd = Command::new("python3");
    cmd.arg(lean.join("scripts/solc_ast_to_lean_source.py"))
        .arg(file)
        .args(["--contract", name, "--namespace", "Fz", "--body-only", "--solc"])
        .arg(&cli.lean_solc);
    let body =
        compile::run(cmd, Duration::from_secs(60)).map_err(|e| format!("import: {}", first(&e)))?;
    let list: Vec<String> = calls
        .iter()
        .map(|c| format!("[{}]", c.iter().map(u8::to_string).collect::<Vec<_>>().join(", ")))
        .collect();
    let text = HARNESS
        .replace("@@BODY@@", &String::from_utf8_lossy(&body))
        .replace("@@CALLS@@", &format!("[{}]", list.join(", ")))
        .replace("@@NAME@@", name)
        .replace("@@FUEL@@", &FUEL.to_string());
    let run_file = std::path::absolute(dir.join("Run.lean")).unwrap();
    std::fs::write(&run_file, text).unwrap();
    let mut cmd = Command::new("lake");
    cmd.current_dir(lean).args(["env", "lean"]).arg(&run_file).env("LEAN_NUM_THREADS", "1");
    let (_, out, err) = compile::run_status(cmd, Duration::from_secs(600))?;
    let out = String::from_utf8_lossy(&out).into_owned() + &String::from_utf8_lossy(&err);
    parse(&out, calls.len())
}

/// Reads what the harness printed: one observation per call.
fn parse(out: &str, calls: usize) -> Result<Vec<Obs>, String> {
    if let Some(e) = out.lines().find(|l| l.contains("error")) {
        return Err(format!("lean: {}", first(e)));
    }
    if out.contains("FZ REJECT") {
        return Err("SolidCore's checker rejects the program".into());
    }
    if out.contains("FZ DEPLOYFAIL") {
        return Err("SolidCore's constructor did not return".into());
    }
    let mut obs: Vec<Obs> = vec![];
    let mut cur: Option<Obs> = None;
    for line in out.lines() {
        let Some(rest) = line.strip_prefix("FZ ") else { continue };
        let (tag, rest) = rest.split_once(' ').unwrap_or((rest, ""));
        match tag {
            "NONE" => {
                obs.push(Obs::unsupported("SolidCore ran out of fuel or cannot run the call"))
            }
            "CALL" => {
                let (ok, data) = rest.split_once(' ').unwrap_or((rest, ""));
                let status = if ok == "true" { Status::Success } else { Status::Revert };
                let output = hex::decode(data.trim()).map_err(|e| e.to_string())?;
                cur = Some(Obs { status, output, ..Obs::default() });
            }
            "LOG" => {
                let o = cur.as_mut().ok_or("LOG outside a call")?;
                let (topics, data) = rest.rsplit_once("] ").ok_or("bad LOG")?;
                let topics = topics.trim_start_matches('[');
                let topics = if topics.trim().is_empty() {
                    vec![]
                } else {
                    topics.split(',').map(word).collect::<Result<Vec<_>, _>>()?
                };
                o.logs.push((topics, hex::decode(data.trim()).map_err(|e| e.to_string())?));
            }
            "SLOT" => {
                let o = cur.as_mut().ok_or("SLOT outside a call")?;
                let (k, v) = rest.split_once(' ').ok_or("bad SLOT")?;
                o.storage.insert(word(k)?, word(v)?);
            }
            "END" => obs.push(cur.take().ok_or("END outside a call")?),
            _ => {}
        }
    }
    if obs.len() != calls {
        return Err(format!("SolidCore printed {} results for {} calls", obs.len(), calls));
    }
    Ok(obs)
}

fn first(s: &str) -> String {
    s.lines().find(|l| !l.trim().is_empty()).unwrap_or("").chars().take(200).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_calls() {
        let out = "FZ CALL true 0000000e\nFZ LOG [1, 7] 07\nFZ SLOT 0 7\nFZ END\nFZ NONE\n\
                   FZ CALL false 4e487b71\nFZ END\n";
        let obs = parse(out, 3).unwrap();
        assert_eq!(obs[0].status, Status::Success);
        assert_eq!(obs[0].output, [0, 0, 0, 0x0e]);
        assert_eq!(obs[0].logs, [(vec![U256::from(1), U256::from(7)], vec![7])]);
        assert_eq!(obs[0].storage.get(&U256::ZERO), Some(&U256::from(7)));
        assert!(obs[1].is_unsupported());
        assert_eq!(obs[2].status, Status::Revert);
    }

    #[test]
    fn fails_closed() {
        assert!(parse("FZ REJECT\n", 1).is_err());
        assert!(parse("Run.lean:3:0: error: unknown identifier\n", 1).is_err());
        // A missing result is an error, never a call that agrees.
        assert!(parse("FZ CALL true 00\nFZ END\n", 2).is_err());
    }
}
