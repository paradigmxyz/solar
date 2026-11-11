import SolarTV.Outcome
import EvmRules.Word
import SolarTV.Keccak

/-! Byte-addressed memory shared by the MIR and EVM IR importers.
The current projection retains the first 256 bytes and the complete active-word
count. Importers admit only constant accesses inside that prefix. Incoming bytes
are arbitrary in active words and zero outside them; bytes beyond the projection
are unchanged by every admitted operation. This is a projected-state model, not
a bounded-address assumption on a symbolic pointer.

Expansion follows EVMYulLean 047f630, MachineStateOps.lean:24-33: zero-length
accesses do not expand, and offset + length is natural arithmetic, not wrapping
word arithmetic. Byte order and the low byte of MSTORE8 follow lines 35-68.
Gas remains outside this fragment. -/

namespace SolarTV.Memory
open EvmRules

abbrev Byte := BitVec 8

structure State where
  bytes : List Byte
  words : BitVec 251
  deriving DecidableEq

/-- Big-endian byte i of a word. -/
def byte (value : Word) (i : Nat) : Byte :=
  (value >>> (8 * (31 - i))).setWidth 8

/-- The count covers all representable, word-aligned MSIZE values.
The importer limits accesses to eight words, so expansion cannot overflow it. -/
def initial (values : List Word) (words : BitVec 251) : State :=
  ⟨(List.range 256).map (fun i =>
    if (BitVec.ofNat 251 (i / 32)).ult words then byte (values[i / 32]?.getD 0) (i % 32)
    else 0), words⟩

def grow (words required : BitVec 251) : BitVec 251 :=
  if words.ult required then required else words

@[simp] theorem grow_idempotent (words required : BitVec 251) :
    grow (grow words required) required = grow words required := by
  unfold grow
  split <;> simp_all [BitVec.ult_iff_toNat_lt]

def expand (memory : State) (offset size : Nat) : State :=
  if size = 0 then memory
  else
    let required := BitVec.ofNat 251 ((offset + size + 31) / 32)
    { memory with words := grow memory.words required }

def msize (memory : State) : Word := memory.words.setWidth 256 <<< 5

def read (memory : State) (offset size : Nat) : List Byte :=
  (List.range size).map (fun i => memory.bytes[offset + i]?.getD 0)

def load (memory : State) (offset : Nat) : Word :=
  (read memory offset 32).foldl (fun acc b => (acc <<< 8) ||| b.setWidth 256) 0

def writeWord (bytes : List Byte) (offset : Nat) (value : Word) : List Byte :=
  bytes.mapIdx (fun i old => if offset ≤ i ∧ i < offset + 32 then byte value (i - offset) else old)

@[simp] theorem writeWord_overwrite (bytes : List Byte) (offset : Nat) (a b : Word) :
    writeWord (writeWord bytes offset a) offset b = writeWord bytes offset b := by
  simp only [writeWord, List.mapIdx_mapIdx]
  congr 1
  funext i old
  split <;> simp_all

def store (memory : State) (offset : Nat) (value : Word) : State :=
  let bytes := writeWord memory.bytes offset value
  expand { memory with bytes } offset 32

def storeByte (memory : State) (offset : Nat) (value : Word) : State :=
  expand { memory with bytes := memory.bytes.set offset (value.setWidth 8) } offset 1

/-- Snapshot the source before writing, including overlapping MCOPY ranges. -/
def copy (memory : State) (destination source count : Nat) : State :=
  if count = 0 then memory else
  let bytes := memory.bytes.mapIdx (fun i old =>
    if destination ≤ i ∧ i < destination + count then
      memory.bytes[source + i - destination]?.getD 0 else old)
  expand (expand {memory with bytes} source count) destination count

def hash (memory : State) (offset count : Nat) : Word :=
  Keccak.hash (read memory offset count)

/-- Only witness discovery uses this projection. Concrete witnesses are replayed
against equality of the actual modeled executions in Lean's kernel. -/
def snapshot (memory : State) : List Word :=
  msize memory :: memory.bytes.map (fun b => b.setWidth 256)

def describe (memory : State) : String :=
  let values := (List.range 8).map (fun i => (load memory (i * 32)).toNat)
  s!"msize={msize memory |>.toNat}, memory={values}"

structure Returned (width : Nat) where
  value : BitVec width
  memory : State
  deriving DecidableEq

def code (result : Outcome (Returned width)) : List Word :=
  match result with
  | .returned result => 0 :: result.value.setWidth 256 :: snapshot result.memory
  | .reverted .empty => [1]
  | .reverted (.panic value) => [2, value]
  | .invalid => [3]

def describeReturn (result : Outcome (Returned width)) : String :=
  match result with
  | .returned result => s!"return={result.value.toNat}, {describe result.memory}"
  | .reverted .empty => "revert(empty)"
  | .reverted (.panic value) => s!"revert Panic({value.toNat})"
  | .invalid => "invalid"

end SolarTV.Memory
