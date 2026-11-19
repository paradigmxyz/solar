import SolarTV.MIR.Transforms.TransferFromEntry
import Lean.Elab.Command
import Lean.Elab.Deriving.ToExpr
import Lean.Meta.Eval

namespace SolarTV.MIR.TransferFromEntry
open Lean Meta Elab Command EvmRules SolarTV.Execution

private def asHex (bytes : List Byte) : String :=
  let digit (n : Nat) := Char.ofNat (if n < 10 then n+48 else n+87)
  String.ofList <| bytes.flatMap fun b => [digit (b.toNat / 16),digit (b.toNat % 16)]

local instance (priority := high) : ToExpr (List Byte) where
  toExpr bytes :=
    if bytes.length > 1024 then mkApp (Lean.mkConst ``imageBytes) (toExpr (asHex bytes))
    else
      let ty := mkApp (Lean.mkConst ``BitVec) (mkNatLit 8)
      let cons := mkApp (Lean.mkConst ``List.cons [Level.zero]) ty
      let nil := mkApp (Lean.mkConst ``List.nil [Level.zero]) ty
      bytes.foldr (fun byte rest => mkApp2 cons (toExpr byte) rest) nil
  toTypeExpr := mkApp (Lean.mkConst ``List [Level.zero]) (mkApp (Lean.mkConst ``BitVec) (mkNatLit 8))

deriving instance ToExpr for World.Log
deriving instance ToExpr for World.Environment
deriving instance ToExpr for Account
deriving instance ToExpr for Journal
deriving instance ToExpr for EVM.State.Frame
deriving instance ToExpr for EVM.State.Stop
deriving instance ToExpr for EVM.State.Result
deriving instance ToExpr for MIR.State.Scalar
deriving instance ToExpr for MIR.State.Value
deriving instance ToExpr for MIR.State.ValueType
deriving instance ToExpr for MIR.State.Cursor
deriving instance ToExpr for MIR.State.Frame
deriving instance ToExpr for MIR.State.Continuation
deriving instance ToExpr for MIR.State.Machine
deriving instance ToExpr for ByteCut
deriving instance ToExpr for ByteExit
deriving instance ToExpr for MirCursor
deriving instance ToExpr for MirCut

private instance [ToExpr α] : ToExpr (Except String α) where
  toExpr
    | .ok a => mkApp3 (Lean.mkConst ``Except.ok [Level.zero,Level.zero])
        (Lean.mkConst ``String) (toTypeExpr α) (toExpr a)
    | .error reason => mkApp3 (Lean.mkConst ``Except.error [Level.zero,Level.zero])
        (Lean.mkConst ``String) (toTypeExpr α) (toExpr reason)
  toTypeExpr := mkApp2 (Lean.mkConst ``Except [Level.zero,Level.zero]) (Lean.mkConst ``String) (toTypeExpr α)

private def propose {S R : Type} [ToExpr S] [ToExpr R] (step : Trace.Step S R) (width : Nat) :
    Nat → Nat → Nat → Nat → S → Except String (List Expr × Option Expr)
  | _, _, _, 0, _ => .ok ([],none)
  | 0, _, _, _+1, _ => .error "trace work exhausted"
  | count+1, fuel, skip, left+1, state => do
    match ← Trace.segment step width fuel state with
    | .inl result => return ([],some (toExpr result))
    | .inr next =>
      match skip with
      | skip+1 => propose step width count (fuel-width) skip (left+1) next
      | 0 =>
        let (rest,result) ← propose step width count (fuel-width) 0 left next
        return (toExpr next :: rest,result)

private def quotedContext (image env : Expr) : TermElabM (Expr → Expr) := do
  let imageValue ← unsafe evalExpr (List Byte) (← inferType image) image
  let imageValue := toExpr imageValue
  let envValue ← unsafe evalExpr World.Environment (← inferType env) env
  let envValue := toExpr envValue
  return fun e => e.replace fun e =>
    if e == imageValue then some image else if e == envValue then some env else none

private def render (expression : Expr) : TermElabM String := do
  withOptions (fun opts => opts.setBool `pp.fullNames true |>.setBool `pp.deepTerms true |>.set `pp.width (180 : Nat)
    |>.set `pp.maxDepth (32768 : Nat) |>.set `pp.maxSteps (2000000 : Nat)) do
    let doc ← PrettyPrinter.ppExpr expression
    return doc.pretty 180

open Term in
elab "export_prefix " step:term " from " start:term " shared " share:term
    " world " envId:ident " width " w:num " fuel " f:num " skip " skip:num " limit " limit:num : command =>
  liftTermElabM do
    let step ← elabTerm step none
    let start ← elabTerm start none
    let image ← elabTerm share none
    let env ← elabTerm envId none
    synthesizeSyntheticMVarsNoPostponing
    let step ← instantiateMVars step
    let start ← instantiateMVars start
    let compress ← quotedContext image env
    let width := w.getNat
    let fuel := f.getNat
    if width = 0 then throwError "prefix segment width must be positive"
    let invoke ← mkAppM ``propose #[step,mkNatLit width,mkNatLit (fuel+1),mkNatLit fuel,
      mkNatLit skip.getNat,mkNatLit limit.getNat,start]
    let result ← unsafe evalExpr (Except String (List Expr × Option Expr)) (← inferType invoke) invoke
    let (middle,found) ← match result with
      | .ok trace => pure trace
      | .error reason => throwError "trace proposal failed: {reason}"
    let states ← middle.mapM (render ∘ compress)
    let found ← found.mapM (render ∘ compress)
    IO.println <| (Json.arr #[toJson states,toJson found]).compress

elab "check_chunk " name:ident " : " statement:term : command =>
  liftTermElabM do
    let type ← Term.elabTermEnsuringType statement (mkSort Level.zero)
    Term.synthesizeSyntheticMVarsNoPostponing
    let type ← instantiateMVars type
    unless type.isAppOfArity ``Eq 3 do throwError "expected a closed transition equation"
    let value ← mkEqRefl type.appArg!
    let name := (← getCurrNamespace) ++ name.getId
    addDecl (.thmDecl {name,levelParams := [],type,value})

private def hashInputs {S R : Type} (step : Trace.Step S R) (input : S → Option (List Byte)) :
    Nat → S → List (List Byte) → Except String (List (List Byte))
  | 0, _, _ => .error "hash proposal work exhausted"
  | remaining+1, state, found => do
    match ← step remaining state with
    | .inl _ => return found
    | .inr next =>
      let found := match input state with
        | some data => if found.contains data then found else data :: found
        | none => found
      hashInputs step input remaining next found

open Term in
elab "export_hashes " step:term " from " start:term " using " inspect:term " fuel " f:num : command =>
  liftTermElabM do
    let step ← elabTerm step none
    let start ← elabTerm start none
    let inspect ← elabTerm inspect none
    synthesizeSyntheticMVarsNoPostponing
    let invoke ← mkAppM ``hashInputs #[← instantiateMVars step, ← instantiateMVars inspect,
      mkNatLit f.getNat, ← instantiateMVars start, toExpr ([] : List (List Byte))]
    let result ← unsafe evalExpr (Except String (List (List Byte))) (← inferType invoke) invoke
    let .ok hashes := result | throwError "hash proposal failed"
    let pairs := hashes.map fun bytes => (bytes.map BitVec.toNat, toString (Keccak.hash bytes).toNat)
    IO.println (toJson pairs).compress

end SolarTV.MIR.TransferFromEntry
