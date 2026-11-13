/-! Contract-level semantics: what a call observes, and stages between languages.

The definitions up to the marker below restate the interface of solidity-lean verbatim, so
that a stage proved here composes with its stages from Solidity to bytecode. A `Sem` runs a
program on a call and gives its observable result, or `none` when it has none (it diverges,
or the call is outside what the semantics defines). `Stage A B p q scope` says that every
result `p` gives for a call in `scope`, `q` gives too. -/

namespace SolarTV

structure Call where
  calldata : ByteArray
  callvalue : Nat
  storage : List (Nat × Nat)

structure Observable where
  success : Bool
  output : List Nat
  logs : List (List Nat × List Nat)
  storage : Nat → Nat

def slotValue (storage : List (Nat × Nat)) (slot : Nat) : Nat :=
  storage.foldl (fun acc kv => if kv.1 % 2^256 = slot % 2^256 then kv.2 % 2^256 else acc) 0

structure Sem where
  Prog : Type
  run : Prog → Call → Option Observable

def Stage (A B : Sem) (p : A.Prog) (q : B.Prog) (scope : Call → Prop) : Prop :=
  ∀ call, scope call → ∀ obs, A.run p call = some obs → B.run q call = some obs

theorem Stage.trans {A B C : Sem} {p : A.Prog} {q : B.Prog} {r : C.Prog} {scope : Call → Prop}
    (first : Stage A B p q scope) (second : Stage B C q r scope) : Stage A C p r scope :=
  fun call inScope obs ran => second call inScope obs (first call inScope obs ran)

/-! Everything below this marker is ours, not solidity-lean's interface. -/

theorem Stage.refl (A : Sem) (p : A.Prog) (scope : Call → Prop) : Stage A A p p scope :=
  fun _ _ _ ran => ran

/-- A stage holds on any narrower scope. -/
theorem Stage.mono {A B : Sem} {p : A.Prog} {q : B.Prog} {scope narrower : Call → Prop}
    (stage : Stage A B p q scope) (within : ∀ call, narrower call → scope call) :
    Stage A B p q narrower :=
  fun call inScope => stage call (within call inScope)

/-- The frame a call runs in, which the interface leaves implicit. -/
structure Environment where
  caller : Nat
  callee : Nat
  frames : Nat

/-- One frame, called by 0x2000 at 0x1000, as solidity-lean's test environment. The machine
runs at its callee but takes the caller from the oracle, so a stage proved for every oracle holds
for every caller, this one among them. -/
def environment : Environment := ⟨0x2000, 0x1000, 1⟩

/-- A failed call rolls back: it logs nothing and leaves the storage it started with. Its
output is then its revert data, which each semantics defines (`Machine.revertData` here). -/
def Sem.FailureConvention (S : Sem) : Prop :=
  ∀ p call obs, S.run p call = some obs → obs.success = false →
    obs.logs = [] ∧ obs.storage = slotValue call.storage

end SolarTV
