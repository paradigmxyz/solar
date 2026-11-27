import SolarTV.MIR.Transforms.LowerAbiEncode
import SolarTV.MIR.Transforms.LowerReturndata
import SolarTV.State.MemoryLaws

/-! Byte observations at a terminal ABI encoder. The output can move from
scratch memory to the original object because only the returned bytes survive
the frame. Model private memory as its zero-extended byte view. -/
namespace SolarTV.MIR.LowerAbiEncode.ZeroCopy
open EvmRules SolarTV.Execution Effects

abbrev Memory := Nat → Byte

def read (memory : Memory) (address count : Nat) : List Byte :=
  (List.range count).map fun i => memory (address + i)

def write (memory : Memory) (address : Nat) (bytes : List Byte) : Memory :=
  fun i => if h : address ≤ i ∧ i < address + bytes.length
    then bytes[i - address]'(by omega)
    else memory i

theorem write_before (memory : Memory) (bytes : List Byte) (address i : Nat) (h : i < address) :
    write memory address bytes i = memory i := by
  simp only [write]
  split
  · omega
  · rfl

theorem write_after (memory : Memory) (bytes : List Byte) (address i : Nat)
    (h : address + bytes.length ≤ i) :
    write memory address bytes i = memory i := by
  simp only [write]
  split
  · omega
  · rfl

theorem write_inside (memory : Memory) (bytes : List Byte) (address i : Nat)
    (h : address ≤ i ∧ i < address + bytes.length) :
    write memory address bytes i = bytes[i - address]'(by omega) := by
  simp [write,h]

@[simp] theorem read_length (memory : Memory) (address count : Nat) : (read memory address count).length = count := by
  simp [read]

@[simp] theorem read_at (memory : Memory) (address count i : Nat) (h : i < count) :
    (read memory address count)[i]'(by simpa) = memory (address + i) := by
  simp [read]

def round (length : Nat) : Nat := ((length + 31) / 32) * 32

theorem round_bound (length : Nat) : length ≤ round length ∧ round length < length + 32 := by
  unfold round
  omega

def copying (memory : Memory) (scratch object size extent : Nat) (length : Word) (bytes : Bool) : Memory :=
  let offset := write memory scratch (wordBytes 32)
  let clear := if bytes then write offset (scratch + 32 + extent) (wordBytes 0) else offset
  let header := write clear (scratch + 32) (wordBytes length)
  write header (scratch + 64) (read header (object + 32) size)

def reuse (memory : Memory) (object size : Nat) (bytes : Bool) : Memory :=
  let clear := if bytes then write memory (object + 32 + size) (wordBytes 0) else memory
  write clear (object - 32) (wordBytes 32)

private theorem zero_word (index : Nat) (h : index < (wordBytes 0).length) :
    (wordBytes 0)[index]'h = 0 := by
  simp [wordBytes] <;> rfl

/-- Every byte observed by RETURNDATA is identical. The original object's
length header is live and its payload lies below the scratch allocation.
The conditions are about memory layout and the observable extent. -/
theorem same (memory : Memory) (scratch object size : Nat) (length : Word) (bytes : Bool)
    (placed : 32 ≤ object) (privateObject : object + 32 + size ≤ scratch)
    (header : ∀ i, ∀ h : i < 32, memory (object + i) =
      (wordBytes length)[i]'(by simpa [wordBytes_length] using h)) :
    let extent := if bytes then round size else size
    read (copying memory scratch object size extent length bytes) scratch (64 + extent) =
      read (reuse memory object size bytes) (object - 32) (64 + extent) := by
  let extent := if bytes then round size else size
  have lower : size ≤ extent := by by_cases h : bytes <;> simp [extent,h,round_bound]
  have upper : extent < size + 32 := by by_cases h : bytes <;> simp [extent,h,round_bound]
  have unchanged : ¬ bytes → extent = size := by
    intro h
    simp [extent,h]
  change read (copying memory scratch object size extent length bytes) scratch (64 + extent) =
    read (reuse memory object size bytes) (object - 32) (64 + extent)
  unfold read
  apply List.map_congr_left
  intro i hi
  have within : i < 64 + extent := List.mem_range.mp hi
  have wordLength (w : Word) : (wordBytes w).length = 32 := wordBytes_length w
  by_cases off : i < 32
  · unfold copying reuse
    rw [write_before _ _ _ _ (by omega)]
    rw [write_before _ _ _ _ (by omega)]
    cases bytes <;> simp only [Bool.false_eq_true,ite_false,ite_true]
    · rw [write_inside _ _ _ _ (by simp only [wordLength]; omega),
          write_inside _ _ _ _ (by simp only [wordLength]; omega)]
      congr 1 <;> omega
    · rw [write_before _ _ _ _ (by omega)]
      rw [write_inside _ _ _ _ (by simp only [wordLength]; omega),
          write_inside _ _ _ _ (by simp only [wordLength]; omega)]
      congr 1 <;> omega
  by_cases len : i < 64
  · unfold copying reuse
    rw [write_before _ _ _ _ (by omega)]
    rw [write_inside _ _ _ _ (by simp only [wordLength]; omega)]
    rw [write_after _ _ _ _ (by simp only [wordLength]; omega)]
    cases bytes <;> simp only [Bool.false_eq_true,ite_false,ite_true]
    · rw [show object - 32 + i = object + (i - 32) by omega, header _ (by omega)]
      congr 1 <;> omega
    · rw [write_before _ _ _ _ (by omega)]
      rw [show object - 32 + i = object + (i - 32) by omega, header _ (by omega)]
      congr 1 <;> omega
  by_cases payload : i < 64 + size
  · unfold copying reuse
    rw [write_inside _ _ _ _ (by simp only [read_length]; omega), read_at _ _ _ _ (by omega)]
    rw [write_before _ _ _ _ (by omega)]
    rw [write_after _ _ _ _ (by simp only [wordLength]; omega)]
    cases bytes <;> simp only [Bool.false_eq_true,ite_false,ite_true]
    · rw [write_before _ _ _ _ (by omega)]
      congr 1 <;> omega
    · rw [write_before _ _ _ _ (by omega)]
      rw [write_before _ _ _ _ (by omega),write_before _ _ _ _ (by omega)]
      congr 1 <;> omega
  have yes : bytes = true := by
    cases h : bytes
    · have := unchanged (by simp [h])
      omega
    · rfl
  unfold copying reuse
  simp only [yes, ite_true]
  rw [write_after _ _ _ _ (by simp only [read_length]; omega)]
  rw [write_after _ _ (scratch+32) _ (by simp only [wordLength]; omega)]
  rw [write_after _ _ (object-32) _ (by simp only [wordLength]; omega)]
  rw [write_inside _ _ (scratch+32+extent) _ (by simp only [wordLength]; omega), zero_word,
      write_inside _ _ (object+32+size) _ (by simp only [wordLength]; omega), zero_word]


def view (memory : List Byte) : Memory := fun i => memory[i]?.getD 0

theorem read_view (memory : List Byte) (address count : Nat) :
    read (view memory) address count = readBytes memory address count := rfl

theorem expand_view (memory expanded : List Byte) (address count : Nat)
    (h : expand memory address count = .ok expanded) :
    view expanded = view memory := by
  unfold expand at h
  dsimp only at h
  split at h
  · contradiction
  · simp only [Except.ok.injEq] at h
    subst expanded
    funext i
    by_cases before : i < memory.length
    · simp [view, List.getElem?_append_left before]
    · simp [view, List.getElem?_append_right (Nat.le_of_not_lt before),
        List.getElem?_eq_none (Nat.le_of_not_lt before), List.getElem?_replicate] <;> split <;> rfl

theorem write_view (memory bytes : List Byte) (address : Nat) (fits : address + bytes.length ≤ memory.length) :
    view (writeBytes memory address bytes) = write (view memory) address bytes := by
  funext i
  rw [writeBytes_eq_spec]
  unfold view write writeBytesSpec
  by_cases here : address ≤ i ∧ i < address + bytes.length
  · have active : i < memory.length := by omega
    have inBytes : i - address < bytes.length := by omega
    simp [here, List.getElem?_mapIdx, List.getElem?_eq_getElem active,
      List.getElem?_eq_getElem inBytes]
  · simp [here, List.getElem?_mapIdx]

theorem expand_fits (memory : List Byte) (address size : Nat)
    (memoryFits : memory.length ≤ 65536) (rangeFits : address + size ≤ 65536) :
    ∃ expanded, expand memory address size = .ok expanded ∧
      expanded.length ≤ 65536 ∧ (size = 0 ∨ address + size ≤ expanded.length) := by
  unfold expand requiredWords memoryWords
  by_cases empty : size = 0
  · simp only [empty, ite_true]
    have bounded : 32 * (memory.length / 32) ≤ 65536 := by omega
    simp [Nat.not_lt.mpr bounded] <;> omega
  · simp only [empty, ite_false]
    have bounded : 32 * max (memory.length / 32) ((address + size + 31) / 32) ≤ 65536 := by
      omega
    have covers : address + size ≤ 32 * max (memory.length / 32) ((address + size + 31) / 32) := by
      omega
    simp [Nat.not_lt.mpr bounded] <;> omega


def writeMemory (memory : List Byte) (address : Nat) (bytes : List Byte) : Except String (List Byte) := do
  let active ← expand memory address bytes.length
  return writeBytes active address bytes

theorem writeMemory_view (memory bytes : List Byte) (address : Nat)
    (bound : memory.length ≤ 65536) (fits : address + bytes.length ≤ 65536) :
    ∃ next, writeMemory memory address bytes = .ok next ∧ next.length ≤ 65536 ∧
      view next = write (view memory) address bytes := by
  obtain ⟨active,expanded,within,cover⟩ := expand_fits memory address bytes.length bound fits
  refine ⟨writeBytes active address bytes, ?_, by simpa using within, ?_⟩
  · simp [writeMemory, expanded, Bind.bind, Except.bind, Pure.pure, Except.pure]
  · rcases cover with empty | covered
    · have none : bytes = [] := List.length_eq_zero_iff.mp empty
      subst bytes
      rw [show writeBytes active address [] = active by
        simp only [writeBytes, List.length_nil, Nat.add_zero, List.append_nil, List.take_nil, List.take_append_drop]]
      rw [expand_view _ _ _ _ expanded]
      funext i
      have outside : ¬ (address ≤ i ∧ i < address + ([] : List Byte).length) := by simp
      simp only [write, dite_eq_right outside]
    · rw [write_view _ _ _ covered, expand_view _ _ _ _ expanded]

def alignExtent (bytes : Bool) (size : Nat) : Nat := if bytes then round size else size

structure Layout (bytes : Bool) where
  memory : List Byte
  scratch : Nat
  object : Nat
  length : Word
  placed : 32 ≤ object
  privateObject : object + 32 + (if bytes then length.toNat else 32 * length.toNat) ≤ scratch
  header : ∀ i, ∀ h : i < 32, view memory (object + i) =
    (wordBytes length)[i]'(by simpa [wordBytes_length] using h)
  cursor : (readWord memory 64).toNat = scratch
  bound : memory.length ≤ 65536
  objectFits : object + (if bytes then length.toNat + 64 else 32*length.toNat+32) ≤ 65536
  scratchFits : scratch + 64 + alignExtent bytes (if bytes then length.toNat else 32*length.toNat) ≤ 65536

def Layout.size (input : Layout bytes) : Nat := if bytes then input.length.toNat else 32 * input.length.toNat

def clearMemory (enabled : Bool) (memory : List Byte) (address : Nat) : Except String (List Byte) :=
  if enabled then writeMemory memory address (wordBytes 0) else .ok memory

def sourceMemory (input : Layout bytes) : Except String (List Byte) := do
  let st ← writeMemory input.memory input.scratch (wordBytes 32)
  let st ← clearMemory bytes st (input.scratch + 32 + alignExtent bytes input.size)
  let st ← writeMemory st (input.scratch + 32) (wordBytes input.length)
  let readFrom ← expand st (input.object + 32) input.size
  writeMemory readFrom (input.scratch + 64) (readBytes readFrom (input.object + 32) input.size)

def reusedMemory (input : Layout bytes) : Except String (List Byte) := do
  let st ← clearMemory bytes input.memory (input.object + 32 + input.size)
  writeMemory st (input.object - 32) (wordBytes 32)


theorem aligned_at_least (bytes : Bool) (size : Nat) : size ≤ alignExtent bytes size := by
  cases bytes
  · simp [alignExtent]
  · exact (round_bound size).1

theorem source_view (input : Layout bytes) :
    ∃ memory, sourceMemory input = .ok memory ∧
      view memory = copying (view input.memory) input.scratch input.object input.size
        (alignExtent bytes input.size) input.length bytes := by
  let p := input.scratch
  let q := input.object
  let n := input.size
  let padded := alignExtent bytes n
  have fit : p + 64 + padded ≤ 65536 := input.scratchFits
  have obj : q + 32 + n ≤ p := input.privateObject
  have noShrink : n ≤ padded := aligned_at_least bytes n
  obtain ⟨offset, hOffset, offsetBound, offsetView⟩ :=
    writeMemory_view input.memory (wordBytes 32) p input.bound (by simp only [wordBytes_length]; omega)
  obtain ⟨cleared, hClear, clearBound, clearView⟩ :
      ∃ cleared, (if bytes then writeMemory offset (p + 32 + padded) (wordBytes 0) else .ok offset)
        = .ok cleared ∧ cleared.length ≤ 65536 ∧
        view cleared = if bytes then write (view offset) (p + 32 + padded) (wordBytes 0) else view offset := by
    by_cases h : bytes
    · simpa [h] using writeMemory_view offset (wordBytes 0) (p+32+padded) offsetBound
        (by simp only [wordBytes_length]; omega)
    · exact ⟨offset, by simp [h], offsetBound, by simp [h]⟩
  obtain ⟨header, hHeader, headerBound, headerView⟩ :=
    writeMemory_view cleared (wordBytes input.length) (p+32) clearBound (by simp only [wordBytes_length]; omega)
  obtain ⟨available, hAvailable, availableBound, _⟩ :=
    expand_fits header (q+32) n headerBound (by omega)
  let data := readBytes available (q+32) n
  have dataLength : data.length = n := by
    change (readBytes available (q+32) n).length = n
    rw [← read_view, read_length]
  obtain ⟨copied, hCopy, _, copyView⟩ :=
    writeMemory_view available data (p+64) availableBound (by rw [dataLength]; omega)
  refine ⟨copied, ?_, ?_⟩
  · have cleared := hClear
    change clearMemory bytes offset (p+32+padded) = .ok _ at cleared
    dsimp only [p,q,n,padded,data] at hOffset cleared hHeader hAvailable hCopy
    simpa only [sourceMemory, Bind.bind, Except.bind, hOffset, cleared, hHeader, hAvailable] using hCopy
  · have vAvailable := expand_view _ _ _ _ hAvailable
    change view copied = copying (view input.memory) p q n padded input.length bytes
    rw [copyView, show data = read (view available) (q+32) n by exact (read_view _ _ _).symm,
      vAvailable, headerView, clearView, offsetView] <;> rfl

theorem reuse_view (input : Layout bytes) :
    ∃ memory, reusedMemory input = .ok memory ∧
      view memory = reuse (view input.memory) input.object input.size bytes := by
  let q := input.object
  let n := input.size
  have fit : q + (if bytes then n + 64 else n + 32) ≤ 65536 := by
    by_cases h : bytes <;> simpa [q,n,Layout.size,h] using input.objectFits
  obtain ⟨cleared, hClear, clearBound, clearView⟩ :
      ∃ cleared, (if bytes then writeMemory input.memory (q + 32 + n) (wordBytes 0) else .ok input.memory)
        = .ok cleared ∧ cleared.length ≤ 65536 ∧
        view cleared = if bytes then write (view input.memory) (q + 32 + n) (wordBytes 0) else view input.memory := by
    by_cases h : bytes
    · have within : q + (n+64) ≤ 65536 := by simpa [h] using fit
      simpa [h] using writeMemory_view input.memory (wordBytes 0) (q+32+n)
        input.bound (by simp only [wordBytes_length]; omega)
    · exact ⟨input.memory, by simp [h], input.bound, by simp [h]⟩
  have qFit : q ≤ 65536 := by
    cases bytes <;> simp_all <;> omega
  obtain ⟨sealed, hSeal, _, sealView⟩ := writeMemory_view cleared (wordBytes 32)
    (q-32) clearBound (by simp only [wordBytes_length]; have := input.placed; omega)
  refine ⟨sealed, ?_, ?_⟩
  · have cleared := hClear
    change clearMemory bytes input.memory (q+32+n) = .ok _ at cleared
    dsimp only [q,n] at cleared hSeal
    simpa only [reusedMemory, Bind.bind, Except.bind, cleared] using hSeal
  · rw [sealView, clearView] <;> rfl

def sourceOutput (input : Layout bytes) : Except String (List Byte) := do
  let memory ← sourceMemory input
  return readBytes memory input.scratch (64 + alignExtent bytes input.size)

def reusedOutput (input : Layout bytes) : Except String (List Byte) := do
  let memory ← reusedMemory input
  return readBytes memory (input.object-32) (64 + alignExtent bytes input.size)

theorem output (input : Layout bytes) : sourceOutput input = reusedOutput input := by
  obtain ⟨source,sourceReady,sourceView⟩ := source_view input
  obtain ⟨target,targetReady,targetView⟩ := reuse_view input
  unfold sourceOutput reusedOutput
  rw [sourceReady, targetReady]
  simp only [Bind.bind, Except.bind, Pure.pure, Except.pure]
  rw [← read_view source, ← read_view target, sourceView, targetView]
  exact congrArg Except.ok
    (same _ _ _ _ input.length bytes input.placed input.privateObject input.header)


variable {bytes : Bool}

theorem length_word (input : Layout bytes) :
    readWord input.memory input.object = input.length := by
  have header : readBytes input.memory input.object 32 = wordBytes input.length := by
    rw [← read_view]
    apply List.ext_getElem
    · simp
    · intro index left right
      have within : index < 32 := by simpa using left
      rw [read_at _ _ _ _ within]
      exact input.header index within
  unfold readWord
  rw [header, ← BitVec.flattenList_eq_flattenListFast, flatten_wordBytes]

/-- At a terminal cut, bounded reads use the zero-extended view. The
private active memory size is discarded along with the frame. -/
def handleMemory (request : Request) (memory : List Byte) :
    Except (Except String (List Byte)) (Word × List Byte) :=
  match request.operation, request.arguments, request.returnsWord with
  | .opcode 0x51, [address], true => .ok (readWord memory address.toNat, memory)
  | .opcode 0x52, [address, word], false =>
    match writeMemory memory address.toNat (wordBytes word) with
    | .ok next => .ok (0, next)
    | .error message => .error (.error message)
  | _, _, _ => .error (.error "unexpected ABI return effect")

def finishMemory (boundary : Boundary) (memory : List Byte) : Except String (List Byte) :=
  match boundary.kind, boundary.values with
  | "returndata", [(_, start), (_, size)] => .ok (readBytes memory start.toNat size.toNat)
  | _, _ => .error "unexpected ABI return boundary"

def runTail (tree : Tree) (memory : List Byte) : Except String (List Byte) :=
  interpret handleMemory finishMemory tree memory

private theorem start_nat (object : Nat) (lower : 32 ≤ object) (upper : object < 2^256) :
    (BitVec.ofNat 256 object - 32).toNat = object - 32 := by
  have lowerWord : (32 : Word) ≤ BitVec.ofNat 256 object := by
    change (32 : Word).toNat ≤ (BitVec.ofNat 256 object).toNat
    simpa [Nat.mod_eq_of_lt upper] using lower
  rw [BitVec.toNat_sub_of_le lowerWord]
  simp [Nat.mod_eq_of_lt upper]

private theorem rounded_extent (length : Word) (fits : length.toNat + 63 < 2^256) :
    ((length + 31) &&& ~~~(31 : Word)) = BitVec.ofNat 256 (round length.toNat) := by
  have equal := LowerReturndata.roundedSize length.toNat fits
  rw [BitVec.ofNat_toNat] at equal
  have adjacent : (length.toNat + 63) / 32 * 32 = round length.toNat + 32 := by
    unfold round
    omega
  rw [adjacent, BitVec.ofNat_add] at equal
  change ((length + 31) &&& ~~~(31 : Word)) + 32 =
    BitVec.ofNat 256 (round length.toNat) + 32 at equal
  simpa only [BitVec.add_sub_cancel] using congrArg (fun (word : Word) => word - 32) equal

theorem run_reuse {bytes : Bool} (input : Layout bytes) :
    runTail (reuseReturn bytes (BitVec.ofNat 256 input.object)) input.memory = reusedOutput input := by
  have placed := input.placed
  have scratchFits := input.scratchFits
  have below := input.privateObject
  have fit : input.object + 32 + input.size ≤ 65536 := by
    simp only [Layout.size]
    omega
  have bounded : input.object < 2^256 := by omega
  have start := start_nat input.object placed bounded
  have objectNat : (BitVec.ofNat 256 input.object).toNat = input.object := by
    simp [Nat.mod_eq_of_lt bounded]
  have total (extent : Nat) (extentFits : extent + 64 ≤ 65536) :
      (BitVec.ofNat 256 extent + 64).toNat = 64 + extent := by
    simp [BitVec.toNat_add, Nat.mod_eq_of_lt (by omega : extent < 2^256),
      Nat.mod_eq_of_lt (by omega : extent + 64 < 2^256), Nat.add_comm]
  cases bytes
  · have sizeFits : 32 * input.length.toNat + 64 ≤ 65536 := by
      simp only [Bool.false_eq_true, ite_false, alignExtent] at scratchFits
      omega
    have lengthShift : input.length <<< 5 = BitVec.ofNat 256 (32 * input.length.toNat) := by
      apply BitVec.eq_of_toNat_eq
      simp [BitVec.toNat_shiftLeft, Nat.shiftLeft_eq, Nat.mul_comm]
    simp only [runTail, reuseReturn, load, interpret, objectNat, handleMemory, length_word,
      Bool.false_eq_true, ite_false, store, start, finishMemory, reusedOutput, reusedMemory,
      clearMemory, Layout.size, alignExtent, lengthShift, total _ sizeFits, Except.bind, Bind.bind]
    cases writeMemory input.memory (input.object - 32) (wordBytes 32) <;> rfl
  · have sizeFits : round input.length.toNat + 64 ≤ 65536 := by
      simp only [ite_true, alignExtent] at scratchFits
      omega
    have lengthBound : input.length.toNat + 63 < 2^256 := by
      simp only [Layout.size, ite_true] at fit
      omega
    have clearNat : (BitVec.ofNat 256 input.object + 32 + input.length).toNat =
        input.object + 32 + input.length.toNat := by
      have midFits : input.object + 32 < 2^256 := by omega
      have endFits : input.object + 32 + input.length.toNat < 2^256 := by
        simp only [Layout.size, ite_true] at fit
        omega
      simp [BitVec.toNat_add, objectNat, Nat.mod_eq_of_lt midFits, Nat.mod_eq_of_lt endFits]
    simp only [runTail, reuseReturn, load, interpret, objectNat, handleMemory, length_word,
      ite_true, store, start, finishMemory, reusedOutput, reusedMemory, clearMemory, Layout.size,
      alignExtent, clearNat, rounded_extent _ lengthBound, total _ sizeFits, Except.bind, Bind.bind]
    cases writeMemory input.memory (input.object + 32 + input.length.toNat) (wordBytes 0) with
    | error message => rfl
    | ok memory =>
      dsimp only
      cases writeMemory memory (input.object - 32) (wordBytes 32) <;> rfl

end SolarTV.MIR.LowerAbiEncode.ZeroCopy
