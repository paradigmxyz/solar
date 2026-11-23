import SolarTV.EVMIR.State.Interpreter

/-! An instruction-aligned byte image with small independently checked leaves.
Its indexed byte accesses and jump checks implement the ordinary list
decoder. Keeping that correspondence outside execution certificates avoids
normalizing a large deployed image at every instruction. -/

namespace SolarTV.EVM.State
open EvmRules SolarTV.Execution

private def scanBlock : Nat → Nat → List Byte → Option (Nat × List Nat)
  | _, _, [] => some (0,[])
  | fuel+1, pc, byte :: rest => do
      let skip := pushWidth byte.toNat
      if rest.length < skip then none else
        let (used,jumps) ← scanBlock fuel (pc+1+skip) (rest.drop skip)
        return (used+1, if byte.toNat = 0x5b then pc :: jumps else jumps)
  | 0, _, _ => none

private theorem scanBlock_append (bound pc used fuel : Nat) (bytes suffix : List Byte)
    (jumps : List Nat) (checked : scanBlock bound pc bytes = some (used,jumps))
    (enough : used ≤ fuel) :
    scanJumps fuel pc (bytes ++ suffix) =
      jumps ++ scanJumps (fuel-used) (pc+bytes.length) suffix := by
  induction bound generalizing pc bytes used jumps fuel with
  | zero =>
    cases bytes with
    | nil => obtain ⟨rfl, rfl⟩ := Option.some.inj checked; simp
    | cons _ _ => contradiction
  | succ bound ih =>
    cases bytes with
    | nil => obtain ⟨rfl, rfl⟩ := Option.some.inj checked; simp
    | cons byte rest =>
      simp only [scanBlock] at checked
      split at checked
      · contradiction
      rename_i fits
      cases h : scanBlock bound (pc+1+pushWidth byte.toNat) (rest.drop (pushWidth byte.toNat)) with
      | none => simp [h] at checked
      | some result =>
        obtain ⟨count,next⟩ := result
        simp only [h, Option.pure_def] at checked
        obtain ⟨rfl, rfl⟩ := checked
        cases fuel with
        | zero => omega
        | succ n =>
          have scan := ih _ _ _ _ _ h (by omega : count ≤ n)
          simp only [List.cons_append, scanJumps, List.drop_append_of_le_length (by omega : pushWidth byte.toNat ≤ rest.length)]
          rw [scan]
          have pos : pc + 1 + pushWidth byte.toNat + (rest.drop (pushWidth byte.toNat)).length =
              pc + (byte :: rest).length := by
            simp only [List.length_drop, List.length_cons]
            omega
          rw [pos, Nat.succ_sub_succ_eq_sub]
          split <;> rfl

/-- Each leaf must end at an instruction boundary. The numerical proofs are
constructed over the leaf's bytes only. -/
structure CodeLeaf where
  bytes : List Byte
  size : Nat
  size_eq : bytes.length = size
  used : Nat
  jumps : List Nat
  checked : scanBlock size 0 bytes = some (used,jumps)

private theorem scanBlock_le (bound pc used : Nat) (bytes : List Byte) (jumps : List Nat)
    (checked : scanBlock bound pc bytes = some (used,jumps)) :
    used ≤ bytes.length := by
  induction bound generalizing pc used bytes jumps with
  | zero => cases bytes <;> simp_all [scanBlock]
  | succ bound ih =>
    cases bytes with
    | nil => simp_all [scanBlock]
    | cons byte rest =>
      simp only [scanBlock] at checked
      split at checked
      · contradiction
      cases h : scanBlock bound (pc+1+pushWidth byte.toNat) (rest.drop (pushWidth byte.toNat)) with
      | none => simp [h] at checked
      | some result =>
        obtain ⟨count,next⟩ := result
        simp only [h, Option.pure_def] at checked
        obtain ⟨rfl, rfl⟩ := checked
        have le := ih _ _ _ _ h
        simp only [List.length_drop, List.length_cons] at *
        omega

private theorem scanBlock_rebase (bound start : Nat) (bytes : List Byte) :
    scanBlock bound start bytes =
      (scanBlock bound 0 bytes).map (fun (used,jumps) => (used,jumps.map (start+·))) := by
  induction bound generalizing start bytes with
  | zero => cases bytes <;> rfl
  | succ bound ih =>
    cases bytes with
    | nil => rfl
    | cons byte rest =>
      simp only [scanBlock, Nat.zero_add]
      split
      · rfl
      rw [ih (start+1+pushWidth byte.toNat), ih (1+pushWidth byte.toNat)]
      cases h : scanBlock bound 0 (rest.drop (pushWidth byte.toNat)) with
      | none => simp
      | some result =>
        obtain ⟨used,jumps⟩ := result
        by_cases jump : byte.toNat = 0x5b
        all_goals
          simp [jump, Option.pure_def, List.map_map, Function.comp_def, Nat.add_assoc]

inductive CodeChunks where
  | leaf (block : CodeLeaf)
  | append (left right : CodeChunks)

namespace CodeChunks

def bytes : CodeChunks → List Byte
  | .leaf block => block.bytes
  | .append left right => left.bytes ++ right.bytes

def size : CodeChunks → Nat
  | .leaf block => block.size
  | .append left right => left.size + right.size

def used : CodeChunks → Nat
  | .leaf block => block.used
  | .append left right => left.used + right.used

def get? : CodeChunks → Nat → Option Byte
  | .leaf block, pc => block.bytes[pc]?
  | .append left right, pc =>
    if pc < left.size then left.get? pc else right.get? (pc-left.size)

def jumps : CodeChunks → Nat → List Nat
  | .leaf block, pc => block.jumps.map (pc+·)
  | .append left right, pc => left.jumps pc ++ right.jumps (pc+left.size)

theorem size_eq (code : CodeChunks) : code.bytes.length = code.size := by
  induction code <;> simp [bytes, size, CodeLeaf.size_eq, *]

theorem get_eq (code : CodeChunks) (pc : Nat) : code.get? pc = code.bytes[pc]? := by
  induction code generalizing pc with
  | leaf _ => rfl
  | append left right hl hr =>
    simp only [get?, bytes, List.getElem?_append, size_eq]
    split <;> simp_all

theorem used_le (code : CodeChunks) : code.used ≤ code.size := by
  induction code with
  | leaf block =>
    simpa [used,size,block.size_eq] using scanBlock_le _ _ _ _ _ block.checked
  | append left right hl hr => exact Nat.add_le_add hl hr

theorem jumps_append (code : CodeChunks) (offset fuel : Nat) (suffix : List Byte)
    (enough : code.used ≤ fuel) :
    scanJumps fuel offset (code.bytes ++ suffix) =
      code.jumps offset ++ scanJumps (fuel-code.used) (offset+code.size) suffix := by
  induction code generalizing offset fuel suffix with
  | leaf block =>
    have checked : scanBlock block.size offset block.bytes =
        some (block.used,block.jumps.map (offset+·)) := by
      rw [scanBlock_rebase,block.checked]
      rfl
    simpa only [bytes,jumps,used,size,block.size_eq] using
      scanBlock_append _ _ _ _ _ _ _ checked enough
  | append left right hl hr =>
    simp only [used] at enough
    simp only [bytes,List.append_assoc]
    rw [hl _ _ _ (by omega : left.used ≤ fuel)]
    rw [hr _ _ _ (by omega : right.used ≤ fuel-left.used)]
    simp only [jumps,size,used,List.append_assoc,Nat.add_assoc]
    rw [Nat.sub_sub]

private theorem scanEmpty (fuel pc : Nat) : scanJumps fuel pc [] = [] := by
  cases fuel <;> rfl

theorem jump_eq (code : CodeChunks) (target : Word) :
    (code.jumps 0).contains target.toNat = validJump code.bytes target := by
  have whole := jumps_append code 0 code.size [] code.used_le
  simpa only [List.append_nil, scanEmpty, validJump, jumpDestinations, List.drop_zero, size_eq] using
    congrArg (·.contains target.toNat) whole.symm

private theorem scanJumps_lower (fuel pc : Nat) (bytes : List Byte) (target : Nat)
    (found : target ∈ scanJumps fuel pc bytes) : pc ≤ target := by
  induction fuel generalizing pc bytes with
  | zero => simp [scanJumps] at found
  | succ fuel ih =>
    cases bytes with
    | nil => simp [scanJumps] at found
    | cons byte rest =>
      simp only [scanJumps] at found
      split at found
      · rcases List.mem_cons.mp found with equal | later
        · omega
        · have bound := ih _ _ later
          omega
      · have bound := ih _ _ found
        omega

/-- Opaque assembly data follows the instruction stream. Any jump destination
in that suffix has a larger offset than every byte in the checked code. -/
theorem jump_eq_with_data (code : CodeChunks) (data : List Byte) (target : Word)
    (inside : target.toNat < code.size) :
    (code.jumps 0).contains target.toNat = validJump (code.bytes ++ data) target := by
  unfold validJump jumpDestinations
  rw [List.drop_zero, jumps_append code 0 _ _ (by simp [size_eq]; exact Nat.le_trans code.used_le (Nat.le_add_right ..))]
  have absent : target.toNat ∉
      scanJumps (code.bytes.length + data.length - code.used) code.size data :=
    fun found => by have := scanJumps_lower _ _ _ _ found; omega
  simp [absent]

def immediate (code : CodeChunks) (pc count : Nat) : Word :=
  ((List.range count).map (fun i => (code.get? (pc+1+i)).getD 0)).foldl
    (fun value byte => (value <<< 8) ||| byte.setWidth 256) 0

theorem immediate_eq (code : CodeChunks) (pc count : Nat) :
    code.immediate pc count = pushValue code.bytes pc count := by
  simp only [immediate, pushValue, readBytes, get_eq]

end CodeChunks
end SolarTV.EVM.State
