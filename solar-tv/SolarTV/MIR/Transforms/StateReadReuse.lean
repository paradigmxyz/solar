import SolarTV.MIR.Transforms.DceEffects
import SolarTV.MIR.Transforms.PhaseShaping
import SolarTV.MIR.State.Observe

/-! A repeated state read can reuse its dominating value if the intervening
actions preserve the relevant storage cell or active memory bytes. -/
namespace SolarTV.MIR.StateReadReuse
open EvmRules SolarTV.Execution MIR.State

theorem warmSlot_known (world : Journal) (owner : Address) (key : Word)
    (warm : world.warmSlots.contains (owner,key) = true) :
    warmSlot world owner key = world := by
  have member : (owner,key) ∈ world.warmSlots := by simpa using warm
  cases world
  simp_all [warmSlot]

theorem warmSlot_contains (world : Journal) (owner : Address) (key : Word) :
    (warmSlot world owner key).warmSlots.contains (owner,key) = true := by
  by_cases warm : world.warmSlots.contains (owner,key) = true
  · rw [warmSlot_known _ _ _ warm]
    exact warm
  · have absent : (owner,key) ∉ world.warmSlots := by simpa using warm
    simp [warmSlot, absent]

theorem warmSlot_preserved (world : Journal) (owner other : Address) (key slot : Word)
    (warm : world.warmSlots.contains (owner,key) = true) :
    (warmSlot world other slot).warmSlots.contains (owner,key) = true := by
  have member : (owner,key) ∈ world.warmSlots := by simpa using warm
  by_cases present : (other,slot) ∈ world.warmSlots <;> simp [warmSlot, present, member]

theorem storage_read (cfg : Config) (frame : Frame) (key word : Word) (transient : Bool)
    (found : slotsRead (if transient then (account frame.world cfg.environment.address).transient
      else (account frame.world cfg.environment.address).storage) key = word)
    (warm : transient = false → frame.world.warmSlots.contains (cfg.environment.address,key) = true) :
    storageOperation cfg frame transient false [key] = value frame word := by
  cases transient <;> simp_all [storageOperation, warmSlot_known]

theorem storage_write_other (slots : Slots) (read written value : Word)
    (different : read ≠ written) :
    slotsRead (slotsWrite slots written value) read = slotsRead slots read := by
  have beq : (written == read) = false := beq_eq_false_iff_ne.mpr different.symm
  have predicate : (fun pair : Word × Word =>
      decide ((pair.1 != written) = true ∧ (pair.1 == read) = true)) =
      (fun pair => pair.1 == read) := by
    funext pair
    by_cases key : pair.1 = read <;> simp [key, different]
  simp only [slotsRead, slotsWrite, List.find?_cons, beq, List.find?_filter, predicate]

theorem expanded_read (memory grown : List Byte) (offset size source : Nat)
    (covered : source + 32 ≤ memory.length) (success : expand memory offset size = .ok grown) :
    source + 32 ≤ grown.length ∧ readWord grown source = readWord memory source := by
  simp only [expand] at success
  split at success
  · contradiction
  · cases success
    exact ⟨by simp; omega, readWord_append_prefix _ _ _ covered⟩

theorem expanded_extent (memory grown : List Byte) (offset size : Nat)
    (aligned : memory.length % 32 = 0) (nonempty : 0 < size)
    (success : expand memory offset size = .ok grown) :
    grown.length % 32 = 0 ∧ grown.length ≤ 65536 ∧ offset + size ≤ grown.length := by
  have ⟨length, bound⟩ := DceEffects.expanded_size memory grown offset size aligned success
  have covers : (offset + size + 31) / 32 ≤ requiredWords memory offset size := by
    simp only [requiredWords, Nat.ne_of_gt nonempty, ite_false]
    exact Nat.le_max_right _ _
  exact ⟨by omega, bound, by omega⟩

theorem store_word (cfg : Config) (frame next : Frame) (offset word : Word)
    (aligned : frame.memory.length % 32 = 0)
    (success : memoryOperation cfg frame .store [offset,word] = effect next) :
    next.memory.length % 32 = 0 ∧ next.memory.length ≤ 65536 ∧
      offset.toNat + 32 ≤ next.memory.length ∧ readWord next.memory offset.toNat = word := by
  simp only [memoryOperation, wordBytes_length, effect, Bind.bind, Except.bind] at success
  split at success
  · contradiction
  · rename_i grown expansion
    cases success
    have ⟨a,b,c⟩ := expanded_extent _ _ _ 32 aligned (by decide) expansion
    simp only [writeBytes_length]
    exact ⟨a,b,c,readWord_writeWord _ _ _ c⟩

/-- An admitted store activates the entire word and establishes its byte image. -/
theorem stored_bytes (cfg : Config) (frame next : Frame) (offset word : Word)
    (aligned : frame.memory.length % 32 = 0)
    (success : memoryOperation cfg frame .store [offset,word] = effect next) :
    offset.toNat + 32 ≤ next.memory.length ∧
      readBytes next.memory offset.toNat 32 = wordBytes word := by
  simp only [memoryOperation, wordBytes_length, effect, Bind.bind, Except.bind] at success
  split at success
  · contradiction
  · rename_i grown expansion
    cases success
    have covered := (expanded_extent _ _ _ 32 aligned (by decide) expansion).2.2
    simp only [writeBytes_length]
    exact ⟨covered, by
      simpa only [wordBytes_length] using
        readBytes_writeBytes_same grown (wordBytes word) offset.toNat (by simpa using covered)⟩

/-- A repeated store writes the existing byte image and cannot expand memory. -/
theorem repeated_store (cfg : Config) (frame : Frame) (offset word : Word)
    (aligned : frame.memory.length % 32 = 0) (bounded : frame.memory.length ≤ 65536)
    (covers : offset.toNat + 32 ≤ frame.memory.length)
    (same : readBytes frame.memory offset.toNat 32 = wordBytes word) :
    memoryOperation cfg frame .store [offset,word] = effect frame := by
  have rounded : 32 * (frame.memory.length / 32) = frame.memory.length := by omega
  have fits : (offset.toNat + 32 + 31) / 32 ≤ frame.memory.length / 32 := by omega
  have write := writeBytes_existing frame.memory (wordBytes word) offset.toNat (by simpa using same)
  simp only [memoryOperation, wordBytes_length, expand, requiredWords, memoryWords,
    Nat.reduceEqDiff, ite_false, Nat.max_eq_left fits, rounded, Nat.not_lt.mpr bounded,
    Nat.sub_self, List.replicate_zero, List.append_nil, Except.bind, Bind.bind, write]

def stopStores (cfg : Config) : List (Word × Word) → Frame → Except String Observation
  | [], frame => .ok (observe (finish (.success []) frame))
  | (offset, word) :: rest, frame => do
    let .returned (_, next) ← memoryOperation cfg frame .store [offset,word]
      | .error "unexpected terminal store outcome"
    stopStores cfg rest next

/-- After STOP, the enclosing activation has no continuation that can read
the stores. A bounded literal interval ensures that the interpreter evaluates
each store instead of failing at its memory work limit. -/
theorem stop_stores (cfg : Config) (stores : List (Word × Word)) (frame : Frame)
    (aligned : frame.memory.length % 32 = 0) (bounded : frame.memory.length ≤ 65536)
    (addresses : ∀ pair ∈ stores, pair.1.toNat + 32 ≤ 65536) :
    stopStores cfg stores frame = .ok (observe (finish (.success []) frame)) := by
  induction stores generalizing frame with
  | nil => rfl
  | cons pair rest ih =>
    obtain ⟨offset,word⟩ := pair
    have room := addresses (offset,word) (by simp)
    change offset.toNat + 32 ≤ 65536 at room
    have cap : 32 * requiredWords frame.memory offset.toNat 32 ≤ 65536 := by
      simp only [requiredWords, memoryWords, Nat.reduceEqDiff, ite_false]
      omega
    let grown := frame.memory ++ List.replicate
      (32 * requiredWords frame.memory offset.toNat 32 - frame.memory.length) 0
    have expanded : expand frame.memory offset.toNat 32 = .ok grown := by
      simp [expand, Nat.not_lt.mpr cap, grown]
    obtain ⟨nextAligned,nextBounded,_⟩ := expanded_extent _ _ _ _ aligned (by decide) expanded
    let next := {frame with memory := writeBytes grown offset.toNat (wordBytes word)}
    have stored : memoryOperation cfg frame .store [offset,word] = effect next := by
      simp [memoryOperation, expanded, Bind.bind, Except.bind, next]
    have keep : observe (finish (.success []) next) = observe (finish (.success []) frame) := rfl
    simp only [stopStores, stored, effect, Bind.bind, Except.bind]
    rw [ih next (by simpa [next] using nextAligned) (by simpa [next] using nextBounded)
      (fun pair member => addresses pair (List.mem_cons_of_mem _ member)), keep]

theorem fixed_stop_stores (cfg : Config) (offsets values : List Word) (frame : Frame)
    (aligned : frame.memory.length % 32 = 0) (bounded : frame.memory.length ≤ 65536)
    (addresses : offsets.all (fun offset => decide (offset.toNat + 32 ≤ 65536)) = true) :
    stopStores cfg (offsets.zip values) frame = .ok (observe (finish (.success []) frame)) := by
  apply stop_stores cfg _ frame aligned bounded
  intro pair member
  obtain ⟨offset,word⟩ := pair
  exact of_decide_eq_true (List.all_eq_true.mp addresses offset (List.of_mem_zip member).1)

theorem load_word (cfg : Config) (frame next : Frame) (offset word : Word)
    (aligned : frame.memory.length % 32 = 0)
    (success : memoryOperation cfg frame .load [offset] = value next word) :
    next.memory.length % 32 = 0 ∧ next.memory.length ≤ 65536 ∧
      offset.toNat + 32 ≤ next.memory.length ∧ readWord next.memory offset.toNat = word := by
  simp only [memoryOperation, value, Bind.bind, Except.bind] at success
  split at success
  · contradiction
  · rename_i grown expansion
    cases success
    exact ⟨(expanded_extent _ _ _ 32 aligned (by decide) expansion).1,
      (expanded_extent _ _ _ 32 aligned (by decide) expansion).2.1,
      (expanded_extent _ _ _ 32 aligned (by decide) expansion).2.2,rfl⟩

theorem word_write_disjoint (memory bytes : List Byte) (written source : Nat)
    (covered : source + 32 ≤ memory.length)
    (different : written + bytes.length ≤ source ∨ source + 32 ≤ written) :
    source + 32 ≤ (writeBytes memory written bytes).length ∧
      readWord (writeBytes memory written bytes) source = readWord memory source := by
  rw [writeBytes_length, readWord_writeBytes_disjoint _ _ _ _ different]
  exact ⟨covered,rfl⟩

theorem relative_disjoint (source : Word) (delta : Nat) (covered : source.toNat + 32 ≤ 65536)
    (later : 32 ≤ delta) (near : delta ≤ 65536) :
    source.toNat + 32 ≤ (source + BitVec.ofNat 256 delta).toNat := by
  have fits : delta < 2^256 := by omega
  have sum : source.toNat + delta < 2^256 := by omega
  simp only [BitVec.toNat_add, BitVec.toNat_ofNat, Nat.mod_eq_of_lt fits, Nat.mod_eq_of_lt sum]
  omega

private theorem account_address (world : Journal) (owner : Address) :
    (account world owner).address = owner := by
  unfold account
  cases found : world.accounts.find? (fun a => a.address == owner)
  · rfl
  · rename_i actual
    have matched : (actual.address == owner) = true :=
      List.find?_some (p := fun a : Account => a.address == owner) found
    exact beq_iff_eq.mp matched

private theorem account_put (world : Journal) (updated : Account) (owner : Address)
    (same : updated.address = owner) :
    account (putAccount world updated) owner = updated := by
  simp [account, putAccount, same]

theorem storage_written (cfg : Config) (frame next : Frame) (key word : Word)
    (transient : Bool)
    (success : storageOperation cfg frame transient true [key,word] = effect next) :
    slotsRead (if transient then (account next.world cfg.environment.address).transient
      else (account next.world cfg.environment.address).storage) key = word ∧
      (transient = false → next.world.warmSlots.contains (cfg.environment.address,key) = true) := by
  simp only [storageOperation, effect, Pure.pure, Except.pure] at success
  split at success
  · cases success
  · cases transient
    · cases success
      have warm := warmSlot_contains frame.world cfg.environment.address key
      constructor
      · simp [account_put, account_address]
      · exact fun _ => warm
    · cases success
      simp [account_put, account_address]

private theorem read_prefix (memory : List Byte) (count : Nat) (fits : count ≤ memory.length) :
    readBytes memory 0 count = memory.take count := by
  apply List.ext_getElem
  · simp [readBytes, Nat.min_eq_left fits]
  · intro index found _
    have inMemory : index < memory.length := by simp [readBytes] at found; omega
    simp [readBytes, List.getElem?_eq_getElem inMemory]

def mappingBytes (key slot : Word) := wordBytes key ++ wordBytes slot

theorem mapping_reuse (frame : Frame) (key slot : Word) (hash : List Byte → Word)
    (aligned : frame.memory.length % 32 = 0) (bounded : frame.memory.length ≤ 65536)
    (covered : 64 ≤ frame.memory.length)
    (bytes : readBytes frame.memory 0 64 = mappingBytes key slot) :
    fixedSlot frame [key,slot] hash = .ok (hash (mappingBytes key slot),frame) := by
  have active := MIR.ContinuedCounter.expandAligned frame.memory 0 64
    (by decide) aligned (by decide) bounded (by decide)
  rw [Nat.zero_add, MIR.ContinuedCounter.padCovered _ _ covered] at active
  have keep : writeBytes frame.memory 0 (mappingBytes key slot) = frame.memory := by
    rw [← bytes, read_prefix _ _ covered]
    simp [writeBytes, List.take_take, Nat.min_eq_left covered, Nat.min_eq_right covered]
  simp [fixedSlot, mappingBytes, active, Bind.bind, Except.bind, Pure.pure, Except.pure] at keep ⊢
  rw [keep]

theorem mapping_sets (frame next : Frame) (key slot word : Word)
    (hash : List Byte → Word) (aligned : frame.memory.length % 32 = 0)
    (success : fixedSlot frame [key,slot] hash = .ok (word,next)) :
    word = hash (mappingBytes key slot) ∧ next.memory.length % 32 = 0 ∧
      next.memory.length ≤ 65536 ∧ 64 ≤ next.memory.length ∧
      readBytes next.memory 0 64 = mappingBytes key slot := by
  simp only [fixedSlot, List.flatMap_cons, List.flatMap_nil, List.append_nil,
    List.length_append, wordBytes_length, Nat.reduceAdd, Bind.bind, Except.bind,
    Pure.pure, Except.pure] at success
  split at success
  · contradiction
  · rename_i grown expansion
    cases success
    have ⟨a,b,c⟩ := expanded_extent _ _ 0 64 aligned (by decide) expansion
    simp only [writeBytes_length]
    have matchBytes := readBytes_writeBytes_same grown (mappingBytes key slot) 0 (by
      simpa [mappingBytes] using c)
    simp only [mappingBytes, List.length_append, wordBytes_length, Nat.reduceAdd] at matchBytes
    exact ⟨rfl,a,b,c,matchBytes⟩

theorem distinct_offsets (base left right : Word) (different : left ≠ right) :
    base + left ≠ base + right := by
  intro same
  apply different
  have cancel := congrArg (· - base) same
  simpa only [BitVec.add_comm base, BitVec.add_sub_cancel] using cancel

theorem require_continues (cfg : Config) (frame next : Frame) (op : RequireOp)
    (args : List Value) (success : abiRequirePayload cfg frame op args = effect next) :
    next = frame := by
  simp only [abiRequirePayload, effect, Bind.bind, Except.bind, Pure.pure, Except.pure] at success
  split at success <;> try contradiction
  split at success <;> try contradiction
  split at success
  · cases success
    rfl
  · split at success
    all_goals
      repeat (first | split at success | cases success)

theorem publish_scalar (cfg : Config) (owner : Function) (word : Value) (frame : Frame)
    (ty : ValueType) (returns : owner.returns = [ty]) (scalar : ty.fields = none) :
    publishReturns cfg owner [word] frame = .ok frame := by
  unfold publishReturns
  by_cases placed :
      (owner.returnBuffer.bind (fun name =>
        (cfg.layout.functions.find? (·.1 == name)).bind (·.2.returnBuffer))).isNone
  · simp [placed, Pure.pure, Except.pure]
  · have leaves : returnLeaves 34 ty word = .ok [word] := by
      rw [show (34 : Nat) = 33 + 1 by decide, returnLeaves, scalar]
      rfl
    simp [placed, returns, leaves, Bind.bind, Except.bind, Pure.pure, Except.pure]

/-- A fact is true at the reuse point if all its incoming paths either walk
through the producer or remain inside the checked backward slice. -/
structure Site where
  producer : Nat
  consumer : Nat
  slice : Nat

def Site.contains (site : Site) (point : Nat) : Bool := site.slice.testBit point

abbrev Edges := List (Nat × Nat)

def check (edges : Edges) (site : Site) : Bool :=
  site.contains site.consumer && !site.contains 0 &&
    edges.all fun (source,target) =>
      !site.contains target || source == site.producer || site.contains source

inductive Runs {α : Type} (edges : Edges) (step : Nat → Nat → α → α → Prop) : Nat → α → Prop where
  | entry (state : α) : Runs edges step 0 state
  | next {source target before after}
      (previous : Runs edges step source before) (edge : (source,target) ∈ edges)
      (action : step source target before after) : Runs edges step target after

theorem invariant {α : Type} (edges : Edges) (site : Site) (verified : check edges site = true)
    (step : Nat → Nat → α → α → Prop) (holds : α → Prop)
    (establish : ∀ {target before after}, step site.producer target before after → holds after)
    (preserve : ∀ {source target before after}, site.contains source = true →
      step source target before after → holds before → holds after)
    {state : α} (run : Runs edges step site.consumer state) : holds state := by
  simp only [check, Bool.and_eq_true, Bool.not_eq_true'] at verified
  obtain ⟨⟨consumer, entry⟩, closed⟩ := verified
  have atSlice {pc : Nat} {state : α} (path : Runs edges step pc state) :
      site.contains pc = true → holds state := by
    induction path with
    | entry _ =>
      intro inside
      rw [entry] at inside
      contradiction
    | @next source target before after _ edge action ih =>
      intro inside
      have condition := List.all_eq_true.mp closed _ edge
      simp only [inside, Bool.not_true, Bool.false_or, Bool.or_eq_true, beq_iff_eq] at condition
      rcases condition with equality | prior
      · subst source
        exact establish action
      · exact preserve prior action (ih prior)
  exact atSlice run consumer

end SolarTV.MIR.StateReadReuse
