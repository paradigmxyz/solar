import SolarTV.Sem

/-! The stages of `lower-abi` and `lower-dispatch`, which `-Zvalidate-passes` checks query by
query (`validate_passes/entries.rs`).

Before them, a contract's code is its external entries, and the ABI routes each call: to an entry
by its selector, to `receive` or `fallback`, or to an outcome of its own, such as a rejected value
or an empty revert on a miss. `Routing` is that routing and `routed` the semantics of a contract
whose entries run `body`. `lower-abi` replaces each entry's body with a wrapper that decodes its
arguments from calldata, runs the body and encodes its results; `lower-dispatch` replaces the
routing with a dispatcher, a program of whatever semantics runs the lowered MIR.

Each query's `unsat` verdict is an obligation here: `AbiObligation` for the query of one wrapper,
that on every call the routing sends to its entry the wrapper ends as the reference over the body
does, and `DispatchObligation` for the dispatcher's query, that the dispatcher ends as the routing
over the wrappers does. They are discharged per query by the solver, or by a certificate as the
pass queries are, not proved here: each states what its query's reference encodes, over the
semantics the encoder gives MIR, and relating that to a Lean semantics of MIR is the work of the
pass validation stages, outside this file. From them `abi_stage` proves the stage of `lower-abi` over all entries, and
`entries_stage` the stage from the routed contract to its dispatcher, which `to_bytecode`
composes with the stages after it. -/

namespace SolarTV.Entries

/-- How the ABI routes a call over a contract's entries `E`: to an entry, or to an outcome of the
routing itself. -/
structure Routing (E : Type) where
  route : Call → Option E
  miss : Call → Option Observable

/-- A routed contract whose entries run `body`. -/
def Routing.run {E : Type} (r : Routing E) (body : E → Call → Option Observable) (call : Call) :
    Option Observable :=
  match r.route call with
  | some entry => body entry call
  | none => r.miss call

/-- The semantics of a contract routed by `r`: a program is the body of each entry. -/
def routed {E : Type} (r : Routing E) : Sem := ⟨E → Call → Option Observable, r.run⟩

/-- What the `lower-abi` query of `entry` proves: on every call in scope that the routing sends to
the entry, its wrapper ends as its body does under the ABI's decoding and encoding, which
`body` includes. -/
def AbiObligation {E : Type} (r : Routing E) (scope : Call → Prop)
    (body wrapper : E → Call → Option Observable) (entry : E) : Prop :=
  ∀ call, scope call → r.route call = some entry →
    ∀ obs, body entry call = some obs → wrapper entry call = some obs

/-- What the `lower-dispatch` query proves: the dispatcher `q` ends as the routing over the
wrappers does. -/
def DispatchObligation {E : Type} (D : Sem) (r : Routing E) (scope : Call → Prop)
    (wrapper : E → Call → Option Observable) (q : D.Prog) : Prop :=
  ∀ call, scope call → ∀ obs, r.run wrapper call = some obs → D.run q call = some obs

/-- The queries of every wrapper give the stage of `lower-abi`. -/
theorem abi_stage {E : Type} {r : Routing E} {scope : Call → Prop}
    {body wrapper : E → Call → Option Observable}
    (each : ∀ entry, AbiObligation r scope body wrapper entry) :
    Stage (routed r) (routed r) body wrapper scope := by
  intro call inScope obs ran
  change r.run body call = some obs at ran
  change r.run wrapper call = some obs
  unfold Routing.run at ran ⊢
  cases h : r.route call with
  | none => rw [h] at ran; exact ran
  | some entry => rw [h] at ran; exact each entry call inScope h obs ran

theorem dispatch_stage {E : Type} {D : Sem} {r : Routing E} {scope : Call → Prop}
    {wrapper : E → Call → Option Observable} {q : D.Prog}
    (query : DispatchObligation D r scope wrapper q) : Stage (routed r) D wrapper q scope :=
  query

/-- The stage from the routed contract to its dispatcher: `lower-abi` then `lower-dispatch`. -/
theorem entries_stage {E : Type} {D : Sem} {r : Routing E} {scope : Call → Prop}
    {body wrapper : E → Call → Option Observable} {q : D.Prog}
    (abi : ∀ entry, AbiObligation r scope body wrapper entry)
    (dispatch : DispatchObligation D r scope wrapper q) : Stage (routed r) D body q scope :=
  (abi_stage abi).trans (dispatch_stage dispatch)

/-- From a source semantics to bytecode: a stage into the routed contract, such as the one from
SolidCore to semantic MIR, the entry lowering, a stage from the dispatcher to assembly, such as
the composed MIR, scheduling and EVM IR stages, and the assembly stage. -/
theorem to_bytecode {E : Type} {S D A B : Sem} {r : Routing E} {scope : Call → Prop}
    {src : S.Prog} {body wrapper : E → Call → Option Observable} {q : D.Prog} {p : A.Prog}
    {code : B.Prog}
    (front : Stage S (routed r) src body scope)
    (abi : ∀ entry, AbiObligation r scope body wrapper entry)
    (dispatch : DispatchObligation D r scope wrapper q)
    (backend : Stage D A q p scope) (assembly : Stage A B p code scope) :
    Stage S B src code scope :=
  ((front.trans (entries_stage abi dispatch)).trans backend).trans assembly

end SolarTV.Entries
