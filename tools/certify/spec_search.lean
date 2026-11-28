
namespace SpecConnectSearch

open SolidCore.Spec SolidCore.Refinement SolidCore.Refinement.Generic

/-- A computable reading of the spec's binary operators (untrusted: it only proposes a call). -/
def bop : SolidCore.Spec.BinOp → Int → Int → Option Int
  | .add, a, b => some (a + b)
  | .sub, a, b => some (a - b)
  | .mul, a, b => some (a * b)
  | .div, a, b => some (a.tdiv b)
  | .mod, a, b => some (a.tmod b)
  | .pow, a, b => if b.toNat ≤ 1024 then some (a ^ b.toNat) else none
  | .band, a, b => some ((toWord a &&& toWord b : Nat) : Int)
  | .bor, a, b => some ((toWord a ||| toWord b : Nat) : Int)
  | .bxor, a, b => some ((toWord a ^^^ toWord b : Nat) : Int)
  | .eq, a, b => some (if a = b then 1 else 0)
  | .ne, a, b => some (if a ≠ b then 1 else 0)
  | .lt, a, b => some (if a < b then 1 else 0)
  | .le, a, b => some (if a ≤ b then 1 else 0)
  | .gt, a, b => some (if a > b then 1 else 0)
  | .ge, a, b => some (if a ≥ b then 1 else 0)
  | .and, a, b => some (if a ≠ 0 ∧ b ≠ 0 then 1 else 0)
  | .or, a, b => some (if a ≠ 0 ∨ b ≠ 0 then 1 else 0)
  | .implies, a, b => some (if a = 0 ∨ b ≠ 0 then 1 else 0)

mutual
/-- A computable reading of the spec's terms: no quantifiers, ghosts or spec function calls. -/
partial def ev (e : Env) : Term → Option Int
  | .lit v => some v
  | .arg i => some (e.args.getD i 0)
  | .ret i => some (e.rets.getD i 0)
  | .var x => some ((e.vars.lookup x).getD 0)
  | .env .sender => some e.ctx.sender
  | .env .value => some e.ctx.value
  | .env .timestamp => some e.ctx.timestamp
  | .env .self => some e.ctx.self
  | .env .time => some e.cur.time
  | .load p off size ty => do
    let sl ← slot e p
    let o ← ev e off
    pure (ty.ofBits (e.cur.storage sl / 2 ^ (8 * toWord o) % 2 ^ (8 * size)))
  | .len p => do pure (e.cur.storage (← slot e p))
  | .old t => ev { e with inOld := true } t
  | .un .neg t => (ev e t).map (- ·)
  | .un .not t => (ev e t).map fun v => if v = 0 then 1 else 0
  | .bin op a b => do bop op (← ev e a) (← ev e b)
  | .shl t k => do pure ((← ev e t) * 2 ^ k % (wordMod : Int))
  | .shr t k => do pure ((← ev e t) / 2 ^ k)
  | .ite c a b => do if (← ev e c) ≠ 0 then ev e a else ev e b
  | _ => none
partial def slot (e : Env) : Place → Option Nat
  | .slot n => some (n % wordMod)
  | .offset b k => do pure (((← slot e b) + toWord (← ev e k)) % wordMod)
  | .entry b k => do pure (keccakWord (SolidCore.Spec.wordBytes (toWord (← ev e k)) ++ SolidCore.Spec.wordBytes (← slot e b)))
  | _ => none
end

/-- Whether every formula holds: some false when one evaluates to 0, none when one cannot be read. -/
def allC (e : Env) (ts : List Term) : Option Bool :=
  if ts.any (fun t => ev e t == some 0) then some false
  else if ts.all (fun t => (ev e t).isSome) then some true else none

def vals : List Nat := [0, 1, 2, 3, 5, 6, 7, 2 ^ 256 - 1]

def cands : Nat → List (List Nat)
  | 0 => [[]]
  | n + 1 => (cands n).flatMap fun l => vals.map (· :: l)

def store (ps : List (Nat × Nat)) (k : Nat) : Nat := (ps.lookup k).getD 0 % 2 ^ 256

/-- The storages tried: none written, the deployed one, and each of the first four slots alone at each value. -/
def stores (deployed : List (Nat × Nat)) : List (List (Nat × Nat)) :=
  [[], deployed] ++ (List.range 4).flatMap fun k => vals.map fun v => [(k, v)]

/-- The first call (arguments, decoded arguments, storage) on which the function's spec without its
succeeds-if conditions fails of its tree, as \"meets0\" (\"meets0inv\" when the invariants
fail after the call), else the first on which its succeeds-if
conditions hold and the tree fails, as \"succeeds\". -/
def search (b : Bundle) (fs : FnSpec) (ctx : Ctx) (n : Nat) (T : Tree) (deployed : List (Nat × Nat)) :
    Option (String × List Nat × List Int × List (Nat × Nat)) := Id.run do
  let as := if n ≤ 3 then cands n else [List.replicate n 0, List.replicate n 1]
  let mut succ : Option (String × List Nat × List Int × List (Nat × Nat)) := none
  for ps in stores deployed do
    for args in as do
      match decodeWords fs.params args with
      | none => pure ()
      | some iargs =>
        let pre : State := ⟨store ps, Ghost.zero, ctx.timestamp⟩
        let env := mkEnv ctx iargs [] pre pre
        if allC env fs.requires == some true && allC env b.spec.invariants == some true then
          let obs := (T.eval ⟨args, store ps, solHash, callerWord⟩).obs
          if obs.success then
            match decodeWords fs.rets (words obs.output) with
            | none => return some ("meets0", args, iargs, ps)
            | some rets =>
              let post : State := ⟨obs.storage, Ghost.zero, ctx.timestamp⟩
              if allC (mkEnv ctx iargs rets pre post) fs.ensures == some false then
                return some ("meets0", args, iargs, ps)
              if allC (mkEnv ctx iargs rets post post) b.spec.invariants == some false then
                return some ("meets0inv", args, iargs, ps)
          else if succ.isNone && !fs.succeedsIf.isEmpty && allC env fs.succeedsIf == some true then
            succ := some ("succeeds", args, iargs, ps)
  return succ

end SpecConnectSearch

namespace SpecConnectSearch

open SolidCore.Spec SolidCore.Refinement SolidCore.Refinement.Generic

def argCands (n : Nat) : List (List Nat) := if n ≤ 3 then cands n else [List.replicate n 0, List.replicate n 1]

/-- Decoded arguments and a storage on which the succeeds-if conditions of a nonpayable function
hold for a call with value 1 (the dispatcher reverts it), if any. -/
def valueCex (fs : FnSpec) (ctx : Ctx) (n : Nat) (deployed : List (Nat × Nat)) :
    Option (List Int × List (Nat × Nat)) := Id.run do
  if fs.payable || fs.succeedsIf.isEmpty then return none
  for ps in stores deployed do
    for args in argCands n do
      if let some iargs := decodeWords fs.params args then
        let pre : State := ⟨store ps, Ghost.zero, ctx.timestamp⟩
        if allC (mkEnv { ctx with value := 1 } iargs [] pre pre) fs.succeedsIf == some true then
          return some (iargs, ps)
  return none

/-- Whether no candidate call meets the requires clauses (each one read and false): a proposal that
they admit no call, which the module then proves for every call. -/
def requiresUnsat (fs : FnSpec) (ctx : Ctx) (n : Nat) (deployed : List (Nat × Nat)) : Bool :=
  !fs.requires.isEmpty && (stores deployed).all fun ps => (argCands n).all fun args =>
    match decodeWords fs.params args with
    | none => true
    | some iargs =>
      let pre : State := ⟨store ps, Ghost.zero, ctx.timestamp⟩
      allC (mkEnv ctx iargs [] pre pre) fs.requires == some false

/-- Whether the invariants are false of the deployed storage. -/
def initFails (b : Bundle) (ctx : Ctx) (deployed : List (Nat × Nat)) : Bool :=
  let pre : State := ⟨store deployed, Ghost.zero, ctx.timestamp⟩
  allC (mkEnv ctx [] [] pre pre) b.spec.invariants == some false

end SpecConnectSearch

