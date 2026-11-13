import SolarTV.Sem
import SolarTV.Memory

/-! The machine state MIR and EVM IR programs share, and the semantics of a layer.

Memory is the model of [Memory.lean](Memory.lean) on every byte: its active-word count,
expansion through `Memory.grow` with natural-number offsets, its `MSIZE` formula and
`Memory.byte` order. `Mem.project` is its 256-byte `Memory.State`, and the `project_`
lemmas show that every operation agrees with it there. Storage and transient storage
are total maps. The world is the history of the logs and calls the frame made, and of its
`GAS` reads. `CALLDATACOPY` and `MCOPY` write memory as `Mem.write`, after the expansion
of every range they touch, with calldata zero past its end.

What the frame cannot see is an `Oracle`: environment reads, and the result and the state
after a call, are functions of the world, as the pass validation encoder takes them to be
uninterpreted functions of its world. An external call (`CALL`, `CALLCODE`, `DELEGATECALL`,
`STATICCALL`) records what its callee sees: the gas, address and value words and the input
bytes, with the storage and transient storage it may reenter. Its success flag (0 or 1), the
size and bytes of the return data, and for every call but `STATICCALL` the storage and
transient storage after it are the oracle's, as functions of the world with the call; the
callee cannot touch the caller's memory, so memory only takes the return data at the output
range, at most as many bytes as the return data has, after the expansion of both ranges.
`RETURNDATACOPY` reads the same return data, and copying past its end is an exceptional halt. `KECCAK256` is the oracle's `keccak`, a function of the
hashed bytes alone that every layer shares, so the same bytes hash to the same word. A layer
runs a program on a call with fuel; its `Sem` is fuel-free, the end some fuel reaches. One
frame, no gas: `GAS` is an environment read, but one the world records, so that each read is
a function of a history no other read has and two reads need not agree. A stage then holds for
every gas the frame may have left at each read, as passes may change the gas the code uses;
it needs both programs to read gas in the same order among their logs and calls. An immutable
is the oracle's environment read `immutable` of its number in the empty world: the word the
deployment code wrote, the same wherever the frame reads it. So is the frame's code, byte by byte
(`code`), which `CODECOPY` copies like calldata, zero past its end; it holds the program data,
such as the creation code of a contract the frame creates, at the offsets the stages take from
the assembled code. `CREATE` and `CREATE2` record their value, salt, code and state in the world
as a call does, and the new address and the storage after them are the oracle's. -/

namespace SolarTV.Machine

open EvmRules

/-- Memory on every byte, with the active-word count of `Memory.State`. -/
structure Mem where
  bytes : Nat → Memory.Byte
  words : BitVec 251

namespace Mem

def initial : Mem := ⟨fun _ => 0, 0⟩

/-- The first 256 bytes and the active size, as the importers model memory. -/
def project (m : Mem) : Memory.State := ⟨(List.range 256).map m.bytes, m.words⟩

def expand (m : Mem) (offset size : Nat) : Mem :=
  if size = 0 then m
  else { m with words := Memory.grow m.words (BitVec.ofNat 251 ((offset + size + 31) / 32)) }

def msize (m : Mem) : Word := m.words.setWidth 256 <<< 5

def read (m : Mem) (offset size : Nat) : List Memory.Byte :=
  (List.range size).map (fun i => m.bytes (offset + i))

def load (m : Mem) (offset : Nat) : Word :=
  (m.read offset 32).foldl (fun acc b => (acc <<< 8) ||| b.setWidth 256) 0

def store (m : Mem) (offset : Nat) (value : Word) : Mem :=
  expand { m with bytes := fun i =>
    if offset ≤ i ∧ i < offset + 32 then Memory.byte value (i - offset) else m.bytes i } offset 32

def storeByte (m : Mem) (offset : Nat) (value : Word) : Mem :=
  expand { m with bytes := fun i => if i = offset then value.setWidth 8 else m.bytes i } offset 1

/-- `size` bytes of `source` written from `offset`, as a copy writes them. -/
def write (m : Mem) (offset size : Nat) (source : Nat → Memory.Byte) : Mem :=
  expand { m with bytes := fun i =>
    if offset ≤ i ∧ i < offset + size then source (i - offset) else m.bytes i } offset size

/-- A load reads bytes, which expansion leaves as they are. -/
theorem load_expand (m : Mem) (offset size address : Nat) :
    (m.expand offset size).load address = m.load address := by
  unfold expand
  split <;> rfl

/-- Expanding memory again over the same range changes nothing. -/
theorem expand_expand (m : Mem) (offset size : Nat) :
    (m.expand offset size).expand offset size = m.expand offset size := by
  unfold expand
  split <;> simp only [Memory.grow_idempotent]

/-- A store already expanded memory over its word, as a load of that word would. -/
theorem expand_store (m : Mem) (offset : Nat) (value : Word) :
    (m.store offset value).expand offset 32 = m.store offset value := by
  unfold store
  exact expand_expand _ _ _

/-- A load reads none of the bytes a write outside its word writes. -/
theorem load_write_of_disjoint (m : Mem) (offset size address : Nat) (source : Nat → Memory.Byte)
    (disjoint : address + 32 ≤ offset ∨ offset + size ≤ address) :
    (m.write offset size source).load address = m.load address := by
  unfold write
  rw [load_expand]
  unfold load read
  congr 1
  apply List.map_congr_left
  intro i mem
  simp only [List.mem_range] at mem
  have : ¬(offset ≤ address + i ∧ address + i < offset + size) := by omega
  simp only [this, if_false]

/-- One more byte of a word, from the top: the word shifted right by one byte less. -/
private theorem step_bytes (v : BitVec 256) (n : Nat) (h : n < 32) :
    ((v >>> (8 * (32 - n))) <<< 8) ||| (SolarTV.Memory.byte v n).setWidth 256 =
      v >>> (8 * (32 - (n + 1))) := by
  apply BitVec.eq_of_getLsbD_eq
  intro j hj
  simp only [SolarTV.Memory.byte, BitVec.getLsbD_or, BitVec.getLsbD_shiftLeft,
    BitVec.getLsbD_ushiftRight, BitVec.getLsbD_setWidth]
  by_cases hj8 : j < 8
  · simp only [hj8, decide_true, Bool.true_and, Bool.not_true, Bool.false_and, Bool.false_or, hj]
    have : 8 * (31 - n) + j = 8 * (32 - (n + 1)) + j := by omega
    rw [this]
  · have h1 : ¬ (j < 8) := hj8
    simp only [h1, decide_false, Bool.false_and, Bool.not_false]
    simp only [hj, decide_true, Bool.true_and, Bool.or_false]
    have : 8 * (32 - n) + (j - 8) = 8 * (32 - (n + 1)) + j := by omega
    rw [this]

/-- A word's first `n` bytes, big-endian, as a load folds them: its top `n` bytes. -/
private theorem fold_bytes (v : BitVec 256) (n : Nat) (h : n ≤ 32) :
    ((List.range n).map (SolarTV.Memory.byte v)).foldl
      (fun (acc : BitVec 256) (b : SolarTV.Memory.Byte) => (acc <<< 8) ||| b.setWidth 256)
      (0 : BitVec 256) = v >>> (8 * (32 - n)) := by
  induction n with
  | zero =>
    simp only [List.range_zero, List.map_nil, List.foldl_nil]
    apply BitVec.eq_of_getLsbD_eq
    intro j hj
    simp only [BitVec.getLsbD_ushiftRight, BitVec.getLsbD_zero]
    rw [BitVec.getLsbD_of_ge v _ (by omega)]
    simp
  | succ n ih =>
    rw [List.range_succ, List.map_append, List.foldl_append, ih (by omega)]
    simp only [List.map_cons, List.map_nil, List.foldl_cons, List.foldl_nil]
    exact step_bytes v n (by omega)

/-- A load of the word a store wrote reads it back. -/
theorem load_store_same (m : Mem) (offset : Nat) (value : Word) :
    (m.store offset value).load offset = value := by
  unfold store
  rw [load_expand]
  unfold load read
  have : (List.range 32).map (fun i =>
      (if offset ≤ offset + i ∧ offset + i < offset + 32 then
        SolarTV.Memory.byte value (offset + i - offset) else m.bytes (offset + i))) =
      (List.range 32).map (SolarTV.Memory.byte value) := by
    apply List.map_congr_left
    intro i mem
    simp only [List.mem_range] at mem
    have c : offset ≤ offset + i ∧ offset + i < offset + 32 := by omega
    simp only [c, and_self, ite_true, Nat.add_sub_cancel_left]
  simp only [this]
  rw [fold_bytes value 32 (by omega)]
  simp

theorem grow_comm (w a b : BitVec 251) :
    Memory.grow (Memory.grow w a) b = Memory.grow (Memory.grow w b) a := by
  unfold Memory.grow
  simp only [BitVec.ult_iff_toNat_lt]
  repeat' split
  all_goals first | rfl | (apply BitVec.eq_of_toNat_eq; omega) | omega

/-- A store as the bytes it writes and the words it grows memory to. -/
theorem store_eq (m : Mem) (a : Nat) (v : Word) :
    m.store a v = ⟨fun i => if a ≤ i ∧ i < a + 32 then Memory.byte v (i - a) else m.bytes i,
      Memory.grow m.words (BitVec.ofNat 251 ((a + 32 + 31) / 32))⟩ := rfl

/-- Expansions commute: each grows memory to the larger of its size and the current one. -/
theorem expand_comm (m : Mem) (o s o' s' : Nat) :
    (m.expand o s).expand o' s' = (m.expand o' s').expand o s := by
  unfold expand
  by_cases h : s = 0 <;> by_cases h' : s' = 0 <;> simp only [h, h', ite_true, ite_false]
  rw [grow_comm]

/-- A store after an expansion is the store, then the expansion. -/
theorem store_expand_comm (m : Mem) (o s a : Nat) (v : Word) :
    (m.expand o s).store a v = (m.store a v).expand o s := by
  unfold expand
  by_cases h : s = 0
  · simp only [h, ite_true]
  · simp only [h, ite_false, store_eq]
    rw [grow_comm]

/-- A store over a word a store wrote leaves only the later value. -/
theorem store_store_same (m : Mem) (a : Nat) (v w : Word) :
    (m.store a v).store a w = m.store a w := by
  simp only [store_eq, Memory.grow_idempotent, Mem.mk.injEq, and_true]
  funext i
  split <;> simp_all

/-- Stores to disjoint words commute. -/
theorem store_store_comm (m : Mem) (a b : Nat) (v w : Word) (h : a + 32 ≤ b) :
    (m.store b w).store a v = (m.store a v).store b w := by
  simp only [store_eq, Mem.mk.injEq]
  refine ⟨?_, grow_comm _ _ _⟩
  funext i
  by_cases h1 : a ≤ i ∧ i < a + 32 <;> by_cases h2 : b ≤ i ∧ i < b + 32 <;>
    simp only [h1, h2, ite_false] <;> omega

theorem project_initial : initial.project = Memory.initial [] 0 := by
  unfold project initial Memory.initial
  congr 1

theorem project_expand (m : Mem) (offset size : Nat) :
    (m.expand offset size).project = Memory.expand m.project offset size := by
  unfold expand Memory.expand project
  split <;> rfl

theorem project_msize (m : Mem) : m.msize = Memory.msize m.project := rfl

theorem project_read (m : Mem) (offset size : Nat) (bounded : offset + size ≤ 256) :
    m.read offset size = Memory.read m.project offset size := by
  unfold read Memory.read project
  apply List.map_congr_left
  intro i mem
  have : offset + i < 256 := by simp at mem; omega
  simp [this]

theorem project_load (m : Mem) (offset : Nat) (bounded : offset + 32 ≤ 256) :
    m.load offset = Memory.load m.project offset := by
  unfold load Memory.load
  rw [project_read m offset 32 bounded]

theorem project_store (m : Mem) (offset : Nat) (value : Word) :
    (m.store offset value).project = Memory.store m.project offset value := by
  unfold store Memory.store
  rw [project_expand]
  apply congrArg (Memory.expand · offset 32)
  simp only [project, Memory.writeWord]
  rw [Memory.State.mk.injEq]
  refine ⟨?_, rfl⟩
  apply List.ext_getElem (by simp only [List.length_map, List.length_mapIdx])
  intro i h _
  simp only [List.getElem_map, List.getElem_range, List.getElem_mapIdx]

theorem project_storeByte (m : Mem) (offset : Nat) (value : Word) :
    (m.storeByte offset value).project = Memory.storeByte m.project offset value := by
  unfold storeByte Memory.storeByte
  rw [project_expand]
  apply congrArg (Memory.expand · offset 1)
  simp only [project]
  rw [Memory.State.mk.injEq]
  refine ⟨?_, rfl⟩
  apply List.ext_getElem (by simp only [List.length_map, List.length_set])
  intro i h _
  simp only [List.getElem_map, List.getElem_range, List.getElem_set]
  by_cases e : i = offset
  · subst e; simp
  · have e' : ¬offset = i := fun h => e h.symm
    simp only [e, e', ↓reduceIte]

end Mem

/-- The external calls. -/
inductive CallKind where
  | call | callcode | delegatecall | staticcall
  deriving DecidableEq

/-- An event of the frame: a log, a call with its operands and the state it sees, or a read
of a word that changes as the frame runs, such as `GAS`. -/
inductive Event where
  | log (topics : List Word) (data : List Memory.Byte)
  | call (kind : CallKind) (operands : List Word) (input : List Memory.Byte)
      (storage transient : Word → Word)
  | read (name : String)
  /-- A contract creation: its value (and `CREATE2`'s salt), the creation code and the state the
  new contract may reenter. -/
  | create (operands : List Word) (code : List Memory.Byte) (storage transient : Word → Word)

structure State where
  memory : Mem
  storage : Word → Word
  transient : Word → Word
  world : List Event

/-- The state a call starts in: zeroed memory, the call's storage, no events. -/
def State.initial (call : Call) : State :=
  ⟨Mem.initial, fun slot => BitVec.ofNat 256 (slotValue call.storage slot.toNat), fun _ => 0, []⟩

/-- What the frame cannot see, as functions of the world (`Encoder`'s `env_`, `result`
and the state after a call event). The size of the return data is the environment read
`returndatasize` and its byte `i` the low byte of the read `returndata` of `i`. -/
structure Oracle where
  env : String → List Event → List Word → Word
  result : List Event → Word
  storageAfter : List Event → Word → Word
  transientAfter : List Event → Word → Word
  /-- The hash of a byte string. Uninterpreted, but a function, so hashing is deterministic. -/
  keccak : List Memory.Byte → Word
  /-- The address that made the call (`CALLER`): any address, so that what holds for every
  oracle holds for every caller. -/
  caller : BitVec 160

/-- The big-endian word of calldata at an offset, zero past its end. -/
def calldataWord (call : Call) (offset : Word) : Word :=
  (List.range 32).foldl
    (fun acc i => (acc <<< 8) ||| BitVec.ofNat 256 (call.calldata.data.getD (offset.toNat + i) 0).toNat) 0

/-- The calldata byte at an offset, zero past its end. -/
def calldataByte (call : Call) (offset : Nat) : Memory.Byte :=
  BitVec.ofNat 8 (call.calldata.data.getD offset 0).toNat

/-- `store m k v` as the certifier translates an SMT-LIB array store. -/
def update (m : Word → Word) (key value : Word) : Word → Word :=
  fun j => if j = key then value else m j

inductive Effect where
  | mload | mstore | mstore8 | msize | sload | sstore | tload | tstore
  | calldataload | calldatasize | callvalue | caller | address | returndatasize
  | log (topics : Nat)
  | keccak256
  | env (name : String)
  | call (kind : CallKind)
  | calldatacopy | mcopy | returndatacopy
  | gas
  /-- The word the deployment code writes for immutable `id` (`loadimmutable`, and the
  `PUSH32` placeholder it fills). -/
  | immutable (id : Nat)
  | codecopy | create | create2

/-- The size of the return data the world's last call left. -/
abbrev returndataSize (o : Oracle) (world : List Event) : Word := o.env "returndatasize" world []

/-- Byte `i` of the return data the world's last call left. -/
def returndataByte (o : Oracle) (world : List Event) (i : Nat) : Memory.Byte :=
  (o.env "returndata" world [BitVec.ofNat 256 i]).setWidth 8

/-- A call's operands in pop order: the words its callee sees (gas, address and, for `CALL`
and `CALLCODE`, value), and its input and output offsets and sizes. -/
def CallKind.shape : CallKind → List Word → Option (List Word × Word × Word × Word × Word)
  | .call, [gas, address, value, inOffset, inSize, outOffset, outSize]
  | .callcode, [gas, address, value, inOffset, inSize, outOffset, outSize] =>
    some ([gas, address, value], inOffset, inSize, outOffset, outSize)
  | .delegatecall, [gas, address, inOffset, inSize, outOffset, outSize]
  | .staticcall, [gas, address, inOffset, inSize, outOffset, outSize] =>
    some ([gas, address], inOffset, inSize, outOffset, outSize)
  | _, _ => none

/-- The state after an external call whose world, with the call, is `world`. -/
def called (o : Oracle) (kind : CallKind) (s : State) (memory : Mem) (outOffset outSize : Word)
    (world : List Event) : State :=
  let copied := min outSize.toNat (returndataSize o world).toNat
  let memory := memory.write outOffset.toNat copied (returndataByte o world)
  match kind with
  | .staticcall => { s with memory, world }
  | _ => ⟨memory, o.storageAfter world, o.transientAfter world, world⟩

/-- Byte `i` of the frame's code, the oracle's environment read `code` of `i` in the empty world. -/
def codeByte (o : Oracle) (i : Nat) : Memory.Byte :=
  (o.env "code" [] [BitVec.ofNat 256 i]).setWidth 8

/-- A contract creation with the operands its new contract sees (value, and `CREATE2`'s salt)
and its creation code at `offset`: the new address (the low 160 bits of the oracle's result,
zero for a failed creation), and the storage after it, as after a call; memory only expands. -/
def created (o : Oracle) (s : State) (seen : List Word) (offset size : Word) :
    Option Word × State :=
  let memory := s.memory.expand offset.toNat size.toNat
  let world := s.world ++ [.create seen (memory.read offset.toNat size.toNat) s.storage s.transient]
  (some (BitVec.ofNat 256 ((o.result world).toNat % 2 ^ 160)),
    ⟨memory, o.storageAfter world, o.transientAfter world, world⟩)

/-- A call's success flag. -/
def callFlag (o : Oracle) (world : List Event) : Word := if o.result world = 0 then 0 else 1

/-- An effect on its operands, in pop order: its result, if any, and the state after it;
`none` for operands of the wrong arity. -/
def Effect.apply (o : Oracle) (call : Call) : Effect → List Word → State → Option (Option Word × State)
  | .mload, [offset], s =>
    some (some (s.memory.load offset.toNat), { s with memory := s.memory.expand offset.toNat 32 })
  | .mstore, [offset, value], s => some (none, { s with memory := s.memory.store offset.toNat value })
  | .mstore8, [offset, value], s =>
    some (none, { s with memory := s.memory.storeByte offset.toNat value })
  | .msize, [], s => some (some s.memory.msize, s)
  | .sload, [key], s => some (some (s.storage key), s)
  | .sstore, [key, value], s => some (none, { s with storage := update s.storage key value })
  | .tload, [key], s => some (some (s.transient key), s)
  | .tstore, [key, value], s => some (none, { s with transient := update s.transient key value })
  | .calldataload, [offset], s => some (some (calldataWord call offset), s)
  | .calldatasize, [], s => some (some (BitVec.ofNat 256 call.calldata.size), s)
  | .callvalue, [], s => some (some (BitVec.ofNat 256 call.callvalue), s)
  | .caller, [], s => some (some (o.caller.setWidth 256), s)
  | .address, [], s => some (some (BitVec.ofNat 256 environment.callee), s)
  | .returndatasize, [], s => some (some (o.env "returndatasize" s.world []), s)
  | .log n, offset :: size :: topics, s =>
    if topics.length = n then
      let memory := s.memory.expand offset.toNat size.toNat
      some (none, { s with
        memory, world := s.world ++ [.log topics (memory.read offset.toNat size.toNat)] })
    else none
  | .keccak256, [offset, size], s =>
    let memory := s.memory.expand offset.toNat size.toNat
    some (some (o.keccak (memory.read offset.toNat size.toNat)), { s with memory })
  | .env name, operands, s => some (some (o.env name s.world operands), s)
  | .calldatacopy, [dest, offset, size], s =>
    let memory := s.memory.write dest.toNat size.toNat (fun i => calldataByte call (offset.toNat + i))
    some (none, { s with memory })
  | .mcopy, [dest, source, size], s =>
    let read := s.memory.expand source.toNat size.toNat
    let memory := read.write dest.toNat size.toNat (fun i => read.bytes (source.toNat + i))
    some (none, { s with memory })
  | .immutable id, [], s => some (some (o.env "immutable" [] [BitVec.ofNat 256 id]), s)
  | .codecopy, [dest, offset, size], s =>
    let memory := s.memory.write dest.toNat size.toNat (fun i => codeByte o (offset.toNat + i))
    some (none, { s with memory })
  | .create, [value, offset, size], s => some (created o s [value] offset size)
  | .create2, [value, offset, size, salt], s => some (created o s [value, salt] offset size)
  | .gas, [], s =>
    let world := s.world ++ [.read "gas"]
    some (some (o.env "gas" world []), { s with world })
  | .returndatacopy, [dest, offset, size], s =>
    if (returndataSize o s.world).toNat < offset.toNat + size.toNat then none
    else
      let memory := s.memory.write dest.toNat size.toNat
        (fun i => returndataByte o s.world (offset.toNat + i))
      some (none, { s with memory })
  | .call kind, operands, s =>
    match kind.shape operands with
    | none => none
    | some (seen, inOffset, inSize, outOffset, outSize) =>
      let memory := (s.memory.expand inOffset.toNat inSize.toNat).expand outOffset.toNat
        outSize.toNat
      let world := s.world ++
        [.call kind seen (memory.read inOffset.toNat inSize.toNat) s.storage s.transient]
      some (some (callFlag o world), called o kind s memory outOffset outSize world)
  | _, _, _ => none

/-- How a frame ends. `invalid` also stands for every exceptional halt. -/
inductive Halt where
  | stop
  | ret (data : List Memory.Byte)
  | revert (data : List Memory.Byte)
  | invalid

def logs (s : State) : List (List Nat × List Nat) :=
  s.world.filterMap fun
    | .log topics data => some (topics.map BitVec.toNat, data.map BitVec.toNat)
    | .call .. | .read _ | .create .. => none

def storageOf (s : State) : Nat → Nat := fun slot => (s.storage (BitVec.ofNat 256 slot)).toNat

def revertData : Halt → List Nat
  | .revert data => data.map BitVec.toNat
  | _ => []

/-- A successful end commits its storage and logs; a failed one rolls them back. -/
def observe (call : Call) (s : State) : Halt → Observable
  | .stop => ⟨true, [], logs s, storageOf s⟩
  | .ret data => ⟨true, data.map BitVec.toNat, logs s, storageOf s⟩
  | .revert data => ⟨false, data.map BitVec.toNat, [], slotValue call.storage⟩
  | .invalid => ⟨false, [], [], slotValue call.storage⟩

theorem observe_failure (call : Call) (s : State) (halt : Halt)
    (failed : (observe call s halt).success = false) :
    (observe call s halt).logs = [] ∧ (observe call s halt).storage = slotValue call.storage ∧
      (observe call s halt).output = revertData halt := by
  cases halt <;> simp_all [observe, revertData]

/-- Two ends no observer tells apart, as `Encoder::differ` compares them: the kind, the
returned bytes (`stop` returns none), and on success storage, transient storage and the
world; a revert compares its data alone. -/
def Agree : Halt × State → Halt × State → Prop
  | (.revert d, _), (.revert e, _) => d = e
  | (.invalid, _), (.invalid, _) => True
  | (a, s), (b, t) =>
    (match a, b with
      | .stop, .stop => True
      | .stop, .ret e => e = []
      | .ret d, .stop => d = []
      | .ret d, .ret e => d = e
      | _, _ => False) ∧
    s.storage = t.storage ∧ s.transient = t.transient ∧ s.world = t.world

theorem Agree.refl (r : Halt × State) : Agree r r := by
  obtain ⟨h, s⟩ := r
  cases h <;> simp [Agree]

theorem Agree.trans {a b c : Halt × State} (ab : Agree a b) (bc : Agree b c) : Agree a c := by
  obtain ⟨ha, sa⟩ := a
  obtain ⟨hb, sb⟩ := b
  obtain ⟨hc, sc⟩ := c
  cases ha <;> cases hb <;> cases hc <;> simp_all [Agree]

theorem storageOf_congr {s t : State} (h : s.storage = t.storage) : storageOf s = storageOf t := by
  unfold storageOf
  rw [h]

theorem logs_congr {s t : State} (h : s.world = t.world) : logs s = logs t := by
  unfold logs
  rw [h]

theorem observe_of_agree (call : Call) {a b : Halt × State} (agree : Agree a b) :
    observe call a.2 a.1 = observe call b.2 b.1 := by
  obtain ⟨ha, sa⟩ := a
  obtain ⟨hb, sb⟩ := b
  cases ha <;> cases hb <;> simp only [Agree] at agree <;> simp only [observe]
  all_goals first
    | exact agree.1.elim
    | (obtain ⟨kind, storage, _, world⟩ := agree
       rw [storageOf_congr storage, logs_congr world]
       try first | rfl | (subst kind; rfl) | (rw [kind]; rfl))
    | (subst agree; rfl)
    | rfl
    | exact agree.elim

/-- An iteration that either ends or continues from a new configuration, with fuel. -/
def iterate {σ : Type} (step : σ → Option ((Halt × State) ⊕ σ)) : Nat → σ → Option (Halt × State)
  | 0, _ => none
  | n + 1, x =>
    match step x with
    | some (.inl result) => some result
    | some (.inr next) => iterate step n next
    | none => none

theorem iterate_mono {σ : Type} (step : σ → Option ((Halt × State) ⊕ σ)) (n : Nat) (x : σ)
    (result : Halt × State) (ran : iterate step n x = some result) :
    iterate step (n + 1) x = some result := by
  induction n generalizing x with
  | zero => simp [iterate] at ran
  | succ n ih =>
    unfold iterate at ran ⊢
    cases hs : step x with
    | none => simp [hs] at ran
    | some v =>
      cases v with
      | inl r => simp_all
      | inr y =>
        simp only [hs] at ran ⊢
        exact ih y ran

/-- A language whose programs run on a call with fuel, deterministically: more fuel never
changes an end. -/
structure Layer where
  Prog : Type
  run : Oracle → Prog → Call → Nat → Option (Halt × State)
  mono : ∀ o p call n result, run o p call n = some result → run o p call (n + 1) = some result

namespace Layer

variable (L : Layer)

/-- Some fuel reaches this end. -/
def Ends (o : Oracle) (p : L.Prog) (call : Call) (result : Halt × State) : Prop :=
  ∃ n, L.run o p call n = some result

theorem mono_le {o : Oracle} {p : L.Prog} {call : Call} {result : Halt × State} {n m : Nat}
    (le : n ≤ m) (ran : L.run o p call n = some result) : L.run o p call m = some result := by
  induction le with
  | refl => exact ran
  | step _ ih => exact L.mono _ _ _ _ _ ih

theorem ends_unique {o : Oracle} {p : L.Prog} {call : Call} {a b : Halt × State}
    (ea : L.Ends o p call a) (eb : L.Ends o p call b) : a = b := by
  obtain ⟨n, hn⟩ := ea
  obtain ⟨m, hm⟩ := eb
  have := L.mono_le (Nat.le_max_left n m) hn
  rw [L.mono_le (Nat.le_max_right n m) hm] at this
  exact (Option.some.inj this).symm

open Classical in
/-- The end of a call, which some fuel reaches, or `none` when no fuel does. -/
noncomputable def result (o : Oracle) (p : L.Prog) (call : Call) : Option (Halt × State) :=
  if h : ∃ r, L.Ends o p call r then some h.choose else none

theorem result_of_ends {o : Oracle} {p : L.Prog} {call : Call} {r : Halt × State}
    (ends : L.Ends o p call r) : L.result o p call = some r := by
  have h : ∃ r, L.Ends o p call r := ⟨r, ends⟩
  simp only [result, h, dite_true]
  exact congrArg some (L.ends_unique h.choose_spec ends)

theorem ends_of_result {o : Oracle} {p : L.Prog} {call : Call} {r : Halt × State}
    (res : L.result o p call = some r) : L.Ends o p call r := by
  unfold result at res
  split at res
  · rename_i h
    cases res
    exact h.choose_spec
  · cases res

/-- The fuel-free semantics of a layer under an oracle. -/
noncomputable def sem (o : Oracle) : Sem :=
  ⟨L.Prog, fun p call => (L.result o p call).map fun r => observe call r.2 r.1⟩

theorem sem_failure (o : Oracle) (p : L.Prog) (call : Call) (obs : Observable)
    (ran : (L.sem o).run p call = some obs) (failed : obs.success = false) :
    ∃ r, L.Ends o p call r ∧ obs.logs = [] ∧ obs.storage = slotValue call.storage ∧
      obs.output = revertData r.1 := by
  simp only [sem, Option.map_eq_some_iff] at ran
  obtain ⟨r, res, rfl⟩ := ran
  exact ⟨r, L.ends_of_result res, observe_failure call r.2 r.1 failed⟩

theorem failureConvention (o : Oracle) : (L.sem o).FailureConvention := by
  intro p call obs ran failed
  obtain ⟨_, _, logs, storage, _⟩ := L.sem_failure o p call obs ran failed
  exact ⟨logs, storage⟩

end Layer

/-- Every end `p` reaches on a call in scope, `q` reaches an end that agrees with. This is
what an `unsat` pass validation query states, and it gives a stage. -/
def Certified (A B : Layer) (o : Oracle) (p : A.Prog) (q : B.Prog) (scope : Call → Prop) : Prop :=
  ∀ call, scope call → ∀ r, A.Ends o p call r → ∃ r', B.Ends o q call r' ∧ Agree r r'

theorem stage_of_certified {A B : Layer} {o : Oracle} {p : A.Prog} {q : B.Prog}
    {scope : Call → Prop} (certified : Certified A B o p q scope) :
    Stage (A.sem o) (B.sem o) p q scope := by
  intro call inScope obs ran
  simp only [Layer.sem, Option.map_eq_some_iff] at ran ⊢
  obtain ⟨r, res, rfl⟩ := ran
  obtain ⟨r', ends, agree⟩ := certified call inScope r (A.ends_of_result res)
  exact ⟨r', B.result_of_ends ends, (observe_of_agree call agree).symm⟩

theorem Certified.refl (A : Layer) (o : Oracle) (p : A.Prog) (scope : Call → Prop) :
    Certified A A o p p scope :=
  fun _ _ r ends => ⟨r, ends, Agree.refl r⟩

theorem Certified.trans {A B C : Layer} {o : Oracle} {p : A.Prog} {q : B.Prog} {r : C.Prog}
    {scope : Call → Prop} (first : Certified A B o p q scope)
    (second : Certified B C o q r scope) : Certified A C o p r scope := by
  intro call inScope x ends
  obtain ⟨y, ey, xy⟩ := first call inScope x ends
  obtain ⟨z, ez, yz⟩ := second call inScope y ey
  exact ⟨z, ez, xy.trans yz⟩

/-- A certificate from runs at given fuel: for every fuel at which `p` ends, `q` ends at
some fuel in an agreeing end. -/
theorem Certified.of_fuel {A B : Layer} {o : Oracle} {p : A.Prog} {q : B.Prog}
    {scope : Call → Prop}
    (runs : ∀ call, scope call → ∀ n r, A.run o p call n = some r →
      ∃ m r', B.run o q call m = some r' ∧ Agree r r') :
    Certified A B o p q scope := by
  intro call inScope r ⟨n, hn⟩
  obtain ⟨m, r', hm, agree⟩ := runs call inScope n r hn
  exact ⟨r', ⟨m, hm⟩, agree⟩

end SolarTV.Machine
