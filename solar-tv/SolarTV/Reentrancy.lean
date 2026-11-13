import SolarTV.Machine

/-! # External calls: what a reentrant callee can do to the caller's storage

The machine semantics leaves the storage after an external call to the oracle
(`Oracle.storageAfter`), as a function of the frame's events, the call among them with the storage
it saw. Every stage holds for every oracle, so it holds for any oracle a theorem picks; but a
theorem about the contract's own specification (an invariant, a frame) cannot hold for every
oracle, since an arbitrary oracle may write anything to the contract's storage across a call.

This module names the oracles a chain gives the contract. Other code reaches the contract's
storage only by calling the contract (no `DELEGATECALL` into it, no other writer of its account),
so across one external call the contract's storage moves only by a sequence of reentrant
transactions into the contract itself: each is a run of the same program from the storage at that
point, which commits its storage when it succeeds and keeps it when it fails, and whose own
external calls are answered the same way, one call deeper. A call that fails, or whose callee never
reenters, leaves the storage as it was.

* `Step R σ σ'`: one reentrant run, under an oracle in `R`, takes storage `σ` to `σ'`.
* `Reach R`: any number of such runs.
* `Respects n o`: every call and creation of a frame under `o` leaves the storage at what
  reentrant runs reach, each under an oracle that respects `n - 1`; at depth 0 nothing reenters.

Two laws carry a property across a call, which is how a verifier's havoc of an external call is
justified:

* `Respects.frame` (a reentrancy lock): when no successful run of the program changes storage
  that satisfies `Locked`, a call made with `Locked` storage leaves the storage as it was.
* `Respects.preserves` (an invariant): when every successful run of the program under an oracle
  of depth `n` keeps `I`, a call under an oracle of depth `n + 1` made with `I` keeps `I`.
  `preserves_all` turns a run's proof, given `I` after each call it holds at, into `I` at
  every depth.

The model leaves out transient storage (a reentrant run starts with zero transient storage, and
`transientAfter` stays free), ETH balances, and the code of other accounts. -/

namespace SolarTV.Reentrancy

open Machine EvmRules

/-- A contract's storage. -/
abbrev Storage := Word → Word

/-- Whether a frame's end commits its state: a normal return or stop. -/
def Halt.commits : Halt → Bool
  | .stop | .ret _ => true
  | .revert _ | .invalid => false

/-- A call starts from storage `σ`: its storage pairs give each slot the value `σ` has there. -/
def StartsAt (call : Call) (σ : Storage) : Prop :=
  ∀ w : Word, slotValue call.storage w.toNat = (σ w).toNat

theorem StartsAt.initial {call : Call} {σ : Storage} (h : StartsAt call σ) :
    (State.initial call).storage = σ := by
  funext w
  simp only [State.initial, h w, BitVec.ofNat_toNat, BitVec.setWidth_eq]

/-- The storage a call leaves: what a committed end holds, else the storage it started from. -/
def after (σ : Storage) (r : Halt × State) : Storage :=
  if Halt.commits r.1 then r.2.storage else σ

variable (L : Layer) (p : L.Prog)

/-- One reentrant transaction into the program, under some oracle in `R`, from storage `σ`
to storage `σ'`. -/
def Step (R : Oracle → Prop) (σ σ' : Storage) : Prop :=
  ∃ o call r, R o ∧ StartsAt call σ ∧ L.Ends o p call r ∧ σ' = after σ r

/-- Any number of reentrant transactions. -/
inductive Reach (R : Oracle → Prop) : Storage → Storage → Prop
  | refl (σ : Storage) : Reach R σ σ
  | step {σ σ₁ σ₂ : Storage} : Reach R σ σ₁ → Step L p R σ₁ σ₂ → Reach R σ σ₂

/-- The world with an event that hands the contract's storage to outside code: a call or a
creation, with the storage it saw. -/
def Handoff (world : List Event) (σ : Storage) : Prop :=
  ∃ pre, (∃ kind operands input transient,
      world = pre ++ [.call kind operands input σ transient]) ∨
    (∃ operands code transient, world = pre ++ [.create operands code σ transient])

/-- Every hand-off under `o` leaves the storage at what reentrant runs under `R` reach. -/
def Answers (R : Oracle → Prop) (o : Oracle) : Prop :=
  ∀ world σ, Handoff world σ → Reach L p R σ (o.storageAfter world)

/-- Oracles of a chain where outside code reaches the contract's storage only through the
program, at most `n` calls deep. -/
def Respects : Nat → Oracle → Prop
  | 0 => Answers L p (fun _ => False)
  | n + 1 => Answers L p (Respects n)

variable {L p}

theorem Reach.mono {R R' : Oracle → Prop} (sub : ∀ o, R o → R' o) {σ σ' : Storage}
    (h : Reach L p R σ σ') : Reach L p R' σ σ' := by
  induction h with
  | refl => exact .refl _
  | step _ s ih =>
    obtain ⟨o, call, r, ro, start, ends, eq⟩ := s
    exact .step ih ⟨o, call, r, sub o ro, start, ends, eq⟩

theorem Reach.of_false {σ σ' : Storage} (h : Reach L p (fun _ => False) σ σ') : σ' = σ := by
  induction h with
  | refl => rfl
  | step _ s _ => obtain ⟨_, _, _, f, _⟩ := s; exact f.elim

/-- A deeper bound admits every oracle of a shallower one. -/
theorem Respects.succ : ∀ {n : Nat} {o : Oracle}, Respects L p n o → Respects L p (n + 1) o
  | 0, _, h => fun w σ hw => (h w σ hw).mono (fun _ f => f.elim)
  | _ + 1, _, h => fun w σ hw => (h w σ hw).mono (fun _ => Respects.succ)

/-- A property every reentrant step keeps holds after any number of them. -/
theorem Reach.keeps {R : Oracle → Prop} (P : Storage → Prop)
    (step : ∀ σ σ', P σ → Step L p R σ σ' → P σ') {σ σ' : Storage} (h : Reach L p R σ σ')
    (start : P σ) : P σ' := by
  induction h with
  | refl => exact start
  | step _ s ih => exact step _ _ ih s

/-- Every hand-off under an oracle of any depth reaches its storage by reentrant runs. -/
theorem Respects.reach {n : Nat} {o : Oracle} (h : Respects L p n o) {world : List Event}
    {σ : Storage} (hw : Handoff world σ) : ∃ R, Reach L p R σ (o.storageAfter world) := by
  cases n with
  | zero => exact ⟨_, h world σ hw⟩
  | succ n => exact ⟨_, h world σ hw⟩

/-- No successful run, under any oracle, changes storage that satisfies `Locked`: the entry
points that write revert while the lock is held, and the others write nothing. -/
def LockFrame (Locked : Storage → Prop) : Prop :=
  ∀ o call r, Locked (State.initial call).storage → L.Ends o p call r →
    Halt.commits r.1 = true → r.2.storage = (State.initial call).storage

/-- Under a reentrancy lock, an external call leaves the storage as it was. -/
theorem Respects.frame {Locked : Storage → Prop} (lock : LockFrame (L := L) (p := p) Locked)
    {n : Nat} {o : Oracle} (h : Respects L p n o) {world : List Event} {σ : Storage}
    (hw : Handoff world σ) (locked : Locked σ) : o.storageAfter world = σ := by
  obtain ⟨R, reach⟩ := h.reach hw
  refine reach.keeps (fun τ => τ = σ) (fun τ τ' eq s => ?_) rfl
  subst eq
  obtain ⟨o', call, r, _, start, ends, rfl⟩ := s
  unfold after
  split
  · rename_i commits
    rw [lock o' call r (by rw [start.initial]; exact locked) ends commits, start.initial]
  · rfl

/-- Every successful run of the program under an oracle in `R` keeps `I`. -/
def Preserves (R : Oracle → Prop) (I : Storage → Prop) : Prop :=
  ∀ o call r, R o → I (State.initial call).storage → L.Ends o p call r →
    Halt.commits r.1 = true → I r.2.storage

/-- An invariant that holds at an external call holds after it, when the runs one call deeper
keep it. -/
theorem Respects.preserves {I : Storage → Prop} {n : Nat}
    (keep : Preserves (L := L) (p := p) (Respects L p n) I) {o : Oracle}
    (h : Respects L p (n + 1) o) {world : List Event} {σ : Storage} (hw : Handoff world σ)
    (holds : I σ) : I (o.storageAfter world) :=
  (h world σ hw).keeps I (fun τ τ' hτ s => by
    obtain ⟨o', call, r, ro, start, ends, rfl⟩ := s
    unfold after
    split
    · rename_i commits
      exact keep o' call r ro (by rw [start.initial]; exact hτ) ends commits
    · exact hτ) holds

/-- At depth 0 every property holds after every call it holds at: nothing reenters. -/
theorem Respects.preserves_zero {I : Storage → Prop} {o : Oracle} (h : Respects L p 0 o)
    {world : List Event} {σ : Storage} (hw : Handoff world σ) (holds : I σ) :
    I (o.storageAfter world) := by
  rw [(h world σ hw).of_false]; exact holds

/-- An invariant that every run keeps, given that each of the run's external calls made where
it holds leaves it holding (a verifier's check before the call and its assumption after),
holds of every run at every depth. -/
theorem preserves_all (I : Storage → Prop)
    (run : ∀ n o, (∀ world σ, Handoff world σ → I σ → I (o.storageAfter world)) →
      Respects L p n o → ∀ call r, I (State.initial call).storage → L.Ends o p call r →
        Halt.commits r.1 = true → I r.2.storage) :
    ∀ n, Preserves (L := L) (p := p) (Respects L p n) I := by
  intro n
  induction n with
  | zero =>
    exact fun o call r ro => run 0 o (fun _ _ hw hi => ro.preserves_zero hw hi) ro call r
  | succ n ih =>
    exact fun o call r ro => run (n + 1) o (fun _ _ hw hi => ro.preserves ih hw hi) ro call r

/-! ## The constraint is satisfiable and not trivial -/

/-- The oracle whose callees never reenter: after a hand-off the storage is the one handed
off. -/
def quiet : Oracle where
  env _ _ _ := 0
  result _ := 0
  storageAfter world := match world.getLast? with
    | some (.call _ _ _ σ _) | some (.create _ _ σ _) => σ
    | _ => fun _ => 0
  transientAfter _ _ := 0
  keccak _ := 0
  caller := 0

theorem quiet_handoff {world : List Event} {σ : Storage} (hw : Handoff world σ) :
    quiet.storageAfter world = σ := by
  obtain ⟨pre, ⟨_, _, _, _, rfl⟩ | ⟨_, _, _, rfl⟩⟩ := hw <;> simp [quiet]

/-- No depth's constraint is vacuous: the quiet oracle respects them all. -/
theorem quiet_respects : ∀ n, Respects L p n quiet
  | 0 => fun _ _ hw => by rw [quiet_handoff hw]; exact .refl _
  | _ + 1 => fun _ _ hw => by rw [quiet_handoff hw]; exact .refl _

/-- The oracle that zeroes the storage after every hand-off. -/
def wipe : Oracle := { quiet with storageAfter := fun _ _ => 0 }

/-- Depth 0 rules out an oracle that writes the contract's storage across a call. -/
theorem wipe_not_respects : ¬ Respects L p 0 wipe := by
  intro h
  have := (h [.call .call [] [] (fun _ => 1) (fun _ => 0)] (fun _ => 1)
    ⟨[], .inl ⟨.call, [], [], fun _ => 0, rfl⟩⟩).of_false
  have := congrFun this 0
  simp [wipe] at this

end SolarTV.Reentrancy
