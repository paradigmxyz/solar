import SolarTV.MIR.Transforms.AffineLoop

/-! Variable-length allocation and fill. The captured MIR has a checked
allocation, a sentinel loop on the fast edge, and a dynamically checked
loop with a carried address on the slow edge. The proof compares all
logical iteration prefixes with arbitrary stateful handlers. -/

namespace SolarTV.MIR.BoundedFill
open EvmRules LoopSimulation

def extent (length : Word) : Word := (length+1) <<< (5 : Nat)

def allocated (base length : Word) : Prop :=
  length+1 ≠ 0 ∧ (length+1).toNat ≤ 2^251-1 ∧
  base.toNat ≤ (base+extent length).toNat ∧ (base+extent length).toNat < 2^64

instance (base length : Word) : Decidable (allocated base length) := by
  unfold allocated
  infer_instance

private theorem unwrapped (a b : Word) (ordered : a.toNat ≤ (a+b).toNat) :
    (a+b).toNat = a.toNat + b.toNat := by
  have left := a.isLt
  have right := b.isLt
  rw [BitVec.toNat_add] at ordered ⊢
  by_cases fits : a.toNat+b.toNat < 2^256
  · exact Nat.mod_eq_of_lt fits
  · rw [Nat.mod_eq_sub_mod (by omega), Nat.mod_eq_of_lt (by omega)] at ordered
    omega

theorem allocatedLength (base length : Word) (valid : allocated base length) :
    length.toNat < 2^59 := by
  obtain ⟨nonzero,words,nowrap,fits⟩ := valid
  have lengthNat : (length+1).toNat = length.toNat+1 := by
    rw [BitVec.toNat_add]
    change (length.toNat+1) % 2^256 = length.toNat+1
    have bound := length.isLt
    by_cases small : length.toNat+1 < 2^256
    · exact Nat.mod_eq_of_lt small
    · have zero : (length+1).toNat = 0 := by
        rw [BitVec.toNat_add]
        change (length.toNat+1) % 2^256 = 0
        rw [show length.toNat+1 = 2^256 by omega]
      exact False.elim (nonzero (BitVec.eq_of_toNat_eq zero))
  have bytes : (extent length).toNat = (length+1).toNat*32 := by
    rw [extent, BitVec.toNat_shiftLeft, Nat.shiftLeft_eq]
    exact Nat.mod_eq_of_lt (by omega)
  rw [unwrapped _ _ nowrap, bytes, lengthNat] at fits
  omega

def fast (base length count : Word) : Prop :=
  (base ||| length).toNat < 2^64 ∧ count.toNat ≤ length.toNat

instance (base length count : Word) : Decidable (fast base length count) := by
  unfold fast
  infer_instance

def cap (count : Word) : Word := if count.toNat > 2^65 then 2^65 else count

theorem capIdentity (base length count : Word) (valid : allocated base length)
    (fastPath : fast base length count) : cap count = count := by
  have small := allocatedLength base length valid
  have within := fastPath.2
  simp [cap, show ¬ count.toNat > 2^65 by omega]

/-- The first two tests are pure. The final bounds check reads the live header
and may fail with the source panic. -/
def slowGuard (length count : Word) (check : AffineLoop.Guard State Event Failure) :
    AffineLoop.Guard State Event Failure :=
  fun index state =>
    if index.toNat < count.toNat ∧ index.toNat < length.toNat then check index state
    else ([],.ok (false,state))

def loop (pointer : Bool) (base length count : Word)
    (check : AffineLoop.Guard State Event Failure)
    (store : AffineLoop.Effect State Event Failure) :
    Loop (AffineLoop.Frame State) Event (AffineLoop.Exit State Failure) :=
  if fast base length count then
    if pointer then AffineLoop.sentinel 32 (AffineLoop.address (base+32) 32 (cap count)) store
    else AffineLoop.indexed (base+32) 32 (AffineLoop.below count) store
  else
    if pointer then AffineLoop.carried 32 (slowGuard length count check) store
    else AffineLoop.indexed (base+32) 32 (slowGuard length count check) store

theorem loopCorrespondence (base length count : Word) (valid : allocated base length)
    (check : AffineLoop.Guard State Event Failure)
    (store : AffineLoop.Effect State Event Failure) (fuel : Nat) (state : State) :
    observe (loop false base length count check store) fuel ⟨0,0,state⟩ =
      observe (loop true base length count check store) fuel ⟨0,base+32,state⟩ := by
  by_cases path : fast base length count
  · have bound := allocatedLength base length valid
    have within := path.2
    have fits : count.toNat * (32 : Word).toNat < 2^256 := by
      change count.toNat * 32 < 2^256
      omega
    simp only [loop, path, ite_true, ite_false, Bool.false_eq_true,
      capIdentity base length count valid path]
    exact (AffineLoop.sentinelSimulation (base+32) 32 count (by decide) fits store).observe fuel
      ⟨⟨rfl,by simp [AffineLoop.address],rfl⟩,by simp⟩
  · simp only [loop, path, ite_true, ite_false, Bool.false_eq_true]
    exact (AffineLoop.carrySimulation (base+32) 32 (slowGuard length count check) store).observe fuel
      ⟨rfl,by simp [AffineLoop.address],rfl⟩

theorem loopTerminates (base length count : Word)
    (check : AffineLoop.Guard State Event Failure)
    (store : AffineLoop.Effect State Event Failure) (state : State) :
    (observe (loop false base length count check store)
      (min length.toNat count.toNat+1) ⟨0,0,state⟩).2.isSome = true := by
  by_cases fastPath : fast base length count
  · simp only [loop,fastPath,ite_true,ite_false,Bool.false_eq_true]
    apply AffineLoop.indexedTerminates (limit := count)
    · intro index frame events next guarded
      exact (by simpa [AffineLoop.below] using guarded :
        events = [] ∧ index.toNat < count.toNat ∧ frame = next).2.1
    · have := fastPath.2
      simp [Nat.min_eq_right this]
  · simp only [loop,fastPath,ite_false,Bool.false_eq_true]
    let limit := if length.toNat < count.toNat then length else count
    have limitNat : limit.toNat = min length.toNat count.toNat := by
      dsimp only [limit]
      split <;> omega
    apply AffineLoop.indexedTerminates (limit := limit)
    · intro index frame events next guarded
      unfold slowGuard at guarded
      split at guarded
      · rw [limitNat]
        omega
      · simp at guarded
    · simp [limitNat]

/-- Shared prefix and continuations surround the two changed CFG regions.
Handlers can mutate any live state and record effects before failure. -/
structure Handlers (State Event Failure Result : Type) where
  abiFailure : State → List Event × Result
  allocationFailure : State → List Event × Result
  prepare : Word → Word → Word → State → List Event × Except Failure State
  check : AffineLoop.Guard State Event Failure
  store : AffineLoop.Effect State Event Failure
  failure : Failure → Result
  publish : Word → State → List Event × Result

def execute (pointer : Bool) (calldataSize base length count : Word)
    (handlers : Handlers State Event Failure Result) (fuel : Nat) (state : State) :
    List Event × Option Result :=
  if calldataSize.toNat < 68 then
    let (events,result) := handlers.abiFailure state
    (events,some result)
  else if allocated base length then
    match handlers.prepare base length (extent length) state with
    | (events,.error failure) => (events,some (handlers.failure failure))
    | (events,.ok state) =>
      let (trace,exit) := observe (loop pointer base length count handlers.check handlers.store)
        fuel ⟨0,if pointer then base+32 else 0,state⟩
      match exit with
      | none => (events ++ trace,none)
      | some (.failure failure) => (events ++ trace,some (handlers.failure failure))
      | some (.success state) =>
        let (resultTrace,result) := handlers.publish base state
        (events ++ trace ++ resultTrace,some result)
  else
    let (events,result) := handlers.allocationFailure state
    (events,some result)

theorem correspondence (calldataSize base length count : Word)
    (handlers : Handlers State Event Failure Result) (fuel : Nat) (state : State) :
    execute false calldataSize base length count handlers fuel state =
      execute true calldataSize base length count handlers fuel state := by
  unfold execute
  split
  · rfl
  split
  next valid =>
    split
    · rfl
    next events frame _ =>
      simp only [Bool.false_eq_true, ite_false, ite_true]
      rw [loopCorrespondence base length count valid handlers.check handlers.store fuel frame]
  · rfl

theorem completes (calldataSize base length count : Word)
    (handlers : Handlers State Event Failure Result) (state : State) :
    (execute false calldataSize base length count handlers
      (min length.toNat count.toNat+1) state).2.isSome = true := by
  unfold execute
  split
  · rfl
  split
  next valid =>
    split
    · rfl
    next events frame _ =>
      have stop := loopTerminates base length count handlers.check handlers.store frame
      change (observe (loop false base length count handlers.check handlers.store)
        (min length.toNat count.toNat+1) ⟨0,0,frame⟩).2.isSome = true at stop
      simp only [Bool.false_eq_true, ite_false]
      cases seen : observe (loop false base length count handlers.check handlers.store)
          (min length.toNat count.toNat+1) ⟨0,0,frame⟩ with
      | mk trace exit =>
        cases exit with
        | none =>
          have missing := congrArg (fun result => result.2.isSome) seen
          exact Bool.noConfusion (stop.symm.trans missing)
        | some result => cases result <;> rfl
  · rfl

/-! Typed CFG instances. Both views share the ABI, allocation and publication
blocks. All other instructions, phis, panic edges and loop backedges must
match the instance, including the wide-count clamp in the sentinel entry. -/

open State in
section

private def v (id : Nat) (width : Nat := 256) : Operand := ⟨.local id,.word width⟩
private def c (word : Word) : Operand := ⟨.constant ⟨256,word⟩,.word 256⟩

private def op (id code : Nat) (kind : Operation) (args : List Operand)
    (width : Nat := 256) : Instruction :=
  ⟨some id,.word width,some code,kind,args⟩

private def add (id : Nat) (a b : Operand) : Instruction :=
  op id 0x01 (.binary .add 256) [a,b]

private def lt (id : Nat) (a b : Operand) : Instruction :=
  op id 0x10 (.comparison .lt 256) [a,b] 1

private def gt (id : Nat) (a b : Operand) : Instruction :=
  op id 0x11 (.comparison .gt 256) [a,b] 1

private def shl (id : Nat) (a : Operand) : Instruction :=
  op id 0x1b (.shift .shl 256) [c 5,a]

private def load (id : Nat) (a : Operand) : Instruction :=
  op id 0x51 (.memory .load) [a]

private def mstore (a b : Operand) : Instruction :=
  ⟨none,.word 0,some 0x52,.memory .store,[a,b]⟩

private def br (id yes no : Nat) : Terminator := .branch (v id 1) yes no

private def panic (code : Word) : State.Block :=
  ⟨[],[mstore (c 32) (c code),mstore (c 0) (c 0x4e487b71)],.reverted (c 28) (c 36)⟩

private def entryBlocks (name : String) : List State.Block := [
  ⟨[],[op 0 0x36 (.memory .calldataSize) [],lt 1 (v 0) (c 68)],br 1 1 2⟩,
  ⟨[],[],.reverted (c 0) (c 0)⟩,
  ⟨[],[op 2 0x35 (.memory .calldataLoad) [c 4],op 3 0x35 (.memory .calldataLoad) [c 36],
    add 4 (v 2) (c 1),op 5 0x14 (.comparison .eq 256) [v 4,c 0] 1],br 5 3 4⟩,
  panic 65,
  ⟨[],[shl 6 (v 4),gt 7 (v 4) (c (2^251-1))],br 7 3 5⟩,
  ⟨[],[⟨some 8,.word 256,none,.context (.heapFloor name),[]⟩,
    add 9 (v 8) (v 6),lt 10 (v 9) (v 8),gt 11 (v 9) (c (2^64-1)),
    op 12 0x17 (.binary .or 1) [v 10 1,v 11 1] 1],br 12 6 7⟩,
  panic 65,
  ⟨[],[⟨none,.word 0,some 0x37,.memory .calldataCopy,[v 8,v 0,v 6]⟩,
    mstore (v 8) (v 2),op 13 0x17 (.binary .or 256) [v 8,v 2],
    lt 14 (v 13) (c (2^64))],br 14 8 9⟩,
  ⟨[],[lt 15 (v 2) (v 3)],br 15 9 10⟩]

private def publish (id : Nat) : State.Block :=
  ⟨[],[load id (v 8),shl (id+1) (v id),
    op (id+2) 0x03 (.binary .sub 256) [v 8,c 32],
    ⟨some (id+3),.pointer 0,none,.word,[v (id+2)]⟩,
    mstore (v (id+2)) (c 32),add (id+4) (v (id+1)) (c 64)],
    .returndata ⟨.local (id+3),.pointer 0⟩ (v (id+4))⟩

private def indexedStore (id index : Nat) (next : Nat) : State.Block :=
  ⟨[],[shl id (v index),add (id+1) (v 8) (v id),add (id+2) (v (id+1)) (c 32),
    mstore (v (id+2)) (c 7),add (id+3) (v index) (c 1)],.jump next⟩

def indexedBody (name : String) : State.Function := ⟨[],[],entryBlocks name ++ [
  ⟨[],[],.jump 11⟩,
  ⟨[],[],.jump 12⟩,
  ⟨[⟨16,.word 256,[(9,c 0),(17,v 35)]⟩],[lt 17 (v 16) (v 3)],br 17 13 14⟩,
  ⟨[⟨18,.word 256,[(10,c 0),(15,v 29)]⟩],[lt 19 (v 18) (v 3)],br 19 15 14⟩,
  ⟨[],[lt 20 (v 16) (v 2)],br 20 16 14⟩,
  publish 21,
  indexedStore 26 18 12,
  ⟨[],[load 30 (v 8),lt 31 (v 16) (v 30)],br 31 17 18⟩,
  indexedStore 32 16 11,
  panic 50],some name⟩

def pointerBody (name : String) : State.Function := ⟨[],[],entryBlocks name ++ [
  ⟨[],[add 16 (v 8) (c 32)],.jump 11⟩,
  ⟨[],[add 17 (v 8) (c 32),gt 18 (v 3) (c (2^65)),
    ⟨some 19,.word 256,none,.select,[v 18 1,c (2^65),v 3]⟩,
    shl 20 (v 19),add 21 (v 8) (v 20),add 22 (v 21) (c 32)],.jump 12⟩,
  ⟨[⟨23,.word 256,[(9,c 0),(17,v 37)]⟩,⟨24,.word 256,[(9,v 16),(17,v 38)]⟩],
    [lt 25 (v 23) (v 3)],br 25 13 14⟩,
  ⟨[⟨26,.word 256,[(10,v 17),(15,v 34)]⟩],
    [⟨some 27,.word 1,none,.notEqual,[v 22,v 26]⟩],br 27 15 14⟩,
  ⟨[],[lt 28 (v 23) (v 2)],br 28 16 14⟩,
  publish 29,
  ⟨[],[mstore (v 26) (c 7),add 34 (v 26) (c 32)],.jump 12⟩,
  ⟨[],[load 35 (v 8),lt 36 (v 23) (v 35)],br 36 17 18⟩,
  ⟨[],[mstore (v 24) (c 7),add 37 (v 23) (c 1),add 38 (v 24) (c 32)],.jump 11⟩,
  panic 50],some name⟩

end

end SolarTV.MIR.BoundedFill
