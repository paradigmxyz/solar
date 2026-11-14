import SolarTV.MIR.State.AbiEncode

/-! Packed arrays use padded ABI leaves, explicit memory strides and nested
object pointers. Contiguous plain words copy as a snapshot. Cleanup and nested
memory rows retain ordered loads/stores, including aliases. Calldata arrays use
wordwise reads on wrapping ranges unless the MIR marks an unguarded word copy.
Neither compiler expansion nor a backend image supplies these semantics. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

def packedElementWidth : AbiEncodeShape → Except String Nat
  | .word _ | .function => pure 32
  | .array element (some length) _ _ => do
    let width ← packedElementWidth element
    if length*width ≥ 2^64 then .error "packed array width" else pure (length*width)
  | _ => .error "packed array element"

def packedPlain : AbiEncodeShape → Bool
  | .word .any => true
  | .array element (some _) _ _ => packedPlain element
  | _ => false

structure PackedArray where
  element : AbiEncodeShape
  data : Word
  length : Nat
  bytes : Nat
  location : Nat
  stride : Nat
  object : Bool
  wordwise : Bool

def packedArrayWidth (element : AbiEncodeShape) : AbiM Nat :=
  match packedElementWidth element with
  | .ok width => pure width
  | .error reason => throw (.unsupported reason)

def preparePackedArray (cfg : Config) (input : Value) (element : AbiEncodeShape)
    (object : Bool) (location : Nat) (length : Option Nat) (stride : Nat) (wordwise : Bool) : AbiM PackedArray := do
  let width ← packedArrayWidth element
  let (data,count) ← if object then do
    let pointer ← abiWord input
    match length with
    | some count => pure (pointer,count)
    | none =>
      let count ← abiInputWord cfg false pointer
      pure (pointer+32,count.toNat)
    else do
      let (data,count) ← abiSlice input
      pure (data,count.toNat)
  let bytes := count*width
  if bytes ≥ 2^256 then throw (.reverted (.panic 0x41))
  if count > 2048 || bytes > 65536 then throw (.unsupported "packed array work limit")
  return ⟨element,data,count,bytes,location,(if object then stride else width/32),object,wordwise⟩

def writePackedArray : Nat → Config → PackedArray → Word → Bool → AbiM Unit
  | 0,_,_,_,_ => throw (.unsupported "packed array work limit")
  | fuel+1,cfg,array,destination,blocks => do
    abiSpend array.length
    let contiguous := if array.object then
        (match array.element with | .word .any => true | _ => false) && array.stride == 1
      else packedPlain array.element
    let unguarded := !array.wordwise && (match array.element with | .word _ => true | _ => false)
    let wraps := array.data.toNat+array.bytes ≥ 2^256
    if blocks && contiguous && (array.location != 1 || unguarded || !wraps) then
      abiCopyFrom cfg array.location destination array.data (BitVec.ofNat 256 array.bytes)
    else
      let width ← packedArrayWidth array.element
      for index in List.range array.length do
        let source := array.data+BitVec.ofNat 256 (index*array.stride*32)
        let target := destination+BitVec.ofNat 256 (index*width)
        match array.element with
        | .word cleanup =>
          let word ← abiInputWord cfg (array.location == 1) source
          let word ← abiClean word cleanup
          abiStore target word
        | .function =>
          let word ← abiInputWord cfg (array.location == 1) source
          abiStore target (if array.object then word &&& ~~~(BitVec.ofNat 256 (2^64-1)) else word)
        | .array element (some count) _ _ =>
          let nestedWidth ← packedArrayWidth element
          let data ← if array.object then abiInputWord cfg false source else pure source
          let nested : PackedArray := ⟨element,data,count,count*nestedWidth,array.location,
            (if array.object then 1 else nestedWidth/32),array.object,array.wordwise⟩
          writePackedArray fuel cfg nested target array.object
        | _ => throw (.unsupported "packed array element")

end SolarTV.MIR.State
