import SolarTV.EVMIR.MemoryProjection

/-! Project frame writes and their gas charges onto the local memory model.
MCOPY reads an immutable snapshot, including when the source and destination
overlap. A bounded write preserves every byte outside the projection. Equal
projections and equal tails recover the complete frame memory. -/

namespace SolarTV.Memory.Projection
open EvmRules
set_option maxRecDepth 4096

variable (limit : Nat)

/-- Equality of the active size and retained bytes can be lifted back to the
full frame when each execution preserves the same bytes outside the prefix. -/
theorem full_eq_of_project_and_tail (left right : List Execution.Byte)
    (leftAligned : left.length % 32 = 0) (rightAligned : right.length % 32 = 0)
    (leftBounded : left.length ≤ 65536) (rightBounded : right.length ≤ 65536)
    (projected : project limit left = project limit right)
    (tail : ∀ start count, limit ≤ start → Execution.readBytes left start count =
      Execution.readBytes right start count) : left = right := by
  have leftFits : Execution.memoryWords left < 2^251 := by unfold Execution.memoryWords; omega
  have rightFits : Execution.memoryWords right < 2^251 := by unfold Execution.memoryWords; omega
  have sameWords : Execution.memoryWords left = Execution.memoryWords right := by
    have size := congrArg (fun state => state.words.toNat) projected
    simpa only [project, BitVec.toNat_ofNat, Nat.mod_eq_of_lt leftFits,
      Nat.mod_eq_of_lt rightFits] using size
  have sameLength : left.length = right.length := by
    unfold Execution.memoryWords at sameWords
    omega
  apply List.ext_getElem sameLength
  intro index inLeft inRight
  have readEq : Execution.readBytes left index 1 = Execution.readBytes right index 1 := by
    by_cases retained : index < limit
    · have inside : 1 = 0 ∨ index + 1 ≤ limit := Or.inr (by omega)
      rw [← read_project limit left index 1 inside, ← read_project limit right index 1 inside, projected]
    · exact tail index 1 (by omega)
  simpa [Execution.readBytes, show List.range 1 = [0] from rfl,
    List.getElem?_eq_getElem inLeft, List.getElem?_eq_getElem inRight] using readEq

theorem write_bytes_project (memory bytes : List Execution.Byte) (offset : Nat)
    (active : offset + bytes.length ≤ memory.length) :
    project limit (Execution.writeBytes memory offset bytes) =
      {(project limit memory) with
        bytes := Execution.writeBytes (project limit memory).bytes offset bytes} := by
  simp only [project, Execution.memoryWords, Execution.writeBytes_length]
  congr 1
  rw [Execution.writeBytes_eq_spec, Execution.writeBytes_eq_spec]
  apply List.ext_getElem
  · simp [Execution.writeBytesSpec]
  · intro index before after
    have inside : index < limit := by simpa using before
    by_cases live : index < memory.length
    · simp [Execution.writeBytesSpec, List.getElem?_mapIdx,
        List.getElem?_eq_getElem live]
    · have outside : ¬(offset ≤ index ∧ index < offset + bytes.length) := by omega
      simp [Execution.writeBytesSpec, List.getElem?_mapIdx, outside,
        List.getElem?_eq_none (Nat.le_of_not_lt live)]

theorem write_word_bytes (bytes : List Execution.Byte) (offset : Nat) (value : Word) :
    Execution.writeBytes bytes offset (Execution.wordBytes value) =
      Memory.writeWord bytes offset value := by
  rw [Execution.writeBytes_eq_spec]
  unfold Execution.writeBytesSpec Memory.writeWord
  congr 1
  funext index old
  simp only [Execution.wordBytes_length]
  by_cases within : offset ≤ index ∧ index < offset + 32
  · have part : index - offset < 32 := by omega
    simp [within, Execution.wordBytes, Memory.byte, List.getElem?_range part]
  · simp [within]

theorem write_byte_bytes (bytes : List Execution.Byte) (offset : Nat) (value : Execution.Byte) :
    Execution.writeBytes bytes offset [value] = bytes.set offset value := by
  rw [Execution.writeBytes_eq_spec]
  apply List.ext_getElem?
  intro index
  by_cases inside : index < bytes.length
  · by_cases same : offset = index
    · subst index
      simp [Execution.writeBytesSpec, inside]
    · have outside : ¬(offset ≤ index ∧ index < offset + 1) := by omega
      simp [Execution.writeBytesSpec, same, outside,
        List.getElem?_eq_getElem inside]
  · have outside : bytes.length ≤ index := by omega
    simp [Execution.writeBytesSpec, List.getElem?_eq_none outside,
      List.getElem?_eq_none (by simpa using outside : (bytes.set offset value).length ≤ index)]

theorem expanded_active (memory grown : List Execution.Byte) (offset count : Nat)
    (positive : 0 < count) (expanded : Execution.expand memory offset count = .ok grown) :
    offset + count ≤ grown.length := by
  have required : offset + count ≤ 32 * Execution.requiredWords memory offset count := by
    unfold Execution.requiredWords
    simp only [show count ≠ 0 by omega, ite_false]
    omega
  dsimp only [Execution.expand] at expanded
  split at expanded
  · contradiction
  · cases expanded
    simp only [List.length_append, List.length_replicate]
    omega

theorem store_project (memory grown : List Execution.Byte) (offset : Nat) (value : Word)
    (aligned : memory.length % 32 = 0) (bounded : memory.length ≤ 65536)
    (expanded : Execution.expand memory offset 32 = .ok grown) :
    Memory.store (project limit memory) offset value =
      project limit (Execution.writeBytes grown offset (Execution.wordBytes value)) := by
  rw [write_bytes_project limit grown (Execution.wordBytes value) offset
    (by simpa using expanded_active memory grown offset 32 (by decide) expanded)]
  rw [write_word_bytes]
  rw [expand_project limit memory grown offset 32 aligned bounded expanded]
  simp [Memory.store, Memory.expand]

theorem store_byte_project (memory grown : List Execution.Byte) (offset : Nat) (value : Word)
    (aligned : memory.length % 32 = 0) (bounded : memory.length ≤ 65536)
    (expanded : Execution.expand memory offset 1 = .ok grown) :
    Memory.storeByte (project limit memory) offset value =
      project limit (Execution.writeBytes grown offset [value.setWidth 8]) := by
  rw [write_bytes_project limit grown [value.setWidth 8] offset
    (by simpa using expanded_active memory grown offset 1 (by decide) expanded)]
  rw [write_byte_bytes]
  rw [expand_project limit memory grown offset 1 aligned bounded expanded]
  simp [Memory.storeByte, Memory.expand]

theorem store_charge (memory grown : List Execution.Byte) (offset value : Word) (stack : EVM.Stack)
    (isByte : Bool) (fork : Fork) (aligned : memory.length % 32 = 0) (bounded : memory.length ≤ 65536)
    (expanded : Execution.expand memory offset.toNat (if isByte then 1 else 32) = .ok grown) :
    EVM.Gas.cost (if isByte then .storeByte else .store) (offset :: value :: stack)
        (project limit memory) fork =
      3 + EVM.State.memoryCharge memory offset.toNat (if isByte then 1 else 32) := by
  cases isByte <;>
    simpa only [Bool.false_eq_true, ↓reduceIte, EVM.Gas.cost, EVM.Gas.baseCost,
      EVM.Gas.touched, Nat.add_zero] using
      congrArg (3 + ·) (expansion_charge limit memory grown offset.toNat _ aligned bounded expanded)

theorem bounded_write_tail (memory grown bytes : List Execution.Byte)
    (expandOffset expandCount destination source count : Nat)
    (expanded : Execution.expand memory expandOffset expandCount = .ok grown)
    (within : destination + bytes.length ≤ limit) (outside : limit ≤ source) :
    Execution.readBytes (Execution.writeBytes grown destination bytes) source count =
      Execution.readBytes memory source count := by
  rw [Execution.readBytes_writeBytes_disjoint grown bytes destination source count (Or.inl (by omega))]
  exact read_expanded memory grown expandOffset expandCount source count expanded

theorem store_opcode (config : EVM.State.Config) (world : Execution.Journal)
    (frame : EVM.State.Frame) (offset value : Word) (stack : EVM.Stack) (isByte : Bool)
    (base dynamic codeLength : Nat) (immediate : Nat → Word) (jump : Word → Bool)
    (opcode : EVM.State.available config (if isByte then 0x53 else 0x52) =
      ⟨true,base,dynamic,2,0⟩)
    (present : frame.pc < codeLength) (words : frame.stack = offset :: value :: stack)
    (fits : (offset :: value :: stack).length ≤ 1024) :
    EVM.State.instructionWithImage config world frame codeLength
        (if isByte then 0x53 else 0x52) immediate jump =
      EVM.State.withMemory world frame base offset.toNat (if isByte then 1 else 32) (fun grown =>
        EVM.State.next world {grown with
          memory := Execution.writeBytes grown.memory offset.toNat
            (if isByte then [value.setWidth 8] else Execution.wordBytes value)} stack) := by
  have notPast : ¬frame.pc ≥ codeLength := by omega
  have enough : ¬stack.length + 1 + 1 < 2 := by omega
  have height : ¬stack.length > 1024 := by
    simp only [List.length_cons] at fits
    omega
  cases isByte <;> simp only [Bool.false_eq_true, ↓reduceIte] at opcode ⊢ <;>
    simp [EVM.State.instructionWithImage, opcode, notPast, words, enough, height,
      EVM.State.pureOp]

theorem store_instruction (config : EVM.State.Config) (world : Execution.Journal)
    (frame : EVM.State.Frame) (offset value : Word) (stack : EVM.Stack) (isByte : Bool)
    (base dynamic : Nat)
    (opcode : EVM.State.available config (if isByte then 0x53 else 0x52) =
      ⟨true,base,dynamic,2,0⟩)
    (fetched : frame.code[frame.pc]? = some (if isByte then 0x53 else 0x52 : Execution.Byte))
    (words : frame.stack = offset :: value :: stack)
    (fits : (offset :: value :: stack).length ≤ 1024) :
    EVM.State.instruction config world frame =
      EVM.State.withMemory world frame base offset.toNat (if isByte then 1 else 32) (fun grown =>
        EVM.State.next world {grown with
          memory := Execution.writeBytes grown.memory offset.toNat
            (if isByte then [value.setWidth 8] else Execution.wordBytes value)} stack) := by
  have present : frame.pc < frame.code.length := (List.getElem?_eq_some_iff.mp fetched).1
  simpa only [EVM.State.instruction, fetched, Option.getD_some] using
    store_opcode config world frame offset value stack isByte base dynamic frame.code.length
      (EVM.State.pushValue frame.code frame.pc) (EVM.State.validJump frame.code)
      opcode present words fits

theorem store_frame (config : EVM.State.Config) (world : Execution.Journal)
    (frame : EVM.State.Frame) (grown : List Execution.Byte)
    (offset value : Word) (stack : EVM.Stack) (isByte : Bool) (base dynamic : Nat)
    (opcode : EVM.State.available config (if isByte then 0x53 else 0x52) =
      ⟨true,base,dynamic,2,0⟩)
    (fetched : frame.code[frame.pc]? = some (if isByte then 0x53 else 0x52 : Execution.Byte))
    (words : frame.stack = offset :: value :: stack)
    (fits : (offset :: value :: stack).length ≤ 1024)
    (aligned : frame.memory.length % 32 = 0) (bounded : frame.memory.length ≤ 65536)
    (within : offset.toNat + (if isByte then 1 else 32) ≤ limit)
    (expanded : Execution.expand frame.memory offset.toNat (if isByte then 1 else 32) = .ok grown) :
    let charge := base + EVM.State.memoryCharge frame.memory offset.toNat (if isByte then 1 else 32)
    let bytes := if isByte then [value.setWidth 8] else Execution.wordBytes value
    let after := {frame with
      pc := frame.pc + 1
      stack := stack
      memory := Execution.writeBytes grown offset.toNat bytes
      gas := frame.gas - charge}
    EVM.State.instruction config world frame =
      (if frame.gas < charge then EVM.State.stepExhausted world else .ok (.next world after)) ∧
      EVM.Memory.step (.ofEnvironment frame.environment frame.returnData)
        (if isByte then .storeByte else .store) frame.stack (project limit frame.memory) =
        .ok (after.stack, project limit after.memory) ∧
      ∀ source count, limit ≤ source → Execution.readBytes after.memory source count =
        Execution.readBytes frame.memory source count := by
  dsimp only
  constructor
  · rw [store_instruction config world frame offset value stack isByte base dynamic opcode fetched words fits]
    simp [EVM.State.withMemory, expanded, EVM.State.next, Bind.bind, Except.bind]
  · constructor
    · have height : ¬stack.length + 1 + 1 > 1024 := by
        simp only [List.length_cons] at fits
        omega
      cases isByte <;> simp only [Bool.false_eq_true, ↓reduceIte] at expanded ⊢
      · simp [EVM.Memory.step, words, height,
          store_project limit frame.memory grown offset.toNat value aligned bounded expanded]
      · simp [EVM.Memory.step, words, height,
          store_byte_project limit frame.memory grown offset.toNat value aligned bounded expanded]
    · intro source count outside
      apply bounded_write_tail limit frame.memory grown _ offset.toNat
        (if isByte then 1 else 32) offset.toNat source count expanded _ outside
      cases isByte <;> simpa using within

theorem expanded_length (memory grown : List Execution.Byte) (offset count : Nat)
    (aligned : memory.length % 32 = 0)
    (expanded : Execution.expand memory offset count = .ok grown) :
    grown.length = 32 * Execution.requiredWords memory offset count := by
  have old : memory.length ≤ 32 * Execution.requiredWords memory offset count := by
    unfold Execution.requiredWords Execution.memoryWords
    split <;> omega
  dsimp only [Execution.expand] at expanded
  split at expanded
  · contradiction
  · cases expanded
    simp only [List.length_append, List.length_replicate]
    omega

theorem expanded_aligned (memory grown : List Execution.Byte) (offset count : Nat)
    (aligned : memory.length % 32 = 0)
    (expanded : Execution.expand memory offset count = .ok grown) :
    grown.length % 32 = 0 := by
  rw [expanded_length memory grown offset count aligned expanded]
  omega

theorem expanded_bound (memory grown : List Execution.Byte) (offset count : Nat)
    (aligned : memory.length % 32 = 0)
    (expanded : Execution.expand memory offset count = .ok grown) : grown.length ≤ 65536 := by
  rw [expanded_length memory grown offset count aligned expanded]
  have := required_bound memory grown offset count expanded
  omega

theorem expanded_monotone (memory grown : List Execution.Byte) (offset count : Nat)
    (expanded : Execution.expand memory offset count = .ok grown) : memory.length ≤ grown.length := by
  dsimp only [Execution.expand] at expanded
  split at expanded
  · contradiction
  · cases expanded
    simp

theorem expand_zero (memory : List Execution.Byte) (offset : Nat)
    (aligned : memory.length % 32 = 0) (bounded : memory.length ≤ 65536) :
    Execution.expand memory offset 0 = .ok memory := by
  have length : 32 * (memory.length / 32) = memory.length := by omega
  simp [Execution.expand, Execution.requiredWords, Execution.memoryWords, length,
    show ¬memory.length > 65536 by omega]

private theorem grow_toNat (first second : BitVec 251) :
    (Memory.grow first second).toNat = max first.toNat second.toNat := by
  unfold Memory.grow
  split
  · rename_i lower
    have : first.toNat < second.toNat := by simpa [BitVec.ult_iff_toNat_lt] using lower
    simp [Nat.max_eq_right (Nat.le_of_lt this)]
  · rename_i lower
    have : second.toNat ≤ first.toNat := by simpa [BitVec.ult_iff_toNat_lt] using lower
    simp [Nat.max_eq_left this]

theorem grow_comm (words first second : BitVec 251) :
    Memory.grow (Memory.grow words first) second = Memory.grow (Memory.grow words second) first := by
  apply BitVec.eq_of_toNat_eq
  simp only [grow_toNat]
  omega

theorem copy_bytes (memory : Memory.State) (destination source count : Nat) :
    Execution.writeBytes memory.bytes destination (Memory.read memory source count) =
      memory.bytes.mapIdx (fun index old =>
        if destination ≤ index ∧ index < destination + count then
          memory.bytes[source + index - destination]?.getD 0 else old) := by
  rw [Execution.writeBytes_eq_spec]
  unfold Execution.writeBytesSpec
  simp only [Memory.read, List.length_map, List.length_range]
  congr 1
  funext index old
  by_cases within : destination ≤ index ∧ index < destination + count
  · have part : index - destination < count := by omega
    have address : source + (index - destination) = source + index - destination := by omega
    simp [within, List.getElem?_range part, address]
  · simp [within]

theorem copy_project (memory first grown : List Execution.Byte) (destination source count : Nat)
    (aligned : memory.length % 32 = 0) (bounded : memory.length ≤ 65536)
    (within : count = 0 ∨ source + count ≤ limit)
    (destinationGrowth : Execution.expand memory destination count = .ok first)
    (sourceGrowth : Execution.expand first source count = .ok grown) :
    Memory.copy (project limit memory) destination source count =
      project limit (Execution.writeBytes grown destination (Execution.readBytes grown source count)) := by
  by_cases zero : count = 0
  · subst count
    have sameFirst : first = memory := by
      rw [expand_zero memory destination aligned bounded] at destinationGrowth
      exact (Except.ok.inj destinationGrowth).symm
    subst first
    have sameGrown : grown = memory := by
      rw [expand_zero memory source aligned bounded] at sourceGrowth
      exact (Except.ok.inj sourceGrowth).symm
    subst grown
    simp [Memory.copy, Execution.writeBytes, Execution.readBytes]
  · have sourceWithin : source + count ≤ limit := by omega
    have active : destination + (Execution.readBytes grown source count).length ≤ grown.length := by
      have := expanded_active memory first destination count (by omega) destinationGrowth
      have := expanded_monotone first grown source count sourceGrowth
      simp only [Execution.readBytes, List.length_map, List.length_range]
      omega
    have firstAligned := expanded_aligned memory first destination count aligned destinationGrowth
    have firstBound := expanded_bound memory first destination count aligned destinationGrowth
    have snapshot : Execution.readBytes grown source count = Memory.read (project limit memory) source count := by
      rw [read_expanded first grown source count source count sourceGrowth,
        read_expanded memory first destination count source count destinationGrowth,
        read_project limit memory source count (Or.inr sourceWithin)]
    rw [write_bytes_project limit grown _ destination active, snapshot,
      expand_project limit first grown source count firstAligned firstBound sourceGrowth,
      expand_project limit memory first destination count aligned bounded destinationGrowth]
    simp only [Memory.copy, Memory.expand, zero, ite_false]
    rw [copy_bytes]
    congr 1
    exact grow_comm _ _ _

private theorem memory_cost_mono {first second : Nat} (ordered : first ≤ second) :
    EVM.State.memoryCost first ≤ EVM.State.memoryCost second := by
  unfold EVM.State.memoryCost
  exact Nat.add_le_add (Nat.mul_le_mul_left _ ordered)
    (Nat.div_le_div_right (Nat.mul_le_mul ordered ordered))

/-- Additional expansion needed for the source after accounting for the destination. -/
def copySourceCharge (memory : List Execution.Byte) (destination source count : Nat) : Nat :=
  EVM.State.memoryCost (max (Execution.requiredWords memory source count)
    (Execution.requiredWords memory destination count)) -
      EVM.State.memoryCost (Execution.requiredWords memory destination count)

theorem copy_source_charge (memory first : List Execution.Byte) (destination source count : Nat)
    (aligned : memory.length % 32 = 0)
    (expanded : Execution.expand memory destination count = .ok first) :
    copySourceCharge memory destination source count = EVM.State.memoryCharge first source count := by
  have words := words_expanded memory first destination count aligned expanded
  unfold copySourceCharge EVM.State.memoryCharge
  rw [words]
  congr 2
  unfold Execution.requiredWords
  by_cases zero : count = 0
  · simp [zero, words, Execution.requiredWords]
  · simp only [zero, ite_false, words, Execution.requiredWords]
    omega

theorem copy_charge (memory first grown : List Execution.Byte)
    (destination source count : Word) (stack : EVM.Stack) (fork : Fork)
    (aligned : memory.length % 32 = 0) (bounded : memory.length ≤ 65536)
    (destinationGrowth : Execution.expand memory destination.toNat count.toNat = .ok first)
    (sourceGrowth : Execution.expand first source.toNat count.toNat = .ok grown) :
    EVM.Gas.cost .copy (destination :: source :: count :: stack) (project limit memory) fork =
      3 + 3 * ((count.toNat + 31) / 32) + copySourceCharge memory destination.toNat source.toNat count.toNat +
        EVM.State.memoryCharge memory destination.toNat count.toNat := by
  have firstAligned := expanded_aligned memory first destination.toNat count.toNat aligned destinationGrowth
  have firstBound := expanded_bound memory first destination.toNat count.toNat aligned destinationGrowth
  have destinationProject := expand_project limit memory first destination.toNat count.toNat aligned bounded destinationGrowth
  have sourceProject := expand_project limit first grown source.toNat count.toNat firstAligned firstBound sourceGrowth
  have price1 := expansion_charge limit memory first destination.toNat count.toNat aligned bounded destinationGrowth
  have price2 := expansion_charge limit first grown source.toNat count.toNat firstAligned firstBound sourceGrowth
  rw [← destinationProject] at price1
  rw [← sourceProject] at price2
  have oldFits : Execution.memoryWords memory < 2^251 := by unfold Execution.memoryWords; omega
  have firstFits : Execution.memoryWords first < 2^251 := by unfold Execution.memoryWords; omega
  have grownFits : Execution.memoryWords grown < 2^251 := by
    have := expanded_bound first grown source.toNat count.toNat firstAligned sourceGrowth
    unfold Execution.memoryWords
    omega
  have le1 : EVM.State.memoryCost (Execution.memoryWords memory) ≤ EVM.State.memoryCost (Execution.memoryWords first) :=
    memory_cost_mono (Nat.div_le_div_right (expanded_monotone _ _ _ _ destinationGrowth))
  have le2 : EVM.State.memoryCost (Execution.memoryWords first) ≤ EVM.State.memoryCost (Execution.memoryWords grown) :=
    memory_cost_mono (Nat.div_le_div_right (expanded_monotone _ _ _ _ sourceGrowth))
  simp only [EVM.Gas.cost, EVM.Gas.baseCost, EVM.Gas.touched]
  have same : Memory.expand (Memory.expand (project limit memory) source.toNat count.toNat)
      destination.toNat count.toNat = project limit grown := by
    by_cases zero : count.toNat = 0
    · simp [Memory.expand, zero] at destinationProject sourceProject ⊢
      rw [sourceProject, destinationProject]
    · rw [sourceProject, destinationProject]
      simp only [Memory.expand, zero, ite_false]
      rw [grow_comm]
  rw [same, copy_source_charge memory first _ _ _ aligned destinationGrowth, ← price1, ← price2]
  simp only [EVM.Gas.expansionCost, EVM.Gas.memoryCost, EVM.State.memoryCost, project,
    BitVec.toNat_ofNat, Nat.mod_eq_of_lt oldFits, Nat.mod_eq_of_lt firstFits,
    Nat.mod_eq_of_lt grownFits] at le1 le2 ⊢
  omega

theorem copy_instruction (config : EVM.State.Config) (world : Execution.Journal)
    (frame : EVM.State.Frame) (destination source count : Word) (stack : EVM.Stack)
    (base dynamic : Nat) (opcode : EVM.State.available config 0x5e = ⟨true,base,dynamic,3,0⟩)
    (fetched : frame.code[frame.pc]? = some (0x5e : Execution.Byte))
    (words : frame.stack = destination :: source :: count :: stack)
    (fits : (destination :: source :: count :: stack).length ≤ 1024) :
    EVM.State.instruction config world frame =
      EVM.State.withMemory world frame (base + dynamic * ((count.toNat + 31) / 32) +
        copySourceCharge frame.memory destination.toNat source.toNat count.toNat)
        destination.toNat count.toNat (fun first => do
          let grown ← Execution.expand first.memory source.toNat count.toNat
          EVM.State.next world {first with
            memory := Execution.writeBytes grown destination.toNat
              (Execution.readBytes grown source.toNat count.toNat)} stack) := by
  have present : ¬frame.pc ≥ frame.code.length := by
    have := (List.getElem?_eq_some_iff.mp fetched).1
    omega
  have enough : ¬stack.length + 1 + 1 + 1 < 3 := by omega
  have height : ¬stack.length > 1024 := by
    simp only [List.length_cons] at fits
    omega
  simp [EVM.State.instruction, EVM.State.instructionWithImage, fetched, opcode, present,
    words, enough, height, EVM.State.pureOp, copySourceCharge]

theorem copy_frame (config : EVM.State.Config) (world : Execution.Journal)
    (frame : EVM.State.Frame) (first grown : List Execution.Byte)
    (destination source count : Word) (stack : EVM.Stack)
    (base dynamic : Nat) (opcode : EVM.State.available config 0x5e = ⟨true,base,dynamic,3,0⟩)
    (fetched : frame.code[frame.pc]? = some (0x5e : Execution.Byte))
    (words : frame.stack = destination :: source :: count :: stack)
    (fits : (destination :: source :: count :: stack).length ≤ 1024)
    (aligned : frame.memory.length % 32 = 0) (bounded : frame.memory.length ≤ 65536)
    (within : count.toNat = 0 ∨ max destination.toNat source.toNat + count.toNat ≤ limit)
    (destinationGrowth : Execution.expand frame.memory destination.toNat count.toNat = .ok first)
    (sourceGrowth : Execution.expand first source.toNat count.toNat = .ok grown) :
    let charge := base + dynamic * ((count.toNat + 31) / 32) +
      copySourceCharge frame.memory destination.toNat source.toNat count.toNat +
      EVM.State.memoryCharge frame.memory destination.toNat count.toNat
    let memory := Execution.writeBytes grown destination.toNat (Execution.readBytes grown source.toNat count.toNat)
    let after := {frame with
      pc := frame.pc + 1
      stack := stack
      memory := memory
      gas := frame.gas - charge}
    EVM.State.instruction config world frame =
      (if frame.gas < charge then EVM.State.stepExhausted world else .ok (.next world after)) ∧
    EVM.Memory.step (.ofEnvironment frame.environment frame.returnData)
      .copy frame.stack (project limit frame.memory) = .ok (after.stack, project limit after.memory) ∧
    ∀ start size, limit ≤ start → Execution.readBytes after.memory start size =
      Execution.readBytes frame.memory start size := by
  dsimp only
  constructor
  · rw [copy_instruction config world frame destination source count stack base dynamic opcode fetched words fits]
    simp [EVM.State.withMemory, destinationGrowth, sourceGrowth, EVM.State.next,
      Bind.bind, Except.bind]
  · constructor
    · have height : ¬stack.length + 1 + 1 + 1 > 1024 := by
        simp only [List.length_cons] at fits
        omega
      have sourceWithin : count.toNat = 0 ∨ source.toNat + count.toNat ≤ limit := by omega
      simp [EVM.Memory.step, words, height, copy_project limit frame.memory first grown _ _ _
        aligned bounded sourceWithin destinationGrowth sourceGrowth]
    · intro start size outside
      by_cases zero : count.toNat = 0
      · have empty : Execution.readBytes grown source.toNat count.toNat = [] := by simp [zero]
        rw [empty, show Execution.writeBytes grown destination.toNat [] = grown by simp [Execution.writeBytes]]
        rw [read_expanded first grown _ _ start size sourceGrowth,
          read_expanded frame.memory first _ _ start size destinationGrowth]
      · have disjoint : destination.toNat + (Execution.readBytes grown source.toNat count.toNat).length ≤ start := by
          simp only [Execution.readBytes, List.length_map, List.length_range]
          omega
        rw [Execution.readBytes_writeBytes_disjoint _ _ _ _ _ (Or.inl disjoint),
          read_expanded first grown _ _ start size sourceGrowth,
          read_expanded frame.memory first _ _ start size destinationGrowth]

end SolarTV.Memory.Projection
