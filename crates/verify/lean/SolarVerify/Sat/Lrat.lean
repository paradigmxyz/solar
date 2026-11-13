import SolarVerify.Sat.Cnf
/-!
A checker for LRAT proofs of unsatisfiability that the kernel runs, and its soundness.

A proof adds clauses, each by reverse unit propagation (RUP) from earlier clauses its hints name,
until it adds the empty clause. RAT steps are not supported: a proof that needs one fails to
check. Deletions only free memory, so a proof here leaves them out and every clause stays
available.

Everything reduces in the kernel by structural recursion over lists, the proof tree and a fuel
bound, with arithmetic on `Nat` literals (which the kernel computes natively), so
`check f p = true` closes by `decide +kernel` and `check_sound` turns it into `f.Unsat`.
-/
namespace SolarVerify.Sat

noncomputable section

/-- Membership of a number in a list, by the kernel's `Nat.beq`. Written with the recursor, as are
`unfalsified` and `rup`, because the kernel reduces that in fewer steps than structural recursion
compiled through `brecOn`. -/
noncomputable def memb (x : Nat) (l : List Nat) : Bool :=
  List.rec false (fun y _ r => Nat.beq x y || r) l

theorem memb_nil (x : Nat) : memb x [] = false := rfl
theorem memb_cons (x y : Nat) (ys : List Nat) : memb x (y :: ys) = (Nat.beq x y || memb x ys) := rfl

theorem memb_iff {x : Nat} {l : List Nat} : memb x l = true ↔ x ∈ l := by
  induction l with
  | nil => simp [memb_nil]
  | cons y ys ih => simp [memb_cons, ih, Nat.beq_eq]

/-- The literals of `c` that the literals `A` (assumed true) do not falsify. -/
noncomputable def unfalsified (A : List Lit) (c : Clause) : List Lit :=
  List.rec [] (fun l _ r => bif memb (Lit.neg l) A then r else l :: r) c

theorem unfalsified_cons (A : List Lit) (l : Lit) (ls : Clause) :
    unfalsified A (l :: ls) =
      bif memb (Lit.neg l) A then unfalsified A ls else l :: unfalsified A ls := rfl

/-- One hint clause: it must leave one literal, which then holds while propagation goes on with
`r`, or none, a conflict, which ends the check successfully. -/
def rupStep (r : List Lit → Bool) (A : List Lit) (c : Clause) : Bool :=
  match unfalsified A c with
  | [] => true
  | [l] => r (l :: A)
  | _ => false

/-- Unit propagation along the hint clauses `hs`, from the literals `A`. -/
noncomputable def rup (hs : List Clause) (A : List Lit) : Bool :=
  List.rec (motive := fun _ => List Lit → Bool) (fun _ => false) (fun c _ r A => rupStep r A c) hs A

theorem rup_nil (A : List Lit) : rup [] A = false := rfl
theorem rup_cons (c : Clause) (cs : List Clause) (A : List Lit) :
    rup (c :: cs) A = rupStep (rup cs) A c := rfl

/-- A binary trie from clause IDs to clauses: key 0 at the root, then the key's bits from the
least significant one choose the left (1) or right (0) branch. -/
inductive Trie where
  | leaf
  | node (v : Option Clause) (l r : Trie)

namespace Trie

def root : Trie → Option Clause
  | leaf => none
  | node v _ _ => v

def left : Trie → Trie
  | leaf => leaf
  | node _ l _ => l

def right : Trie → Trie
  | leaf => leaf
  | node _ _ r => r

/-- The clause at key `k`, looking at most `fuel` levels down. -/
def find? : (fuel : Nat) → Trie → Nat → Option Clause
  | 0, t, k => if k = 0 then t.root else none
  | f + 1, t, k =>
    if k = 0 then t.root
    else if k % 2 = 1 then find? f t.left (k / 2) else find? f t.right (k / 2)

/-- Store a clause at key `k`; a key deeper than `fuel` is dropped. -/
def insert : (fuel : Nat) → Trie → Nat → Clause → Trie
  | 0, t, k, c => if k = 0 then node (some c) t.left t.right else t
  | f + 1, t, k, c =>
    if k = 0 then node (some c) t.left t.right
    else if k % 2 = 1 then node t.root (insert f t.left (k / 2) c) t.right
    else node t.root t.left (insert f t.right (k / 2) c)

/-- Every clause stored in the trie has property `P`. -/
def All (P : Clause → Prop) : Trie → Prop
  | leaf => True
  | node v l r => (∀ c, v = some c → P c) ∧ All P l ∧ All P r

theorem All.root {P : Clause → Prop} {t : Trie} (h : t.All P) {c : Clause} (e : t.root = some c) :
    P c := by
  cases t with
  | leaf => simp [Trie.root] at e
  | node v l r => exact h.1 c e

theorem All.left {P : Clause → Prop} {t : Trie} (h : t.All P) : t.left.All P := by
  cases t with
  | leaf => trivial
  | node v l r => exact h.2.1

theorem All.right {P : Clause → Prop} {t : Trie} (h : t.All P) : t.right.All P := by
  cases t with
  | leaf => trivial
  | node v l r => exact h.2.2

theorem find?_all {P : Clause → Prop} :
    ∀ (fuel : Nat) (t : Trie) (k : Nat) (c : Clause), t.All P → t.find? fuel k = some c → P c
  | 0, t, k, c, h, e => by
    simp only [find?] at e
    split at e
    · exact h.root e
    · simp at e
  | f + 1, t, k, c, h, e => by
    simp only [find?] at e
    split at e
    · exact h.root e
    · split at e
      · exact find?_all f _ _ c h.left e
      · exact find?_all f _ _ c h.right e

theorem insert_all {P : Clause → Prop} :
    ∀ (fuel : Nat) (t : Trie) (k : Nat) (c : Clause), t.All P → P c → (t.insert fuel k c).All P
  | 0, t, k, c, h, hc => by
    simp only [insert]
    split
    · exact ⟨fun d e => by cases e; exact hc, h.left, h.right⟩
    · exact h
  | f + 1, t, k, c, h, hc => by
    simp only [insert]
    split
    · exact ⟨fun d e => by cases e; exact hc, h.left, h.right⟩
    · split
      · exact ⟨fun d e => h.root e, insert_all f _ _ c h.left hc, h.right⟩
      · exact ⟨fun d e => h.root e, h.left, insert_all f _ _ c h.right hc⟩

end Trie

/-- How deep the clause trie goes: IDs below `2 ^ 64` are stored. -/
def fuel : Nat := 64

/-- One added clause: its ID, its literals, and the IDs of the clauses its RUP check uses, in
order. -/
structure Step where
  id : Nat
  clause : Clause
  hints : List Nat
  deriving Inhabited

/-- A proof: its steps in order, as a tree so that the term stays shallow for the kernel. -/
inductive Proof where
  | step (s : Step)
  | seq (a b : Proof)

/-- How running a proof ends: it failed, it derived the empty clause, or it goes on with these
clauses. -/
inductive Res where
  | fail
  | done
  | db (t : Trie)

/-- The clauses hint IDs name, or none when one is missing. -/
def lookupAll (t : Trie) : List Nat → Option (List Clause)
  | [] => some []
  | i :: is =>
    match t.find? fuel i with
    | none => none
    | some c =>
      match lookupAll t is with
      | none => none
      | some cs => some (c :: cs)

def Step.run (t : Trie) (s : Step) : Res :=
  match lookupAll t s.hints with
  | none => .fail
  | some hs =>
    if rup hs (s.clause.map Lit.neg) then
      match s.clause with
      | [] => .done
      | _ :: _ => .db (t.insert fuel s.id s.clause)
    else .fail

def Proof.run : Trie → Proof → Res
  | t, .step s => s.run t
  | t, .seq p q =>
    match p.run t with
    | .db t' => q.run t'
    | r => r

/-- The clauses of a formula, numbered from `i`. -/
def Trie.ofCnf (t : Trie) (i : Nat) : Cnf → Trie
  | [] => t
  | c :: cs => Trie.ofCnf (t.insert fuel i c) (i + 1) cs

/-- The formula's clauses, numbered from 1 as in DIMACS, then the proof, which must end by adding
the empty clause. -/
def check (f : Cnf) (p : Proof) : Bool :=
  match p.run (Trie.ofCnf .leaf 1 f) with
  | .done => true
  | _ => false

/-! ## Soundness -/

section sound
variable (a : Assignment)

/-- Every literal of `A` is true. -/
def AllTrue (A : List Lit) : Prop := ∀ l ∈ A, Lit.eval a l = true

/-- The clauses an assignment satisfies. -/
abbrev Sat (c : Clause) : Prop := Clause.eval a c = true

theorem unfalsified_spec {A : List Lit} (hA : AllTrue a A) :
    ∀ c : Clause, Clause.eval a c = true → ∃ l ∈ unfalsified A c, Lit.eval a l = true
  | [], h => by simp [Clause.eval] at h
  | l :: ls, h => by
    rw [Clause.eval_cons] at h
    rw [unfalsified_cons]
    cases hm : memb (Lit.neg l) A
    · simp only [Bool.cond_false]
      by_cases hl : Lit.eval a l = true
      · exact ⟨l, List.mem_cons_self, hl⟩
      · have hls : Clause.eval a ls = true := by simpa [hl] using h
        obtain ⟨x, hx, hxv⟩ := unfalsified_spec hA ls hls
        exact ⟨x, List.mem_cons_of_mem _ hx, hxv⟩
    · simp only [Bool.cond_true]
      have hn := hA _ (memb_iff.mp hm)
      rw [Lit.eval_neg] at hn
      have hl : Lit.eval a l = false := by simpa using hn
      rw [hl] at h
      exact unfalsified_spec hA ls (by simpa using h)

theorem rup_sound :
    ∀ (hs : List Clause) (A : List Lit), AllTrue a A → (∀ c ∈ hs, Clause.eval a c = true) →
      rup hs A = true → False
  | [], _, _, _, h => by simp [rup_nil] at h
  | c :: cs, A, hA, hc, h => by
    obtain ⟨l, hl, hlv⟩ := unfalsified_spec a hA c (hc c List.mem_cons_self)
    rw [rup_cons] at h
    unfold rupStep at h
    split at h
    · next e => rw [e] at hl; simp at hl
    · next l' e =>
      rw [e] at hl
      simp at hl
      subst hl
      refine rup_sound cs (l :: A) ?_ (fun d hd => hc d (List.mem_cons_of_mem _ hd)) h
      intro x hx
      rcases List.mem_cons.mp hx with rfl | hx
      · exact hlv
      · exact hA x hx
    · simp at h

theorem allTrue_neg_of_false (c : Clause) (h : Clause.eval a c = false) :
    AllTrue a (c.map Lit.neg) := by
  intro l hl
  obtain ⟨x, hx, rfl⟩ := List.mem_map.mp hl
  rw [Lit.eval_neg]
  have hn : ¬ (∃ l ∈ c, Lit.eval a l = true) := by
    rw [← Clause.eval_eq_true]; simp [h]
  have hx' : Lit.eval a x ≠ true := fun e => hn ⟨x, hx, e⟩
  simpa using hx'

theorem lookupAll_sat {t : Trie} (ht : t.All (Sat a)) :
    ∀ (is : List Nat) (hs : List Clause), lookupAll t is = some hs →
      ∀ c ∈ hs, Clause.eval a c = true
  | [], hs, e => by simp [lookupAll] at e; subst e; simp
  | i :: is, hs, e => by
    simp only [lookupAll] at e
    split at e
    · simp at e
    · next c hc =>
      split at e
      · simp at e
      · next cs hcs =>
        simp at e; subst e
        intro d hd
        rcases List.mem_cons.mp hd with rfl | hd
        · exact Trie.find?_all fuel t i d ht hc
        · exact lookupAll_sat ht is cs hcs d hd

theorem Step.run_sound {t : Trie} (ht : t.All (Sat a)) (s : Step) :
    (s.run t = .done → False) ∧ (∀ t', s.run t = .db t' → t'.All (Sat a)) := by
  unfold Step.run
  split
  · simp
  · next hs ehs =>
    have hsat := lookupAll_sat a ht _ hs ehs
    split
    · next hr =>
      -- The added clause is true: otherwise its negation propagates to a conflict.
      have hc : Clause.eval a s.clause = true := by
        cases e : Clause.eval a s.clause
        · exact (rup_sound a hs _ (allTrue_neg_of_false a _ e) hsat hr).elim
        · rfl
      split
      · next e => rw [e] at hc; simp [Clause.eval] at hc
      · refine ⟨by simp, fun t' e => ?_⟩
        simp at e; subst e
        exact Trie.insert_all fuel t s.id s.clause ht hc
    · simp

theorem Proof.run_sound :
    ∀ (p : Proof) (t : Trie), t.All (Sat a) →
      (p.run t = .done → False) ∧ (∀ t', p.run t = .db t' → t'.All (Sat a))
  | .step s, t, ht => Step.run_sound a ht s
  | .seq p q, t, ht => by
    simp only [Proof.run]
    have hp := Proof.run_sound p t ht
    split
    · next t' e => exact Proof.run_sound q t' (hp.2 t' e)
    · next r h1 =>
      refine ⟨fun e => hp.1 e, fun t' e => ?_⟩
      exact (h1 t' e).elim

theorem Trie.ofCnf_all (f : Cnf) (hf : ∀ c ∈ f, Clause.eval a c = true) :
    ∀ (t : Trie) (i : Nat), t.All (Sat a) → ∀ g : Cnf, (∀ c ∈ g, c ∈ f) →
      (Trie.ofCnf t i g).All (Sat a)
  | t, _, ht, [], _ => ht
  | t, i, ht, c :: cs, hg => by
    simp only [Trie.ofCnf]
    exact Trie.ofCnf_all f hf _ _ (Trie.insert_all fuel t i c ht (hf c (hg c List.mem_cons_self)))
      cs (fun d hd => hg d (List.mem_cons_of_mem _ hd))

end sound

/-- A proof that checks shows the formula unsatisfiable. -/
theorem check_sound (f : Cnf) (p : Proof) (h : check f p = true) : f.Unsat := by
  intro a
  cases e : Cnf.eval a f
  · rfl
  · have hf := (Cnf.eval_eq_true a f).mp e
    have h0 := Trie.ofCnf_all a f hf .leaf 1 trivial f (fun _ h => h)
    have hr := (Proof.run_sound a p _ h0).1
    unfold check at h
    split at h
    · next e' => exact (hr e').elim
    · simp at h


/-! ## Checking one step at a time

For large proofs, a single evaluation of `check` keeps every intermediate clause database in the
kernel's caches. Instead, each step can be its own declaration, a lemma that every assignment
satisfying the formula satisfies the step's clause, proved from the lemmas of its hints by one
small `rup` evaluation. The environment then plays the clause database. These are the lemmas such
declarations apply. -/

section steps
variable (a : Assignment)

/-- Every clause of the list is true. -/
def AllSat : List Clause → Prop
  | [] => True
  | c :: cs => Clause.eval a c = true ∧ AllSat cs

theorem AllSat.nil : AllSat a [] := trivial

theorem AllSat.cons (c : Clause) (cs : List Clause) (h : Clause.eval a c = true)
    (hs : AllSat a cs) : AllSat a (c :: cs) := ⟨h, hs⟩

theorem AllSat.mem : ∀ (hs : List Clause), AllSat a hs → ∀ c ∈ hs, Clause.eval a c = true
  | [], _, c, hc => by simp at hc
  | d :: ds, ⟨hd, hds⟩, c, hc => by
    rcases List.mem_cons.mp hc with rfl | hc
    · exact hd
    · exact AllSat.mem ds hds c hc

/-- A clause that unit propagation derives from true hint clauses is true. -/
theorem rup_clause (c : Clause) (hs : List Clause) (hr : rup hs (c.map Lit.neg) = true)
    (h : AllSat a hs) : Clause.eval a c = true := by
  cases e : Clause.eval a c
  · exact (rup_sound a hs _ (allTrue_neg_of_false a c e) (AllSat.mem a hs h) hr).elim
  · rfl

theorem Cnf.eval_mem (f : Cnf) (c : Clause) (h : Cnf.eval a f = true) (hc : c ∈ f) :
    Clause.eval a c = true :=
  (Cnf.eval_eq_true a f).mp h c hc

theorem mem_head (c : Clause) (l : List Clause) : c ∈ c :: l := List.mem_cons_self

theorem mem_tail (c d : Clause) (l : List Clause) (h : c ∈ l) : c ∈ d :: l :=
  List.mem_cons_of_mem _ h

theorem mem_append_left (c : Clause) (l r : List Clause) (h : c ∈ l) : c ∈ List.append l r :=
  List.mem_append_left r h

theorem mem_append_right (c : Clause) (l r : List Clause) (h : c ∈ r) : c ∈ List.append l r :=
  List.mem_append_right l h

end steps

/-- A formula from which every satisfying assignment would satisfy the empty clause is
unsatisfiable. -/
theorem unsat_of_empty (f : Cnf) (h : ∀ a, Cnf.eval a f = true → Clause.eval a [] = true) :
    f.Unsat := by
  intro a
  cases e : Cnf.eval a f
  · rfl
  · have := h a e
    simp [Clause.eval] at this

end

end SolarVerify.Sat
