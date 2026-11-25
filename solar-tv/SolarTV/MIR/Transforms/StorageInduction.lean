import SolarTV.MIR.State.StorageTrace

/-! The search and shift loops of the fixed five-record storage example.
The region starts after initialization, with the base slot and needle live,
and ends at the common packing continuation or the shared failure path. -/

namespace SolarTV.MIR.StorageInduction
open EvmRules

def address (base index : Word) : Word := base + (index + index)

def recover (base cursor : Word) : Word := (cursor - base) >>> (1 : Nat)

def addressMask : Word := 0xffffffffffffffffffffffffffffffffffffffff

/-- Ordered commands with a continuation for each read and each potentially
failing effect. Equality permits interpretation in a stateful world, including
static mode, aliasing, checked memory growth and failed allocation. -/
inductive Action where
  | resume
  | workLimit
  | read (slot : Word) (next : Word → Action)
  | write (slot value : Word) (next : Action)
  | reserve (next : Action)
  | allocate (next : Action)

def clear (base : Word) : Action :=
  .allocate (.write (base+8) 0 (.write (base+8+1) 0 .resume))

def indexedShift (base : Word) : Nat → Word → Action
  | 0, _ => .workLimit
  | fuel+1, index =>
    if index.toNat < 4 then
      let src := address base (index+1)
      let dest := address base index
      .reserve (.read src fun owner =>
        .read (src+1) fun amount =>
          .write dest (owner &&& addressMask)
            (.write (dest+1) amount (indexedShift base fuel (index+1))))
    else clear base

def pointerShift (base : Word) : Nat → Word → Word → Action
  | 0, _, _ => .workLimit
  | fuel+1, index, cursor =>
    if index.toNat < 4 then
      .reserve (.read (cursor+2) fun owner =>
        .read (cursor+3) fun amount =>
          .write cursor (owner &&& addressMask)
            (.write (cursor+1) amount (pointerShift base fuel (index+1) (cursor+2))))
    else clear base

def indexedSearch (base needle : Word) : Nat → Word → Action
  | 0, _ => .workLimit
  | fuel+1, index =>
    if index.toNat < 5 then
      .read (address base index) fun owner =>
        if owner &&& addressMask = needle then indexedShift base 5 index
        else indexedSearch base needle fuel (index+1)
    else .resume

def pointerSearch (base needle : Word) : Nat → Word → Action
  | 0, _ => .workLimit
  | fuel+1, cursor =>
    if cursor = base+10 then .resume
    else .read cursor fun owner =>
      if owner &&& addressMask = needle then
        let index := recover base cursor
        pointerShift base 5 index (base + (index <<< (1 : Nat)))
      else pointerSearch base needle fuel (cursor+2)

theorem addressStep (base index : Word) :
    address base (index+1) = address base index + 2 := by
  unfold address
  have pair : (index+1)+(index+1) = (index+index)+2 := by
    rw [BitVec.add_assoc, ← BitVec.add_assoc (1 : Word), BitVec.add_comm (1 : Word) index,
      BitVec.add_assoc index 1, ← BitVec.add_assoc index index]
    rfl
  rw [pair, BitVec.add_assoc base]

theorem recoverIndex (base index : Word) (small : index.toNat ≤ 5) :
    recover base (address base index) = index := by
  have cancel : address base index - base = index+index := by
    rw [address, BitVec.add_comm, BitVec.add_sub_cancel]
  apply BitVec.eq_of_toNat_eq
  rw [recover,cancel,BitVec.toNat_ushiftRight,BitVec.toNat_add]
  rw [Nat.mod_eq_of_lt (by omega : index.toNat + index.toNat < 2^256)]
  omega

theorem shiftStart (base index : Word) :
    base + (index <<< (1 : Nat)) = address base index := by
  rw [address,BitVec.shiftLeft_eq_mul_twoPow]
  change base + index * 2 = base + (index+index)
  have double : index * 2 = index + index := by
    rw [show (2 : Word) = 1+1 from rfl, BitVec.mul_add]; simp
  rw [double]

theorem shiftSimulation (base index : Word) (fuel : Nat) :
    indexedShift base fuel index = pointerShift base fuel index (address base index) := by
  induction fuel generalizing index with
  | zero => rfl
  | succ fuel ih =>
    simp only [indexedShift,pointerShift]
    rw [ih, addressStep, BitVec.add_assoc (address base index) 2 1]
    rfl

theorem searchSimulation (base needle index : Word) (fuel : Nat)
    (within : index.toNat ≤ 5) :
    indexedSearch base needle fuel index =
      pointerSearch base needle fuel (address base index) := by
  induction fuel generalizing index with
  | zero => rfl
  | succ fuel ih =>
    have endAt : address base (5 : Word) = base+10 := rfl
    have guard : address base index = base+10 ↔ index = 5 := by
      rw [← endAt]
      constructor
      · intro h
        have same := congrArg (recover base) h
        simpa only [recoverIndex base index within, recoverIndex base 5 (by decide)] using same
      · intro h
        rw [h]
    by_cases active : index.toNat < 5
    · have different : index ≠ 5 := by
        intro h
        simp [h] at active
      have nextNat : (index+1).toNat = index.toNat+1 := by
        rw [BitVec.toNat_add]
        change (index.toNat+1) % 2^256 = index.toNat+1
        exact Nat.mod_eq_of_lt (by omega)
      simp only [indexedSearch, pointerSearch, guard, different, active, ite_false, ite_true,
        recoverIndex base index within, shiftStart]
      rw [← addressStep, ← ih (index+1) (by omega), ← shiftSimulation]
    · have last : index = 5 := BitVec.eq_of_toNat_eq (by change index.toNat = 5; omega)
      rw [indexedSearch, pointerSearch, ite_eq_right active, ite_eq_left (guard.mpr last)]

def completes : Action → Prop
  | .resume => True
  | .workLimit => False
  | .read _ next => ∀ word, completes (next word)
  | .write _ _ next | .reserve next | .allocate next => completes next

private theorem advanceIndex (index : Word) (small : index.toNat ≤ 5) :
    (index+1).toNat = index.toNat+1 := by
  rw [BitVec.toNat_add]
  change (index.toNat+1) % 2^256 = index.toNat+1
  exact Nat.mod_eq_of_lt (by omega)

theorem shiftCompletes (base index : Word) (fuel : Nat)
    (enough : 4 - index.toNat < fuel) : completes (indexedShift base fuel index) := by
  induction fuel generalizing index with
  | zero => omega
  | succ fuel ih =>
    by_cases active : index.toNat < 4
    · simp only [indexedShift, active, ite_true, completes]
      intro _ _
      exact ih (index+1) (by rw [advanceIndex index (by omega)]; omega)
    · simp [indexedShift, active, clear, completes]

theorem searchCompletes (base needle index : Word) (fuel : Nat)
    (enough : 5 - index.toNat < fuel) : completes (indexedSearch base needle fuel index) := by
  induction fuel generalizing index with
  | zero => omega
  | succ fuel ih =>
    by_cases active : index.toNat < 5
    · simp only [indexedSearch, active, ite_true, completes]
      intro owner
      split
      · exact shiftCompletes base index 5 (by omega)
      · exact ih (index+1) (by rw [advanceIndex index (by omega)]; omega)
    · simp [indexedSearch, active, completes]

/-- Six header visits cover five reads and the exhausted search. A matched
index is in 0..4, so at most four shifts precede the clear. No assumption
bounds the base address or the storage contents. -/
theorem correspondence (base needle : Word) :
    indexedSearch base needle 6 0 = pointerSearch base needle 6 base := by
  simpa [address] using searchSimulation base needle 0 6 (by decide)

/-! The two typed CFGs for the projection. Block 10 is the common continuation
outside the cut. The deferred allocation's site is kept in the template and
paired by its position, with equal placement at the world boundary. -/

open State in
section

private def v (id : Nat) (width : Nat := 256) : Operand := ⟨.local id,.word width⟩
private def c (value : Word) : Operand := ⟨.constant ⟨256,value⟩,.word 256⟩
private def base : Operand := ⟨.argument 0,.word 256⟩
private def needle : Operand := ⟨.argument 1,.word 256⟩

private def op (id code : Nat) (kind : Operation) (args : List Operand)
    (width : Nat := 256) : Instruction := ⟨some id,.word width,some code,kind,args⟩

private def add (id : Nat) (a b : Operand) : Instruction :=
  op id 0x01 (.binary .add 256) [a,b]

private def lt (id : Nat) (a b : Operand) : Instruction :=
  op id 0x10 (.comparison .lt 256) [a,b] 1

private def load (id : Nat) (slot : Operand) : Instruction :=
  op id 0x54 (.storage false false) [slot]

private def store (slot value : Operand) : Instruction :=
  ⟨none,.word 0,some 0x55,.storage false true,[slot,value]⟩

private def mask (id : Nat) (value : Operand) : Instruction :=
  op id 0x16 (.binary .and 256) [value,c addressMask]

private def matchesNeedle (id : Nat) (value : Operand) : Instruction :=
  op id 0x14 (.comparison .eq 256) [value,needle] 1

private def mstore (slot : Word) (value : Operand) : Instruction :=
  ⟨none,.word 0,some 0x52,.memory .store,[c slot,value]⟩

private def br (condition yes no : Nat) : Terminator := .branch (v condition 1) yes no

private def reserveInstructions (id : Nat) : List Instruction := [
  op id 0x51 (.memory .load) [c 64],add (id+1) (v id) (c 64),
  lt (id+2) (v (id+1)) (v id),
  op (id+3) 0x11 (.comparison .gt 256) [v (id+1),c 0xffffffffffffffff] 1,
  op (id+4) 0x17 (.binary .or 1) [v (id+2) 1,v (id+3) 1] 1]

private def clearBlock (site id : Nat) : State.Block :=
  ⟨[],[⟨some id,.pointer 0,none,.deferredAllocate "remove" site false false,[c 64]⟩,
    add (id+1) base (c 8),store (v (id+1)) (c 0),add (id+2) (v (id+1)) (c 1),
    store (v (id+2)) (c 0)],.jump 10⟩

private def panicBlock : State.Block :=
  ⟨[],[mstore 32 (c 65),mstore 0 (c 0x4e487b71)],.reverted (c 28) (c 36)⟩

def indexBlocks (site : Nat) : List State.Block := [
  ⟨[],[],.jump 1⟩,
  ⟨[⟨0,.word 256,[(0,c 0),(3,v 7)]⟩],[lt 1 (v 0) (c 5)],br 1 2 10⟩,
  ⟨[],[add 2 (v 0) (v 0),add 3 (v 2) base,load 4 (v 3),mask 5 (v 4),
    matchesNeedle 6 (v 5)],br 6 4 3⟩,
  ⟨[],[add 7 (v 0) (c 1)],.jump 1⟩,
  ⟨[],[],.jump 5⟩,
  ⟨[⟨8,.word 256,[(4,v 0),(7,v 10)]⟩],[lt 9 (v 8) (c 4)],br 9 6 8⟩,
  ⟨[],[add 10 (v 8) (c 1),add 11 (v 10) (v 10),add 12 (v 11) base,
    add 13 (v 8) (v 8),add 14 (v 13) base] ++ reserveInstructions 15,br 19 9 7⟩,
  ⟨[],[mstore 64 (v 16),load 20 (v 12),mask 21 (v 20),add 22 (v 12) (c 1),
    load 23 (v 22),store (v 14) (v 21),add 24 (v 14) (c 1),store (v 24) (v 23)],.jump 5⟩,
  clearBlock site 25,panicBlock]

def pointerBlocks (site : Nat) (recoveryShift : Word) : List State.Block := [
  ⟨[],[add 0 base (c 10)],.jump 1⟩,
  ⟨[⟨1,.word 256,[(0,base),(3,v 6)]⟩],
    [⟨some 2,.word 1,none,.notEqual,[v 0,v 1]⟩],br 2 2 10⟩,
  ⟨[],[load 3 (v 1),mask 4 (v 3),matchesNeedle 5 (v 4)],br 5 4 3⟩,
  ⟨[],[add 6 (v 1) (c 2)],.jump 1⟩,
  ⟨[],[op 7 0x03 (.binary .sub 256) [v 1,base],op 8 0x1c (.shift .shr 256) [c recoveryShift,v 7],
    op 9 0x1b (.shift .shl 256) [c 1,v 8],add 10 base (v 9)],.jump 5⟩,
  ⟨[⟨11,.word 256,[(4,v 8),(7,v 14)]⟩,⟨12,.word 256,[(4,v 10),(7,v 26)]⟩],
    [lt 13 (v 11) (c 4)],br 13 6 8⟩,
  ⟨[],[add 14 (v 11) (c 1),add 15 (v 12) (c 2)] ++ reserveInstructions 16,br 20 9 7⟩,
  ⟨[],[mstore 64 (v 17),load 21 (v 15),mask 22 (v 21),add 23 (v 12) (c 3),
    load 24 (v 23),store (v 12) (v 22),add 25 (v 12) (c 1),store (v 25) (v 24),
    add 26 (v 12) (c 2)],.jump 5⟩,
  clearBlock site 27,panicBlock]

end

end SolarTV.MIR.StorageInduction
