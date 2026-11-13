import SolarTV.MIR.State.Abi

/-! Recursive ABI decoding with logical tuple results and physical memory objects.
Each descent validates the containing head and relative offset. Bytes and dynamic
arrays reserve checked objects before checking/copying their payloads; fixed
aggregates reserve their word slots before their children. Calldata tuples retain
the source-base trailer used by subsequent calldata access. Memory copies have
snapshot semantics, including overlapping input/output. Limits are model errors,
not shortened arrays or successful decodes. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

/- The shape and its nested field lists have a shared structural recursor. This
avoids repeatedly reducing well-founded recursion proofs during kernel replay. -/
mutual
  def AbiShape.layout : AbiShape → Bool × Nat
    | .scalar .. => (false,32)
    | .bytes | .array _ none => (true,32)
    | .array element (some length) =>
      let child := element.layout
      if child.1 then (true,32) else (false,length*child.2)
    | .tuple fields =>
      let children := abiFieldsLayout fields
      if children.1 then (true,32) else children
  termination_by structural shape => shape

  def abiFieldsLayout : List AbiShape → Bool × Nat
    | [] => (false,0)
    | field::rest =>
      let child := field.layout
      let children := abiFieldsLayout rest
      (child.1 || children.1,child.2+children.2)
  termination_by structural fields => fields
end

def AbiShape.dynamic (shape : AbiShape) : Bool := shape.layout.1

def AbiShape.head (shape : AbiShape) : Nat := shape.layout.2

def abiFieldsDynamic (fields : List AbiShape) : Bool := fields.any (·.dynamic)

def abiFieldsHead (fields : List AbiShape) : Nat := (fields.map (·.head)).sum

def AbiShape.function : AbiShape → Bool
  | .scalar _ function _ => function
  | _ => false

def AbiShape.width : AbiShape → Nat
  | .scalar _ _ width => width
  | _ => 256

def AbiShape.isScalar : AbiShape → Bool
  | .scalar .. => true
  | _ => false

def AbiShape.unchecked : AbiShape → Bool
  | .scalar .any _ _ => true
  | _ => false

inductive AbiFault where
  | unsupported (reason : String)
  | reverted (data : RevertData)
  | invalid

structure AbiState where
  frame : Frame
  work : Nat := 2048

abbrev AbiM := StateT AbiState (Except AbiFault)

def abiRequire (invalid : Bool) : AbiM Unit :=
  if invalid then throw (.reverted .empty) else pure ()

def abiMemory (action : List Byte → Except String (α × List Byte)) : AbiM α := do
  let state ← get
  match action state.frame.memory with
  | .error reason => throw (.unsupported reason)
  | .ok (value,memory) =>
    set {state with frame := {state.frame with memory}}
    return value

def abiInputWord (cfg : Config) (calldata : Bool) (address : Word) : AbiM Word :=
  if calldata then pure (readWord cfg.environment.input address.toNat) else
    abiMemory fun memory => do
      let memory ← expand memory address.toNat 32
      return (readWord memory address.toNat,memory)

def abiStore (address word : Word) : AbiM Unit :=
  abiMemory fun memory => do
    let memory ← expand memory address.toNat 32
    return ((),writeBytes memory address.toNat (wordBytes word))

def abiCopyFrom (cfg : Config) (location : Nat) (destination source count : Word) : AbiM Unit := do
  let state ← get
  if location > 2 then throw (.unsupported "ABI source address space")
  if location == 2 then
    if !cfg.available.contains 0x3e then throw .invalid
    if source.toNat+count.toNat > state.frame.returnData.length then throw .invalid
  abiMemory fun memory => do
    let memory ← expand memory destination.toNat count.toNat
    let memory ← if location == 0 then expand memory source.toNat count.toNat else pure memory
    let input := if location == 0 then memory else if location == 1 then cfg.environment.input else state.frame.returnData
    return ((),writeBytes memory destination.toNat (readBytes input source.toNat count.toNat))

def abiCopy (cfg : Config) (calldata : Bool) (destination source count : Word) : AbiM Unit :=
  abiCopyFrom cfg (if calldata then 1 else 0) destination source count

def abiZero (address : Word) (count : Nat) : AbiM Unit :=
  abiMemory fun memory => do
    let memory ← expand memory address.toNat count
    return ((),writeBytes memory address.toNat (List.replicate count 0))

def abiReserve (size : Word) (checked : Bool) : AbiM Word := do
  let state ← get
  match allocate state.frame size false checked with
  | .error reason => throw (.unsupported reason)
  | .ok (.returned (some pointer,frame)) => set {state with frame}; return pointer
  | .ok (.reverted data) => throw (.reverted data)
  | .ok .invalid => throw .invalid
  | _ => throw (.unsupported "ABI allocation result")

def abiSpend (count : Nat) : AbiM Unit := do
  let state ← get
  if count > state.work then throw (.unsupported "ABI decode work limit")
  set {state with work := state.work-count}

private def abiArrayObject (length : Word) : AbiM Word := do
  if length.toNat > (2^64-1)/32 then throw (.reverted (.panic 0x41))
  let object ← abiReserve ((length+1)*32) true
  abiStore object length
  return object

private def abiBytesObject (length : Word) : AbiM Word := do
  if length.toNat+63 ≥ 2^256 then throw (.reverted (.panic 0x41))
  let object ← abiReserve (BitVec.ofNat 256 ((length.toNat+63)/32*32)) true
  abiStore object length
  return object

def abiDecodeNode : Nat → Config → Bool → AbiShape → Word → Word → Word → Bool → AbiM Word
  | 0,_,_,_,_,_,_,_ => throw (.unsupported "ABI decode depth limit")
  | fuel+1,cfg,calldata,shape,head,tupleBase,inputEnd,staticTuple => do
    abiSpend 1
    match shape with
    | .scalar rule function _ =>
      let word ← abiInputWord cfg calldata head
      abiRequire (!abiCanonical rule word)
      return if function then word >>> 64 else word
    | _ => pure ()
    let dynamic := shape.dynamic
    let staticTuple := staticTuple || calldata && !dynamic
    let base ← if dynamic then do
        let offset ← abiInputWord cfg calldata head
        let target := tupleBase+offset
        match shape with
        | .bytes | .array _ none =>
          abiRequire (offset.toNat > 2^64-1 || (target+32).toNat > inputEnd.toNat)
        | _ => abiRequire (offset.toNat > (inputEnd-tupleBase).toNat)
        pure target
      else pure head
    match shape with
    | .scalar .. => throw (.unsupported "ABI scalar dispatch")
    | .bytes =>
      let length ← abiInputWord cfg calldata base
      let object ← abiBytesObject length
      let data := base+32
      abiRequire (length.toNat > (inputEnd-data).toNat)
      abiCopy cfg calldata (object+32) data length
      abiZero (object+32+length) ((-length).toNat % 32)
      return object
    | .array element length =>
      let (count,data,object) ← match length with
        | none => do
          let count ← abiInputWord cfg calldata base
          let object ← abiArrayObject count
          let data := base+32
          let maximum := if element.head = 0 then 0 else (inputEnd-data).toNat / element.head
          abiRequire (count.toNat > maximum)
          pure (count.toNat,data,object)
        | some count => do
          if !staticTuple && dynamic then
            abiRequire (count*element.head > (inputEnd-base).toNat)
          if count > (← get).work then throw (.unsupported "ABI decode work limit")
          let object ← abiReserve (BitVec.ofNat 256 (count*32)) false
          pure (count,base,object)
      if count > (← get).work then throw (.unsupported "ABI decode work limit")
      let destination := object + if length.isNone then 32 else 0
      let blockCopy := element.isScalar &&
        (if length.isNone then element.unchecked || calldata else staticTuple && !element.function)
      if blockCopy then
        if !element.unchecked then
          for index in List.range count do
            let _ ← abiDecodeNode fuel cfg calldata element (data+BitVec.ofNat 256 (index*32)) data inputEnd staticTuple
            pure ()
        else abiSpend count
        abiCopy cfg calldata destination data (BitVec.ofNat 256 (count*32))
      else
        for index in List.range count do
          let value ← abiDecodeNode fuel cfg calldata element
            (data+BitVec.ofNat 256 (index*element.head)) data inputEnd staticTuple
          abiStore (destination+BitVec.ofNat 256 (index*32)) (if element.function then value <<< 64 else value)
      return object
    | .tuple fields =>
      if !staticTuple && dynamic then abiRequire (abiFieldsHead fields > (inputEnd-base).toNat)
      let carriesBase := !staticTuple && calldata && abiFieldsDynamic fields
      let object ← abiReserve (BitVec.ofNat 256 ((fields.length + if carriesBase then 1 else 0)*32)) false
      let mut source := base
      for (field,index) in fields.zipIdx do
        let value ← abiDecodeNode fuel cfg calldata field source base inputEnd staticTuple
        abiStore (object+BitVec.ofNat 256 (index*32)) (if field.function then value <<< 64 else value)
        source := source + BitVec.ofNat 256 field.head
      if carriesBase then abiStore (object+BitVec.ofNat 256 (fields.length*32)) base
      return object

private def abiDecodeFields (cfg : Config) (calldata : Bool) (shapes : List AbiShape) (input : Value) : AbiM (List Value) := do
  let (base,length) ← match input with
    | .scalar pointer => do
      if calldata then throw (.unsupported "calldata decode needs a slice")
      let length ← abiInputWord cfg false pointer.word
      pure (pointer.word+32,length)
    | .aggregate [base,length] =>
      match base.word,length.word with
      | .ok base,.ok length => pure (base,length)
      | _,_ => throw (.unsupported "undefined ABI decode slice")
    | _ => throw (.unsupported "undefined ABI decode input")
  abiRequire (length.toNat ≥ 2^255 || length.toNat < abiFieldsHead shapes)
  let staticTuple := !abiFieldsDynamic shapes
  let mut head := base
  let mut values := []
  for shape in shapes do
    let word ← abiDecodeNode 33 cfg calldata shape head base (base+length) staticTuple
    values := values ++ [.scalar (Scalar.ofWord shape.width word)]
    head := head + BitVec.ofNat 256 shape.head
  return values

def abiDecodeTree (cfg : Config) (frame : Frame) (shapes : List AbiShape) (calldata : Bool) (input : Value) : Except String (Outcome (Option Value × Frame)) :=
  match (abiDecodeFields cfg calldata shapes input).run ⟨frame,2048⟩ with
  | .error (.unsupported reason) => .error reason
  | .error (.reverted data) => .ok (.reverted data)
  | .error .invalid => .ok .invalid
  | .ok (values,state) => .ok (.returned (match values with
      | [] => none | [value] => some value | values => some (.aggregate values),state.frame))

end SolarTV.MIR.State
