import SolarTV.MIR.Effects

/-! ABI encoding as ordered requests on MIR memory and allocation.
Word leaves, memory tuple fields, and a calldata byte tail admit distinct
decoders for their operands. The adapter uses the same stores and reads
as direct MIR, including any stores that overlap a preceding selector
or clear dirty padding. -/

namespace SolarTV.MIR.LowerAbiEncode
open EvmRules Effects

def load (pointer : Word) (next : Word → Tree) : Tree :=
  .request ⟨.opcode 0x51, [pointer], true⟩ next

def store (pointer value : Word) (next : Tree) : Tree :=
  .request ⟨.opcode 0x52, [pointer, value], false⟩ (fun _ => next)

def copyCalldata (destination source size : Word) (next : Tree) : Tree :=
  .request ⟨.opcode 0x37, [destination, source, size], false⟩ (fun _ => next)

def reserve (size : Word) (next : Word → Tree) : Tree :=
  .request ⟨.allocate false false, [size], true⟩ next

inductive Cleanup where
  | word
  | uint (bits : Nat)
  | boolean

def Cleanup.apply (rule : Cleanup) (value : Word) : Word :=
  match rule with
  | .word => value
  | .uint bits => value &&& BitVec.ofNat 256 (2^bits - 1)
  | .boolean => if value = 0 then 0 else 1

inductive Field where
  | word (value : Word)
  | memory (pointer : Word) (cleanup : Cleanup)

def writeFields (head : Word) (fields : List Field) (next : Tree) : Tree :=
  match fields with
  | [] => next
  | .word value :: rest => store head value (writeFields (head + 32) rest next)
  | .memory pointer rule :: rest =>
    load pointer (fun value => store head (rule.apply value) (writeFields (head + 32) rest next))

def selectorSize : Option Word → Nat
  | none => 0
  | some _ => 4

def writeStatic (pointer : Word) (selector : Option Word) (fields : List Field)
    (next : Tree) : Tree :=
  match selector with
  | none => writeFields pointer fields next
  | some selector => store pointer selector (writeFields (pointer + 4) fields next)

def scratch (base selector : Option Word) (fields : List Field)
    (next : Word → Word → Tree) : Tree :=
  let size := BitVec.ofNat 256 (selectorSize selector + 32 * fields.length)
  let encode := fun pointer => writeStatic pointer selector fields (next pointer size)
  match base with
  | none => load 64 encode
  | some pointer => encode pointer

def object (selector : Option Word) (fields : List Field) (next : Word → Tree) : Tree :=
  let size := selectorSize selector + 32 * fields.length
  reserve (BitVec.ofNat 256 (((size + 31) / 32 * 32) + 32)) (fun pointer =>
    store pointer (BitVec.ofNat 256 size) (writeStatic (pointer + 32) selector fields (next pointer)))

def bytesScratch (selector value source length : Word) (next : Word → Word → Tree) : Tree :=
  load 64 fun pointer =>
    let head := pointer + 4
    let tail := head + 64
    let padding := (length + 31) &&& ~~~(31 : Word)
    store pointer selector <|
      store head value <|
        store (head + 32) 64 <|
          store (tail + padding) 0 <|
            store tail length <|
              copyCalldata (tail + 32) source length (next pointer (100 + padding))

theorem sliceStart (pointer length : Word) :
    (pointer ++ length).extractLsb' 256 256 = pointer :=
  BitVec.extractLsb'_append_eq_left

theorem sliceSize (pointer length : Word) :
    (pointer ++ length).extractLsb' 0 256 = length :=
  BitVec.extractLsb'_append_eq_right

theorem cancelBase (pointer first second : Word) :
    (pointer + first) - (pointer + second) = first - second := by
  bv_omega


def reuseReturn (byteObject : Bool) (object : Word) : Tree :=
  load object fun length =>
    let start := object - 32
    let size := if byteObject then length else length <<< 5
    let extent := if byteObject then (length + 31) &&& ~~~(31 : Word) else size
    let finish := store start 32 (.done ⟨"returndata", [(256,start), (256,extent+64)]⟩)
    if byteObject then store (object + 32 + size) 0 finish else finish

theorem preceding (address : Word) :
    address + 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0#256 = address - 32 := by
  bv_omega

end SolarTV.MIR.LowerAbiEncode
