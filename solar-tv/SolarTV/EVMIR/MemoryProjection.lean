import SolarTV.Memory
import SolarTV.State.MemoryLaws
import SolarTV.EVMIR.Memory
import SolarTV.EVMIR.Gas
import SolarTV.EVMIR.State.Interpreter

/-! Relate a chosen byte-memory prefix to full frame execution.
Read footprints stay explicit, including zero-length reads at arbitrary offsets.
Successful expansion preserves zero-extended reads and the full active-word
count. Fetched MLOAD, KECCAK256, RETURN and REVERT retain their actual gas
charges, resource failures and model errors. Funded MLOAD also commutes with
the local stack/memory step under frame alignment and the 64 KiB model bound. -/

namespace SolarTV.Memory.Projection
open EvmRules
set_option maxRecDepth 4096

variable (limit : Nat)

def project (memory : List Execution.Byte) : Memory.State :=
  ⟨(List.range limit).map (fun index => memory[index]?.getD 0),
    BitVec.ofNat 251 (Execution.memoryWords memory)⟩

theorem read_project (memory : List Execution.Byte) (offset count : Nat)
    (within : count = 0 ∨ offset + count ≤ limit) :
    Memory.read (project limit memory) offset count = Execution.readBytes memory offset count := by
  rcases within with rfl | within
  · rfl
  · unfold Memory.read Execution.readBytes
    apply List.map_congr_left
    intro index present
    have inside : offset + index < limit := by
      have := List.mem_range.mp present
      omega
    have atIndex : (List.range limit)[offset + index]? = some (offset + index) := by
      simp [List.getElem?_range inside]
    simp [project, List.getElem?_map, atIndex]

private theorem getD_append_zeros (memory : List Execution.Byte) (count index : Nat) :
    (memory ++ List.replicate count 0)[index]?.getD 0 = memory[index]?.getD 0 := by
  induction memory generalizing index with
  | nil =>
    simp only [List.nil_append, List.getElem?_replicate, List.getElem?_nil]
    split <;> rfl
  | cons head tail ih =>
    cases index with
    | zero => rfl
    | succ index => exact ih index

/-- Expanding active memory leaves every zero-extended read unchanged. -/
theorem read_expanded (memory grown : List Execution.Byte) (offset count source size : Nat)
    (expanded : Execution.expand memory offset count = .ok grown) :
    Execution.readBytes grown source size = Execution.readBytes memory source size := by
  dsimp only [Execution.expand] at expanded
  split at expanded
  · contradiction
  · cases expanded
    unfold Execution.readBytes
    apply List.map_congr_left
    intro index _
    exact getD_append_zeros memory _ _

theorem load_expanded (memory grown : List Execution.Byte) (offset count source : Nat)
    (expanded : Execution.expand memory offset count = .ok grown) :
    Execution.readWord grown source = Execution.readWord memory source := by
  unfold Execution.readWord
  rw [read_expanded memory grown offset count source 32 expanded]

theorem projected_bytes_expanded (memory grown : List Execution.Byte) (offset count : Nat)
    (expanded : Execution.expand memory offset count = .ok grown) :
    (project limit grown).bytes = (project limit memory).bytes := by
  simpa [project, Execution.readBytes] using
    read_expanded memory grown offset count 0 limit expanded

theorem words_expanded (memory grown : List Execution.Byte) (offset count : Nat)
    (aligned : memory.length % 32 = 0)
    (expanded : Execution.expand memory offset count = .ok grown) :
    Execution.memoryWords grown = Execution.requiredWords memory offset count := by
  have old : memory.length ≤ 32 * Execution.requiredWords memory offset count := by
    unfold Execution.requiredWords Execution.memoryWords
    split <;> omega
  dsimp only [Execution.expand] at expanded
  split at expanded
  · contradiction
  · cases expanded
    simp only [Execution.memoryWords, List.length_append, List.length_replicate]
    omega

theorem required_bound (memory grown : List Execution.Byte) (offset count : Nat)
    (expanded : Execution.expand memory offset count = .ok grown) :
    Execution.requiredWords memory offset count ≤ 2048 := by
  dsimp only [Execution.expand] at expanded
  split at expanded
  · contradiction
  · omega

/-- Equal projections of two expansions from the same aligned heap also
preserve every unprojected byte. Equality of the active size fixes the zero
suffix appended to the common original heap. -/
theorem expanded_eq_of_project_eq (memory first second : List Execution.Byte)
    (firstOffset firstCount secondOffset secondCount : Nat)
    (aligned : memory.length % 32 = 0)
    (left : Execution.expand memory firstOffset firstCount = .ok first)
    (right : Execution.expand memory secondOffset secondCount = .ok second)
    (equal : project limit first = project limit second) : first = second := by
  have firstWords := words_expanded memory first firstOffset firstCount aligned left
  have secondWords := words_expanded memory second secondOffset secondCount aligned right
  have firstFits : Execution.memoryWords first < 2^251 := by
    have := required_bound memory first firstOffset firstCount left
    omega
  have secondFits : Execution.memoryWords second < 2^251 := by
    have := required_bound memory second secondOffset secondCount right
    omega
  have wordEquality := congrArg (fun state => state.words.toNat) equal
  have sameWords : Execution.memoryWords first = Execution.memoryWords second := by
    simpa only [project, BitVec.toNat_ofNat, Nat.mod_eq_of_lt firstFits,
      Nat.mod_eq_of_lt secondFits] using wordEquality
  have required : Execution.requiredWords memory firstOffset firstCount =
      Execution.requiredWords memory secondOffset secondCount := by omega
  have sameExpansion : Execution.expand memory firstOffset firstCount =
      Execution.expand memory secondOffset secondCount := by
    simp only [Execution.expand, required]
  rw [left, right] at sameExpansion
  exact Except.ok.inj sameExpansion

private theorem grow_ofNat (old required : Nat) (oldFits : old < 2^251)
    (newFits : required < 2^251) :
    Memory.grow (BitVec.ofNat 251 old) (BitVec.ofNat 251 required) =
      BitVec.ofNat 251 (max old required) := by
  unfold Memory.grow
  simp only [BitVec.ult_iff_toNat_lt, BitVec.toNat_ofNat,
    Nat.mod_eq_of_lt oldFits, Nat.mod_eq_of_lt newFits]
  by_cases less : old < required
  · simp [less, Nat.max_eq_right (Nat.le_of_lt less)]
  · simp [less, Nat.max_eq_left (Nat.le_of_not_lt less)]

/-- Under actual frame alignment, active size and zero-extended bytes commute
with every successful expansion, including accesses beyond the projection. -/
theorem expand_project (memory grown : List Execution.Byte) (offset count : Nat)
    (aligned : memory.length % 32 = 0) (bounded : memory.length ≤ 65536)
    (expanded : Execution.expand memory offset count = .ok grown) :
    project limit grown = Memory.expand (project limit memory) offset count := by
  have bytes := projected_bytes_expanded limit memory grown offset count expanded
  have words := words_expanded memory grown offset count aligned expanded
  have oldFits : Execution.memoryWords memory < 2^251 := by
    unfold Execution.memoryWords
    omega
  by_cases zero : count = 0
  · subst count
    simp only [Memory.expand, ite_true]
    unfold project
    congr 1
    simp only [words, Execution.requiredWords, ite_true]
  · have requiredBound := required_bound memory grown offset count expanded
    have newFits : (offset + count + 31) / 32 < 2^251 := by
      unfold Execution.requiredWords at requiredBound
      simp only [zero, ite_false] at requiredBound
      omega
    simp only [Memory.expand, zero, ite_false]
    unfold project
    congr 1
    simp only [words, Execution.requiredWords, zero, ite_false]
    exact (grow_ofNat _ _ oldFits newFits).symm

/-- The local active-word ledger charges exactly the frame's quadratic memory
expansion cost. Successful full-memory expansion proves that neither word
count wraps its representation. -/
theorem expansion_charge (memory grown : List Execution.Byte) (offset count : Nat)
    (aligned : memory.length % 32 = 0) (bounded : memory.length ≤ 65536)
    (expanded : Execution.expand memory offset count = .ok grown) :
    EVM.Gas.expansionCost (project limit memory) (Memory.expand (project limit memory) offset count) =
      EVM.State.memoryCharge memory offset count := by
  have oldFits : Execution.memoryWords memory < 2^251 := by
    unfold Execution.memoryWords
    omega
  have newFits : Execution.requiredWords memory offset count < 2^251 := by
    have := required_bound memory grown offset count expanded
    omega
  rw [← expand_project limit memory grown offset count aligned bounded expanded]
  simp only [EVM.Gas.expansionCost, project, BitVec.toNat_ofNat,
    Nat.mod_eq_of_lt oldFits, Nat.mod_eq_of_lt newFits,
    words_expanded memory grown offset count aligned expanded,
    EVM.Gas.memoryCost, EVM.State.memoryCharge, EVM.State.memoryCost]

theorem load_charge (memory grown : List Execution.Byte) (offset : Word) (stack : EVM.Stack)
    (fork : Fork) (aligned : memory.length % 32 = 0) (bounded : memory.length ≤ 65536)
    (expanded : Execution.expand memory offset.toNat 32 = .ok grown) :
    EVM.Gas.cost .load (offset :: stack) (project limit memory) fork =
      3 + EVM.State.memoryCharge memory offset.toNat 32 := by
  simpa only [EVM.Gas.cost, EVM.Gas.baseCost, EVM.Gas.touched, Nat.add_zero] using
    congrArg (3 + ·) (expansion_charge limit memory grown offset.toNat 32 aligned bounded expanded)

private theorem append_word (high : BitVec upper) (low : BitVec lower) :
    (high ++ low).setWidth 256 = (high.setWidth 256 <<< lower) ||| low.setWidth 256 := by
  apply BitVec.eq_of_getLsbD_eq
  intro index inside
  simp only [BitVec.getLsbD_setWidth, BitVec.getLsbD_append,
    BitVec.getLsbD_or, BitVec.getLsbD_shiftLeft]
  by_cases small : index < lower
  · simp [inside, small]
  · have shifted : index - lower < 256 := by omega
    simp [inside, small, shifted,
      BitVec.getLsbD_of_ge low index (Nat.le_of_not_lt small)]

private theorem fold_word (bytes : List Execution.Byte) (size : bytes.length = 32) :
    bytes.foldl (fun (word : Word) byte => (word <<< 8) ||| byte.setWidth 256) 0 =
      (BitVec.flattenListFast bytes).setWidth 256 := by
  have positions : (List.range 32).map (fun index => bytes[index]?.getD 0) = bytes := by
    apply List.ext_getElem
    · simp [size]
    · intro index left right
      simp [List.getElem?_eq_getElem right]
  rw [← positions]
  rw [← BitVec.flattenList_eq_flattenListFast]
  dsimp only [List.range, List.range.loop, List.map, List.foldl, List.length, BitVec.flattenList]
  simp only [BitVec.setWidth_cast, append_word, BitVec.setWidth_zero,
    BitVec.shiftLeft_or_distrib, ← BitVec.shiftLeft_add, BitVec.or_assoc,
    BitVec.or_zero, BitVec.shiftLeft_zero]
  simp

theorem load_project (memory : List Execution.Byte) (offset : Nat)
    (within : offset + 32 ≤ limit) :
    Memory.load (project limit memory) offset = Execution.readWord memory offset := by
  unfold Memory.load Execution.readWord
  rw [read_project limit memory offset 32 (Or.inr within)]
  exact fold_word _ (by simp [Execution.readBytes])

theorem hash_project (memory : List Execution.Byte) (offset count : Nat)
    (within : count = 0 ∨ offset + count ≤ limit) :
    Memory.hash (project limit memory) offset count = Keccak.hash (Execution.readBytes memory offset count) := by
  unfold Memory.hash
  rw [read_project limit memory offset count within]

theorem load_step (context : EVM.Context) (memory : List Execution.Byte)
    (offset : Word) (stack : EVM.Stack)
    (within : offset.toNat + 32 ≤ limit) (fits : (offset :: stack).length ≤ 1024) :
    EVM.Memory.step context .load (offset :: stack) (project limit memory) =
      .ok (Execution.readWord memory offset.toNat :: stack,
        Memory.expand (project limit memory) offset.toNat 32) := by
  simp only [EVM.Memory.step, show ¬(offset :: stack).length > 1024 by omega,
    ite_false]
  rw [load_project limit memory offset.toNat within]

/-- The raw MLOAD step uses the same projected word, retaining its actual
memory-expansion charge, gas failure and model-limit result. -/
theorem load_opcode (config : EVM.State.Config) (world : Execution.Journal)
    (frame : EVM.State.Frame) (offset : Word) (stack : EVM.Stack)
    (base dynamic codeLength : Nat) (immediate : Nat → Word) (jump : Word → Bool)
    (opcode : EVM.State.available config 0x51 = ⟨true,base,dynamic,1,1⟩)
    (present : frame.pc < codeLength) (words : frame.stack = offset :: stack)
    (within : offset.toNat + 32 ≤ limit) (fits : (offset :: stack).length ≤ 1024) :
    EVM.State.instructionWithImage config world frame codeLength 0x51 immediate jump =
      EVM.State.withMemory world frame base offset.toNat 32 (fun grown =>
        EVM.State.next world grown (Memory.load (project limit grown.memory) offset.toNat :: stack)) := by
  have notPast : ¬frame.pc ≥ codeLength := by omega
  have height : ¬stack.length + 1 > 1024 := by
    simp only [List.length_cons] at fits
    omega
  simp [EVM.State.instructionWithImage, opcode, notPast, words, height,
    EVM.State.pureOp, load_project, within]

theorem load_instruction (config : EVM.State.Config) (world : Execution.Journal)
    (frame : EVM.State.Frame) (offset : Word) (stack : EVM.Stack) (base dynamic : Nat)
    (opcode : EVM.State.available config 0x51 = ⟨true,base,dynamic,1,1⟩)
    (fetched : frame.code[frame.pc]? = some (0x51 : Execution.Byte))
    (words : frame.stack = offset :: stack) (within : offset.toNat + 32 ≤ limit)
    (fits : (offset :: stack).length ≤ 1024) :
    EVM.State.instruction config world frame =
      EVM.State.withMemory world frame base offset.toNat 32 (fun grown =>
        EVM.State.next world grown (Memory.load (project limit grown.memory) offset.toNat :: stack)) := by
  have present : frame.pc < frame.code.length := (List.getElem?_eq_some_iff.mp fetched).1
  simpa only [EVM.State.instruction, fetched, Option.getD_some] using
    load_opcode limit config world frame offset stack base dynamic frame.code.length
      (EVM.State.pushValue frame.code frame.pc) (EVM.State.validJump frame.code)
      opcode present words within fits

/-- A funded fetched MLOAD and the local operation reach the same stack and
projected memory. The physical step also retains its exact gas and PC. -/
theorem load_frame (config : EVM.State.Config) (world : Execution.Journal)
    (frame : EVM.State.Frame) (grown : List Execution.Byte)
    (offset : Word) (stack : EVM.Stack) (base dynamic : Nat)
    (opcode : EVM.State.available config 0x51 = ⟨true,base,dynamic,1,1⟩)
    (fetched : frame.code[frame.pc]? = some (0x51 : Execution.Byte))
    (words : frame.stack = offset :: stack) (within : offset.toNat + 32 ≤ limit)
    (fits : (offset :: stack).length ≤ 1024)
    (aligned : frame.memory.length % 32 = 0) (bounded : frame.memory.length ≤ 65536)
    (expanded : Execution.expand frame.memory offset.toNat 32 = .ok grown)
    (funded : base + EVM.State.memoryCharge frame.memory offset.toNat 32 ≤ frame.gas) :
    let after := {frame with
      pc := frame.pc + 1
      stack := Execution.readWord frame.memory offset.toNat :: stack
      memory := grown
      gas := frame.gas - (base + EVM.State.memoryCharge frame.memory offset.toNat 32)}
    EVM.State.instruction config world frame = .ok (.next world after) ∧
      EVM.Memory.step (.ofEnvironment frame.environment frame.returnData) .load
        frame.stack (project limit frame.memory) = .ok (after.stack, project limit after.memory) := by
  dsimp only
  constructor
  · rw [load_instruction limit config world frame offset stack base dynamic opcode fetched words within fits]
    have enough : ¬frame.gas < base + EVM.State.memoryCharge frame.memory offset.toNat 32 := by omega
    simp [EVM.State.withMemory, enough, expanded, EVM.State.next, Bind.bind, Except.bind,
      load_project, within, load_expanded frame.memory grown offset.toNat 32 offset.toNat expanded]
  · rw [words, load_step limit _ frame.memory offset stack within fits,
      ← expand_project limit frame.memory grown offset.toNat 32 aligned bounded expanded]

theorem hash_opcode (config : EVM.State.Config) (world : Execution.Journal)
    (frame : EVM.State.Frame) (offset count : Word) (stack : EVM.Stack)
    (base dynamic codeLength : Nat) (immediate : Nat → Word) (jump : Word → Bool)
    (opcode : EVM.State.available config 0x20 = ⟨true,base,dynamic,2,1⟩)
    (present : frame.pc < codeLength) (words : frame.stack = offset :: count :: stack)
    (within : count.toNat = 0 ∨ offset.toNat + count.toNat ≤ limit)
    (fits : (offset :: count :: stack).length ≤ 1024) :
    EVM.State.instructionWithImage config world frame codeLength 0x20 immediate jump =
      EVM.State.withMemory world frame (base + dynamic * ((count.toNat + 31) / 32))
        offset.toNat count.toNat (fun grown => EVM.State.next world grown
          (Memory.hash (project limit grown.memory) offset.toNat count.toNat :: stack)) := by
  have notPast : ¬frame.pc ≥ codeLength := by omega
  have height : ¬stack.length + 1 + 1 < 2 := by omega
  have resultHeight : ¬stack.length + 1 > 1024 := by
    simp only [List.length_cons] at fits
    omega
  simp [EVM.State.instructionWithImage, opcode, notPast, words, height, resultHeight,
    EVM.State.pureOp, hash_project, within]

theorem hash_instruction (config : EVM.State.Config) (world : Execution.Journal)
    (frame : EVM.State.Frame) (offset count : Word) (stack : EVM.Stack) (base dynamic : Nat)
    (opcode : EVM.State.available config 0x20 = ⟨true,base,dynamic,2,1⟩)
    (fetched : frame.code[frame.pc]? = some (0x20 : Execution.Byte))
    (words : frame.stack = offset :: count :: stack)
    (within : count.toNat = 0 ∨ offset.toNat + count.toNat ≤ limit)
    (fits : (offset :: count :: stack).length ≤ 1024) :
    EVM.State.instruction config world frame =
      EVM.State.withMemory world frame (base + dynamic * ((count.toNat + 31) / 32))
        offset.toNat count.toNat (fun grown => EVM.State.next world grown
          (Memory.hash (project limit grown.memory) offset.toNat count.toNat :: stack)) := by
  have present : frame.pc < frame.code.length := (List.getElem?_eq_some_iff.mp fetched).1
  simpa only [EVM.State.instruction, fetched, Option.getD_some] using
    hash_opcode limit config world frame offset count stack base dynamic frame.code.length
      (EVM.State.pushValue frame.code frame.pc) (EVM.State.validJump frame.code)
      opcode present words within fits

theorem output_opcode (config : EVM.State.Config) (world : Execution.Journal)
    (frame : EVM.State.Frame) (offset count : Word) (stack : EVM.Stack) (reverted : Bool)
    (base dynamic codeLength : Nat) (immediate : Nat → Word) (jump : Word → Bool)
    (opcode : EVM.State.available config (if reverted then 0xfd else 0xf3) =
      ⟨true,base,dynamic,2,0⟩)
    (present : frame.pc < codeLength) (words : frame.stack = offset :: count :: stack)
    (within : count.toNat = 0 ∨ offset.toNat + count.toNat ≤ limit)
    (fits : (offset :: count :: stack).length ≤ 1024) :
    EVM.State.instructionWithImage config world frame codeLength
        (if reverted then 0xfd else 0xf3) immediate jump =
      EVM.State.withMemory world frame base offset.toNat count.toNat (fun grown =>
        .ok (.done ⟨if reverted then .reverted else .success,
          Memory.read (project limit grown.memory) offset.toNat count.toNat, world, grown.gas, 0⟩)) := by
  have notPast : ¬frame.pc ≥ codeLength := by omega
  have height : ¬stack.length + 1 + 1 < 2 := by omega
  have resultHeight : ¬stack.length > 1024 := by
    simp only [List.length_cons] at fits
    omega
  cases reverted <;> simp only [Bool.false_eq_true, ↓reduceIte] at opcode ⊢ <;>
    simp [EVM.State.instructionWithImage, opcode, notPast, words, height, resultHeight,
      EVM.State.pureOp, read_project, within]

theorem output_instruction (config : EVM.State.Config) (world : Execution.Journal)
    (frame : EVM.State.Frame) (offset count : Word) (stack : EVM.Stack) (reverted : Bool)
    (base dynamic : Nat)
    (opcode : EVM.State.available config (if reverted then 0xfd else 0xf3) =
      ⟨true,base,dynamic,2,0⟩)
    (fetched : frame.code[frame.pc]? = some (if reverted then 0xfd else 0xf3 : Execution.Byte))
    (words : frame.stack = offset :: count :: stack)
    (within : count.toNat = 0 ∨ offset.toNat + count.toNat ≤ limit)
    (fits : (offset :: count :: stack).length ≤ 1024) :
    EVM.State.instruction config world frame =
      EVM.State.withMemory world frame base offset.toNat count.toNat (fun grown =>
        .ok (.done ⟨if reverted then .reverted else .success,
          Memory.read (project limit grown.memory) offset.toNat count.toNat, world, grown.gas, 0⟩)) := by
  have present : frame.pc < frame.code.length := (List.getElem?_eq_some_iff.mp fetched).1
  simpa only [EVM.State.instruction, fetched, Option.getD_some] using
    output_opcode limit config world frame offset count stack reverted base dynamic frame.code.length
      (EVM.State.pushValue frame.code frame.pc) (EVM.State.validJump frame.code)
      opcode present words within fits

end SolarTV.Memory.Projection
