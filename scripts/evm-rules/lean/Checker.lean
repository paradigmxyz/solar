import Lean

/-!
# A long-running proof checker

`evm_check` imports `EvmRules` once, then reads one JSON object per line from standard input,
`{"id": …, "source": "<Lean commands>"}`, elaborates the source in that environment, and replies
with one line, `{"id": …, "ok": …, "messages": […]}`. `ok` holds when no message is an error and
none reports `sorry`; warnings count as errors, as they do for the prover's `lean` processes.
Discovery checks thousands of small candidate equalities this way, without starting `lean` and
importing the model for each one.
-/

open Lean Elab

/-- One reply message: its severity and rendered text. -/
def describe (message : Message) : IO Json := do
  let severity := match message.severity with
    | .error => "error"
    | .warning => "warning"
    | .information => "information"
  return Json.mkObj [("severity", severity), ("text", ← message.data.toString),
    ("line", toJson message.pos.line), ("column", toJson message.pos.column)]

def answer (env : Environment) (opts : Options) (request : Json) : IO Json := do
  let id := request.getObjValD "id"
  let some source := (request.getObjValAs? String "source").toOption
    | return Json.mkObj [("id", id), ("ok", false), ("messages", Json.arr #[])]
  let path := (request.getObjValAs? String "path").toOption.getD "<query>"
  let (_, log) ← Elab.process source env opts path
  let messages ← log.toList.toArray.mapM describe
  let texts ← log.toList.mapM fun message => message.data.toString
  -- A proof must have no error and must not rely on `sorry`.
  let ok := log.toList.all (·.severity != .error) && texts.all (!·.contains "sorry")
  return Json.mkObj [("id", id), ("ok", ok), ("messages", Json.arr messages)]

def main : IO UInt32 := do
  let sysroot ← findSysroot
  initSearchPath sysroot
  unsafe enableInitializersExecution
  let env ← importModules #[{ module := `EvmRules }] {} (leakEnv := true) (loadExts := true)
  let opts := ({} : Options)
    |>.set `sat.solver (sysroot / "bin" / "cadical").toString
    |>.set `maxRecDepth (100000 : Nat)
    |>.set `maxHeartbeats (0 : Nat)
    |>.set `warningAsError true
  let stdin ← IO.getStdin
  let stdout ← IO.getStdout
  repeat
    let line ← stdin.getLine
    if line.isEmpty then
      break
    let reply ← match Json.parse line with
      | .ok request => answer env opts request
      | .error error => pure <| Json.mkObj [("ok", false), ("error", error)]
    stdout.putStrLn reply.compress
    stdout.flush
  return 0
