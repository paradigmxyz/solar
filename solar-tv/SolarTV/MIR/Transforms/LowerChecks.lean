import SolarTV.MIR.Transforms.LowerAbiEncode.ZeroCopy
import SolarTV.MIR.State.Ops

/-! Window-level payload equivalence for the selector and the evaluated words.
The encoder writes the selector before its words at the free-memory pointer.
Lowering stages the words first and reverts from byte 28 in reserved scratch. -/
namespace SolarTV.MIR.LowerChecks
open EvmRules SolarTV.Execution
open MIR.LowerAbiEncode.ZeroCopy
set_option maxRecDepth 8192

private theorem stageWord_view (memory : List Byte) (address : Nat) (word : Word)
    (bounded : memory.length ≤ 65536) (fits : address + 32 ≤ 65536) :
    ∃ next, MIR.State.stageWord memory address word = .ok next ∧ next.length ≤ 65536 ∧
      view next = write (view memory) address (wordBytes word) := by
  simpa [MIR.State.stageWord, writeMemory, wordBytes_length] using
    writeMemory_view memory (wordBytes word) address bounded (by simpa using fits)

private theorem write_append (memory : Memory) (address : Nat) (left right : List Byte) :
    write (write memory address left) (address + left.length) right =
      write memory address (left ++ right) := by
  funext i
  by_cases before : i < address
  · rw [write_before _ _ _ i (by omega), write_before _ _ _ i before,
      write_before _ _ _ i before]
  · by_cases middle : i < address + left.length
    · rw [write_before _ _ _ i middle, write_inside _ _ _ i ⟨by omega, middle⟩,
        write_inside _ _ _ i ⟨by omega, by simp only [List.length_append]; omega⟩]
      rw [List.getElem_append_left (by omega)]
    · by_cases after : address + left.length + right.length ≤ i
      · rw [write_after _ _ _ i after, write_after _ _ _ i (by omega),
          write_after _ _ _ i (by simp only [List.length_append]; omega)]
      · rw [write_inside _ _ _ i ⟨by omega, by omega⟩,
          write_inside _ _ _ i ⟨by omega, by simp only [List.length_append]; omega⟩]
        rw [List.getElem_append_right (by omega)]
        congr 1
        omega

private theorem stageWords_view (memory : List Byte) (address : Nat) (words : List Word)
    (bounded : memory.length ≤ 65536) (fits : address + 32 * words.length ≤ 65536) :
    ∃ next, MIR.State.stageWords memory address words = .ok next ∧ next.length ≤ 65536 ∧
      view next = write (view memory) address (words.flatMap wordBytes) := by
  induction words generalizing memory address with
  | nil =>
    refine ⟨memory, rfl, bounded, ?_⟩
    funext i
    by_cases before : i < address
    · exact (write_before (view memory) [] address i before).symm
    · exact (write_after (view memory) [] address i (by simp; omega)).symm
  | cons word tail ih =>
    have fitsWord : address + (wordBytes word).length ≤ 65536 := by
      simp only [wordBytes_length, List.length_cons, Nat.mul_succ] at fits ⊢
      omega
    obtain ⟨written, equation, within, contents⟩ :=
      stageWord_view memory address word bounded (by simpa using fitsWord)
    have fitsTail : address + 32 + 32 * tail.length ≤ 65536 := by
      simp only [List.length_cons, Nat.mul_succ] at fits
      omega
    obtain ⟨next, step, nextWithin, nextContents⟩ := ih written (address + 32) within fitsTail
    refine ⟨next, by simpa only [MIR.State.stageWords, Bind.bind, Except.bind, equation] using step,
      nextWithin, ?_⟩
    rw [nextContents, contents]
    simpa only [List.flatMap_cons, wordBytes_length] using
      write_append (view memory) address (wordBytes word) (tail.flatMap wordBytes)

theorem scratch_view (memory : Memory) (selector : Word) (data : List Byte) :
    read (write (write memory 32 data) 0 (wordBytes selector)) 28 (4 + data.length) =
      (wordBytes selector).drop 28 ++ data := by
  have len : ((wordBytes selector).drop 28).length = 4 := by simp
  apply List.ext_getElem
  · simp [len]
  · intro i hi hj
    have inRange : i < 4 + data.length := by simpa using hi
    rw [read_at _ _ _ _ inRange]
    by_cases head : i < 4
    · rw [write_inside _ _ _ _ ⟨by omega, by simp only [wordBytes_length]; omega⟩,
        List.getElem_append_left (by simpa [len] using head), List.getElem_drop]
      congr 1
    · rw [write_after _ _ _ _ (by simp only [wordBytes_length]; omega),
        write_inside _ _ _ _ ⟨by omega, by omega⟩,
        List.getElem_append_right (by simpa [len] using Nat.le_of_not_gt head)]
      congr 1
      omega

theorem packed_view (memory : Memory) (base : Nat) (selector : Word) (data : List Byte) :
    read (write (write memory base (wordBytes selector)) (base + 4) data)
      base (4 + data.length) = (wordBytes selector).take 4 ++ data := by
  have len : ((wordBytes selector).take 4).length = 4 := by simp
  apply List.ext_getElem
  · simp [len]
  · intro i hi hj
    have inRange : i < 4 + data.length := by simpa using hi
    rw [read_at _ _ _ _ inRange]
    by_cases head : i < 4
    · rw [write_before _ _ _ _ (by omega),
        write_inside _ _ _ _ ⟨by omega, by simp only [wordBytes_length]; omega⟩,
        List.getElem_append_left (by simpa [len] using head), List.getElem_take]
      congr 1
      omega
    · rw [write_inside _ _ _ _ ⟨by omega, by omega⟩,
        List.getElem_append_right (by simpa [len] using Nat.le_of_not_gt head)]
      congr 1
      omega

private theorem payloadLength (words : List Word) :
    (words.flatMap wordBytes).length = 32 * words.length := by
  induction words with
  | nil => rfl
  | cons word tail ih =>
    simp only [List.flatMap_cons, List.length_append, wordBytes_length, ih,
      List.length_cons, Nat.mul_succ]
    omega

def observePayload (memory : List Byte) (address length : Nat) : Except String (List Byte) := do
  let active ← expand memory address length
  return readBytes active address length

private theorem observe_eq (memory : List Byte) (address length : Nat)
    (bounded : memory.length ≤ 65536) (fits : address + length ≤ 65536) :
    observePayload memory address length = .ok (read (view memory) address length) := by
  obtain ⟨active, expanded, _, _⟩ := expand_fits memory address length bounded fits
  have contents := expand_view memory active address length expanded
  simp only [observePayload, Bind.bind, Except.bind, expanded, Pure.pure, Except.pure]
  rw [← read_view active, contents]

def scratchPayload (memory : List Byte) (selector : Word) (words : List Word) :
    Except String (List Byte) := do
  let memory ← MIR.State.stageWords memory 32 words
  let memory ← MIR.State.stageWord memory 0 selector
  observePayload memory 28 (4 + 32 * words.length)

theorem scratch_payload (memory : List Byte) (selector : Word) (words : List Word)
    (bounded : memory.length ≤ 65536) (fits : 32 + 32 * words.length ≤ 65536) :
    scratchPayload memory selector words =
      .ok ((wordBytes selector).drop 28 ++ words.flatMap wordBytes) := by
  obtain ⟨staged, equation, within, contents⟩ := stageWords_view memory 32 words bounded fits
  obtain ⟨next, done, nextWithin, nextContents⟩ :=
    stageWord_view staged 0 selector within (by decide)
  simp only [scratchPayload, Bind.bind, Except.bind, equation, done]
  rw [observe_eq next 28 (4 + 32 * words.length) nextWithin (by omega),
    nextContents, contents, ← payloadLength words, scratch_view]

theorem selector_window (selector : BitVec 32) :
    (wordBytes ((selector.setWidth 256) <<< 224)).take 4 =
      (wordBytes (selector.setWidth 256)).drop 28 := by
  apply List.ext_getElem
  · simp
  · intro i head tail
    have less : i < 4 := by simpa using head
    rw [List.getElem_take, List.getElem_drop]
    simp only [wordBytes, List.getElem_map, List.getElem_range]
    let k := 8 * (3 - i)
    have high : 8 * (31 - i) = 224 + k := by dsimp only [k]; omega
    have low : 8 * (31 - (28 + i)) = k := by dsimp only [k]; omega
    rw [high, low]
    apply BitVec.eq_of_getLsbD_eq
    intro j bound
    have inWord : 224 + k + j < 256 := by dsimp only [k]; omega
    have after : ¬ 224 + k + j < 224 := by omega
    have rightIndex : 224 + k + j - 224 = k + j := by omega
    have small : k + j < 256 := by omega
    simp only [BitVec.getLsbD_setWidth, BitVec.getLsbD_ushiftRight,
      BitVec.getLsbD_shiftLeft, bound, inWord, after, rightIndex, small, decide_true,
      decide_false, Bool.not_false, Bool.true_and]

private theorem short_failure (cfg : MIR.State.Config) (frame : MIR.State.Frame) (length data : Word)
    (bounded : frame.memory.length ≤ 65536) :
    (do
      let payload ← scratchPayload frame.memory 0x08c379a0 [32, length, data]
      return Outcome.reverted (RevertData.fromBytes payload)) =
      MIR.State.abiRequirePayload cfg frame .short
        [MIR.State.abiScalar 0, MIR.State.abiScalar length, MIR.State.abiScalar data] := by
  have fits : 32 + 32 * ([32, length, data] : List Word).length ≤ 65536 := by
    change 128 ≤ 65536
    decide
  rw [scratch_payload _ _ _ bounded fits]
  have selector : (wordBytes (0x08c379a0 : Word)).drop 28 = [0x08,0xc3,0x79,0xa0] := by
    decide
  rw [selector]
  simp only [List.flatMap_cons, List.flatMap_nil, List.append_nil]
  rfl

private theorem flag_widen_zero (flag : BitVec 1) :
    (flag.setWidth 256 == 0) = (flag == 0) := by
  have bound := flag.isLt
  have casesNat : flag.toNat = 0 ∨ flag.toNat = 1 := by omega
  rcases casesNat with zero | one
  · have h : flag = 0 := BitVec.eq_of_toNat_eq zero
    rw [h]
    rfl
  · have h : flag = 1 := BitVec.eq_of_toNat_eq one
    rw [h]
    rfl

def branchCheck (frame : MIR.State.Frame) (flag : BitVec 1) (zero : Bool)
    (failure : MIR.State.Step) : MIR.State.Step :=
  if (flag == 0) == zero then failure else MIR.State.effect frame

theorem empty_check (cfg : MIR.State.Config) (frame : MIR.State.Frame) (flag : BitVec 1)
    (zero : Bool) (message : Option (List Byte)) (defaultMode : cfg.debugReverts = false)
    (bounded : frame.memory.length ≤ 65536) :
    branchCheck frame flag zero
      (do
        let bytes ← observePayload frame.memory 0 0
        return .reverted (RevertData.fromBytes bytes)) =
      MIR.State.operation cfg frame (.check zero none message) [flag.setWidth 256] := by
  have eq := flag_widen_zero flag
  rw [observe_eq frame.memory 0 0 bounded (by decide)]
  have empty : LowerAbiEncode.ZeroCopy.read (view frame.memory) 0 0 = [] := rfl
  rw [empty]
  unfold branchCheck
  simp only [MIR.State.operation, eq, defaultMode]
  rfl

theorem panic65_check (cfg : MIR.State.Config) (frame : MIR.State.Frame) (flag : BitVec 1)
    (zero : Bool) (bounded : frame.memory.length ≤ 65536) :
    branchCheck frame flag zero
      (do
        let bytes ← scratchPayload frame.memory 0x4e487b71 [65]
        return .reverted (RevertData.fromBytes bytes)) =
      MIR.State.operation cfg frame (.check zero (some 65) none) [flag.setWidth 256] := by
  have eq := flag_widen_zero flag
  rw [scratch_payload _ _ _ bounded (by decide)]
  unfold branchCheck
  simp only [MIR.State.operation, eq]
  rfl


private theorem require_pass (cfg : MIR.State.Config) (frame : MIR.State.Frame)
    (op : MIR.State.RequireOp) (flag : BitVec 1) (args : List MIR.State.Value) (pass : flag ≠ 0#1) :
    MIR.State.abiRequirePayload cfg frame op (.scalar ⟨1, flag⟩ :: args) = MIR.State.effect frame := by
  have nonzero : flag.setWidth 256 ≠ 0#256 := by
    intro zero
    have eq := flag_widen_zero flag
    change (flag.setWidth 256 == 0#256) = (flag == 0#1) at eq
    exact pass (beq_iff_eq.mp (by rw [← eq, zero]; rfl))
  change (if flag.setWidth 256 != 0#256 then _ else _) = _
  simp [nonzero, MIR.State.effect]
  rfl

theorem short_check (cfg : MIR.State.Config) (frame : MIR.State.Frame)
    (flag : BitVec 1) (length data : Word) (bounded : frame.memory.length ≤ 65536) :
    branchCheck frame flag true
      (do
        let bytes ← scratchPayload frame.memory 0x08c379a0 [32,length,data]
        return .reverted (RevertData.fromBytes bytes)) =
      MIR.State.abiRequirePayload cfg frame .short
        [.scalar ⟨1,flag⟩, MIR.State.abiScalar length, MIR.State.abiScalar data] := by
  by_cases h : flag = 0#1
  · subst flag
    exact short_failure cfg frame length data bounded
  · rw [require_pass _ _ _ _ _ h]
    simp [branchCheck, h]

private theorem custom_check (cfg : MIR.State.Config) (frame : MIR.State.Frame)
    (flag : BitVec 1) (selector : BitVec 32) (words : List Word)
    (shapes : List MIR.State.AbiEncodeShape) (bounded : frame.memory.length ≤ 65536)
    (fits : (readWord frame.memory 64).toNat + 4 + 32 * words.length ≤ 65536)
    (nonempty : 0 < words.length) (scratchArity : words.length ≤ 3)
    (encode : ∀ active staged final,
      expand frame.memory 64 32 = .ok active →
      MIR.State.stageWord active (readWord active 64).toNat (selector.setWidth 256 <<< 224) = .ok staged →
      MIR.State.stageWords staged ((readWord active 64).toNat + 4) words = .ok final →
      (readWord active 64).toNat + (4 + 32 * words.length) ≤ 65536 →
      MIR.State.abiEncodeTree cfg frame 2 false true shapes (List.replicate words.length false)
          (MIR.State.abiScalar (selector.setWidth 256 <<< 224) :: words.map MIR.State.abiScalar) =
        .ok (.returned (.aggregate [MIR.State.abiScalar (readWord active 64),
          MIR.State.abiScalar (BitVec.ofNat 256 (4 + 32 * words.length))], {frame with memory := final}))) :
    branchCheck frame flag true
      (do
        let bytes ← scratchPayload frame.memory (selector.setWidth 256) words
        return .reverted (RevertData.fromBytes bytes)) =
      MIR.State.abiRequirePayload cfg frame (.encode true shapes (List.replicate words.length false))
        (.scalar ⟨1,flag⟩ :: MIR.State.abiScalar (selector.setWidth 256 <<< 224) :: words.map MIR.State.abiScalar) := by
  by_cases h : flag = 0#1
  · subst flag
    obtain ⟨active, expanded, activeFits, _⟩ := expand_fits frame.memory 64 32 bounded (by decide)
    have cursor : readWord active 64 = readWord frame.memory 64 := by
      unfold readWord
      rw [← read_view active, expand_view _ _ _ _ expanded, read_view]
    let base := (readWord active 64).toNat
    let length := 4 + 32 * words.length
    have range : base + length ≤ 65536 := by simp only [base, length, cursor]; omega
    obtain ⟨staged, selectorWritten, stagedFits, selectorBytes⟩ :=
      stageWord_view active base (selector.setWidth 256 <<< 224) activeFits (by
        change base + 32 ≤ 65536
        dsimp only [length] at range
        omega)
    obtain ⟨final, wordsWritten, finalFits, wordBytesWritten⟩ :=
      stageWords_view staged (base + 4) words stagedFits (by simpa only [length, Nat.add_assoc] using range)
    have encoded := encode active staged final expanded selectorWritten wordsWritten range
    obtain ⟨observed, expandedFinal, _, _⟩ := expand_fits final base length finalFits range
    have bytes : readBytes observed base length =
        (wordBytes (selector.setWidth 256)).drop 28 ++ words.flatMap wordBytes := by
      rw [← read_view observed, expand_view _ _ _ _ expandedFinal, wordBytesWritten, selectorBytes]
      dsimp only [length]
      rw [← payloadLength words, packed_view, selector_window]
    have scratchFits : 32 + 32 * words.length ≤ 65536 := by omega
    rw [scratch_payload _ _ _ bounded scratchFits]
    simp only [MIR.State.abiRequirePayload, MIR.State.Value.word, MIR.State.Scalar.word,
      MIR.State.widenBits, Bind.bind, Except.bind, Pure.pure, Except.pure]
    rw [encoded]
    have length_word : (BitVec.ofNat 256 length).toNat = length := by
      rw [BitVec.toNat_ofNat, Nat.mod_eq_of_lt (by omega)]
    change _ = (do
      let memory ← expand final base (BitVec.ofNat 256 length).toNat
      return Outcome.reverted (RevertData.fromBytes (readBytes memory base (BitVec.ofNat 256 length).toNat)))
    rw [length_word, expandedFinal]
    exact congrArg (fun data => Except.ok (Outcome.reverted (RevertData.fromBytes data))) bytes.symm
  · rw [require_pass _ _ _ _ _ h]
    simp [branchCheck, h]

theorem custom_one (cfg : MIR.State.Config) (frame : MIR.State.Frame)
    (flag : BitVec 1) (selector : BitVec 32) (rule : MIR.State.AbiRule) (a : Word)
    (bounded : frame.memory.length ≤ 65536)
    (fits : (readWord frame.memory 64).toNat + 36 ≤ 65536) :
    branchCheck frame flag true
      (do
        let bytes ← scratchPayload frame.memory (selector.setWidth 256) [a]
        return .reverted (RevertData.fromBytes bytes)) =
      MIR.State.abiRequirePayload cfg frame (.encode true [.word rule] [false])
        [.scalar ⟨1,flag⟩, MIR.State.abiScalar (selector.setWidth 256 <<< 224),
          MIR.State.abiScalar a] :=
  custom_check cfg frame flag selector [a] [.word rule] bounded fits
    (by change 0 < 1; decide) (by change 1 ≤ 3; decide)
    (MIR.State.abiEncodeTree_static_one cfg frame rule _ a)

theorem custom_three (cfg : MIR.State.Config) (frame : MIR.State.Frame)
    (flag : BitVec 1) (selector : BitVec 32) (ra rb rc : MIR.State.AbiRule) (a b c : Word)
    (bounded : frame.memory.length ≤ 65536)
    (fits : (readWord frame.memory 64).toNat + 100 ≤ 65536) :
    branchCheck frame flag true
      (do
        let bytes ← scratchPayload frame.memory (selector.setWidth 256) [a,b,c]
        return .reverted (RevertData.fromBytes bytes)) =
      MIR.State.abiRequirePayload cfg frame (.encode true [.word ra,.word rb,.word rc] [false,false,false])
        [.scalar ⟨1,flag⟩, MIR.State.abiScalar (selector.setWidth 256 <<< 224),
          MIR.State.abiScalar a, MIR.State.abiScalar b, MIR.State.abiScalar c] :=
  custom_check cfg frame flag selector [a,b,c] [.word ra,.word rb,.word rc] bounded fits
    (by change 0 < 3; decide) (by change 3 ≤ 3; decide)
    (MIR.State.abiEncodeTree_static_three cfg frame ra rb rc _ a b c)

end SolarTV.MIR.LowerChecks
