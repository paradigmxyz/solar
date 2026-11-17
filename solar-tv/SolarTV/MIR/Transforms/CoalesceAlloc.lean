import SolarTV.State.MemoryLaws

/-! Contextual allocation-coalescing certificates.

The admitted prefix only reserves uninitialized memory and initializes full
words at offsets from its initial allocation cursor. All addresses are aligned,
inside the reserved extent and disjoint from the free-pointer slot. There are
no intermediate pointer observations, calls, zeroing, checked failures or reads.
Reservations may move earlier; ordered data writes, final cursor and active
memory must agree before the unchanged continuation runs.

Byte contents and active memory are arbitrary. The producer must supply a
well-formed FMP and a nonwrapping reserved extent; neither a default cursor nor
successful execution of the continuation is assumed. The source projection and
its binding to these byte-memory steps remain trusted. This unpriced prefix
certificate does not certify physical gas or backend placement.
-/

namespace SolarTV.MIR.CoalesceAlloc
open EvmRules SolarTV.Execution

abbrev Memory := Nat → Byte

def write (memory : Memory) (address : Nat) (value : Word) : Memory :=
  fun i => if address ≤ i ∧ i < address + 32 then
    (wordBytes value)[i-address]?.getD 0 else memory i

theorem overwrite (memory : Memory) (address : Nat) (a b : Word) :
    write (write memory address a) address b = write memory address b := by
  funext i
  by_cases h : address ≤ i ∧ i < address + 32 <;> simp [write,h]

theorem commute (memory : Memory) (address : Nat) (a b : Word)
    (disjoint : 96 ≤ address) :
    write (write memory 64 a) address b = write (write memory address b) 64 a := by
  funext i
  simp only [write]
  by_cases slot : 64 ≤ i ∧ i < 64 + 32
  · have outside : ¬ (address ≤ i ∧ i < address + 32) := by omega
    simp [slot,outside]
  · simp [slot]

/-- In the active range this functional write is the shared byte-list write. -/
theorem write_list (memory : List Byte) (address i : Nat) (value : Word)
    (active : i < memory.length) :
    write (fun j => memory[j]?.getD 0) address value i =
      (writeBytes memory address (wordBytes value))[i]?.getD 0 := by
  simp [write,writeBytes_eq_spec,writeBytesSpec,List.getElem?_mapIdx,
    List.getElem?_eq_getElem active]

def readCursor (memory : Memory) : Word :=
  (BitVec.flattenListFast ((List.range 32).map (fun i => memory (64+i)))).setWidth 256

theorem readCursor_write (memory : Memory) (value : Word) :
    readCursor (write memory 64 value) = value := by
  have bytes : (List.range 32).map (fun i => write memory 64 value (64+i)) = wordBytes value := by
    apply List.ext_getElem
    · simp
    · intro i hi hj
      have bound : i < 32 := by simpa using hi
      have inside : 64 ≤ 64+i ∧ 64+i < 64+32 := by omega
      simp [write,inside,List.getElem?_eq_getElem hj]
  unfold readCursor
  rw [bytes,← BitVec.flattenList_eq_flattenListFast]
  exact flatten_wordBytes value

theorem readCursor_disjoint (memory : Memory) (address : Nat) (value : Word)
    (safe : 96 ≤ address) : readCursor (write memory address value) = readCursor memory := by
  unfold readCursor
  congr 2
  apply List.map_congr_left
  intro i present
  have bound := List.mem_range.mp present
  have outside : ¬ (address ≤ 64+i ∧ 64+i < address+32) := by omega
  simp [write,outside]

inductive Step where
  | reserve (size : Nat)
  | store (offset : Nat) (value : Word)
  deriving DecidableEq

structure Result where
  cursor : Nat
  memory : Memory
  active : Nat

def run (base : Nat) : List Step → Result → Result
  | [], state => state
  | .reserve size :: rest, state =>
    let cursor := state.cursor + size
    run base rest {state with cursor, memory := write state.memory 64 (BitVec.ofNat 256 cursor)}
  | .store offset value :: rest, state =>
    let address := base + offset
    run base rest {state with
      memory := write state.memory address value
      active := max state.active (address + 32)}

def extent : List Step → Nat
  | [] => 0
  | .reserve size :: rest => size + extent rest
  | .store _ _ :: rest => extent rest

def stores : List Step → List (Nat × Word)
  | [] => []
  | .reserve _ :: rest => stores rest
  | .store offset value :: rest => (offset,value) :: stores rest

def fill (base : Nat) : List (Nat × Word) → Result → Result
  | [], state => state
  | (offset,value) :: rest, state =>
    fill base rest {state with
      memory := write state.memory (base+offset) value
      active := max state.active (base+offset+32)}

def stamp (cursor : Nat) (state : Result) : Result :=
  {state with cursor, memory := write state.memory 64 (BitVec.ofNat 256 cursor)}

theorem fill_stamp (base cursor : Nat) (writes : List (Nat × Word))
    (state : Result) (safe : 128 ≤ base) :
    fill base writes (stamp cursor state) = stamp cursor (fill base writes state) := by
  induction writes generalizing state with
  | nil => rfl
  | cons item rest ih =>
    obtain ⟨offset,value⟩ := item
    simp only [fill,stamp]
    rw [commute _ _ _ _ (by omega)]
    exact ih {state with
      memory := write state.memory (base+offset) value
      active := max state.active (base+offset+32)}

theorem normalize (base : Nat) (plan : List Step) (state : Result)
    (cursor : Nat) (safe : 128 ≤ base) :
    run base plan (stamp cursor state) =
      stamp (cursor + extent plan) (fill base (stores plan) state) := by
  induction plan generalizing cursor state with
  | nil => simp [run,extent,stores,fill]
  | cons step rest ih =>
    cases step with
    | reserve size =>
      simp only [run,stamp,overwrite,extent,stores]
      simpa [stamp,Nat.add_assoc] using ih state (cursor+size)
    | store offset value =>
      simp only [run,stamp,extent,stores,fill]
      rw [commute _ _ _ _ (by omega)]
      exact ih {state with
        memory := write state.memory (base+offset) value
        active := max state.active (base+offset+32)} cursor

/-- An allocation is required first: an absent reservation cannot silently
change the initial free-pointer bytes. -/
def allocated (plan : List Step) : Bool :=
  match plan with | .reserve _ :: _ => true | _ => false

/-- Validate each access against the reservation available at that point. -/
def wellFormed : Nat → List Step → Bool
  | _, [] => true
  | available, .reserve size :: rest =>
    size > 0 && size % 32 == 0 && available + size ≤ 65536 &&
      wellFormed (available+size) rest
  | available, .store offset _ :: rest =>
    offset % 32 == 0 && offset + 32 ≤ available && wellFormed available rest

/-- This is an explicit producer contract, not an inferred initial cursor.
The cached cursor denotes the word in bytes 0x40..0x5f; the source projection
must retain this correspondence. The total extent cannot wrap a machine word. -/
def ProducerContract (base bytes : Nat) (state : Result) : Prop :=
  128 ≤ base ∧ base % 32 = 0 ∧ base + bytes < 2^64 ∧
  state.cursor = base ∧ 96 ≤ state.active ∧ state.active % 32 = 0 ∧
  readCursor state.memory = BitVec.ofNat 256 base

theorem first_reservation (base : Nat) (plan : List Step) (state : Result)
    (safe : 128 ≤ base) (initial : state.cursor = base) (reserved : allocated plan = true) :
    run base plan state = stamp (base + extent plan) (fill base (stores plan) state) := by
  cases plan with
  | nil => simp [allocated] at reserved
  | cons step rest =>
    cases step with
    | store _ _ => simp [allocated] at reserved
    | reserve size =>
      simp only [run,extent,stores]
      have step := normalize base rest state (base+size) safe
      simpa [stamp,initial,Nat.add_assoc] using step

/-- Equal byte writes and reservation totals preserve every continuation,
including arbitrary stateful callees, failure and nontermination observations. -/
theorem contextual (before after : List Step) (base : Nat) (state : Result)
    (safe : 128 ≤ base) (initial : state.cursor = base)
    (left : allocated before = true) (right : allocated after = true)
    (size : extent before = extent after) (data : stores before = stores after)
    (continuation : Result → α) :
    continuation (run base before state) = continuation (run base after state) := by
  rw [first_reservation base before state safe initial left,
      first_reservation base after state safe initial right,size,data]

end SolarTV.MIR.CoalesceAlloc
