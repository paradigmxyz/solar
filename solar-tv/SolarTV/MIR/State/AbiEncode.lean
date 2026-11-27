import SolarTV.MIR.State.AbiDecode
import SolarTV.MIR.State.StorageBytes

/-! Direct recursive ABI encoding. Memory objects contain word slots, while
calldata aggregate heads contain relative offsets. The encoder validates nested
calldata accesses, cleans memory scalars and writes each offset before its tail.
Dynamic output is written at the supplied FMP and reserved afterwards; static
output reserves first. No encoder pass, temporary object recognition or bytecode
image supplies the semantics. Resource exhaustion is unsupported. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

mutual
  def AbiEncodeShape.layout : AbiEncodeShape → Bool × Nat
    | .word _ | .function => (false,32)
    | .bytes _ | .array _ none _ _ => (true,32)
    | .array element (some length) _ _ =>
      let child := element.layout
      if child.1 then (true,32) else (false,length*child.2)
    | .tuple fields =>
      let children := abiEncodeFieldsLayout fields
      if children.1 then (true,32) else children
  termination_by structural shape => shape

  def abiEncodeFieldsLayout : List AbiEncodeShape → Bool × Nat
    | [] => (false,0)
    | field::rest =>
      let child := field.layout
      let children := abiEncodeFieldsLayout rest
      (child.1 || children.1,child.2+children.2)
  termination_by structural fields => fields
end

def AbiEncodeShape.dynamic (shape : AbiEncodeShape) : Bool := shape.layout.1

def AbiEncodeShape.head (shape : AbiEncodeShape) : Nat := shape.layout.2

def AbiEncodeShape.tail : AbiEncodeShape → Nat
  | .array element (some length) _ _ => length*element.head
  | .tuple fields => (fields.map (·.head)).sum
  | _ => 32

inductive EncodeSource where
  | value (value : Value) (memory : Bool)
  | calldata (head base : Word) (validate : Bool)
  | memory (head : Word)

def abiWord (value : Value) : AbiM Word :=
  match value.word with
  | .ok word => pure word
  | .error reason => throw (.unsupported reason)

def abiSlice (value : Value) : AbiM (Word × Word) := do
  let .aggregate [base,length] := value | throw (.unsupported "ABI slice representation")
  return (← abiWord base,← abiWord length)

def abiScalar (word : Word) : Value := .scalar (Scalar.ofWord 256 word)

def abiClean (word : Word) : AbiRule → AbiM Word
  | .any => pure word
  | .uint bits => pure (narrow bits word)
  | .left bits => pure (word &&& BitVec.ofNat 256 (2^256-2^(256-bits)))
  | .sint bits => pure ((word.setWidth bits).signExtend 256)
  | .boolean => pure (if word = 0 then 0 else 1)
  | .enumeration bound =>
    if word.toNat < bound then pure word else throw (.reverted (.panic 0x21))

def abiEncodeRange (cfg : Config) (value : Value) (declared : Nat) (header : Bool) : AbiM (Nat × Word × Word) := do
  match value with
  | .scalar pointer =>
    let length ← if header then abiInputWord cfg false pointer.word else pure 0
    return (0,pointer.word + if header then 32 else 0,length)
  | _ =>
    let (base,length) ← abiSlice value
    if declared > 2 then throw (.unsupported "ABI slice address space")
    return (declared,base,length)

def abiEncodeCalldataTail (cfg : Config) (shape : AbiEncodeShape) (head base : Word) : AbiM Value := do
  let inputEnd : Word := BitVec.ofNat 256 cfg.environment.input.length
  let offset ← abiInputWord cfg true head
  if shape.tail = 0 then throw (.unsupported "ABI zero-sized calldata tail")
  let bound := inputEnd-base-BitVec.ofNat 256 (shape.tail-1)
  abiRequire (!(offset.slt bound))
  let position := base+offset
  let (data,length) ← match shape with
    | .array _ (some length) _ _ => pure (position,BitVec.ofNat 256 length)
    | .tuple _ => pure (position,BitVec.ofNat 256 shape.head)
    | _ => do
      let length ← abiInputWord cfg true position
      abiRequire (length.toNat > 2^64-1)
      let stride := match shape with | .array element .. => element.head | _ => 1
      let data := position+32
      abiRequire ((inputEnd-length*BitVec.ofNat 256 stride).slt data)
      pure (data,length)
  return .aggregate [abiScalar data,abiScalar length]

def abiEncodeStorageBytes (cfg : Config) (slot destination : Word) : AbiM Word := do
  let state ← get
  let (header,frame) := storageRead cfg state.frame slot
  set {state with frame}
  let (long,length) ← match bytesHeader header with
    | .error data => throw (.reverted data)
    | .ok result => pure result
  abiStore destination (BitVec.ofNat 256 length)
  let count := (length+31)/32
  abiSpend count
  if long then
    let state ← get
    match fixedSlot state.frame [slot] with
    | .error reason => throw (.unsupported reason)
    | .ok (base,frame) =>
      set {state with frame}
      for index in List.range count do
        let state ← get
        let (word,frame) := storageRead cfg state.frame (base+BitVec.ofNat 256 index)
        set {state with frame}
        abiStore (destination+BitVec.ofNat 256 (32+index*32)) word
  else abiStore (destination+32) (header &&& ~~~(255 : Word))
  return destination + BitVec.ofNat 256 (32+count*32)

def abiEncodeNode : Nat → Config → AbiEncodeShape → EncodeSource → Word → Word → Word → AbiM Word
  | 0,_,_,_,_,_,_ => throw (.unsupported "ABI encoding depth limit")
  | fuel+1,cfg,shape,initial,head,tupleBase,tail => do
    abiSpend 1
    match shape with
    | .word _ | .function =>
      let cleanup := match shape with | .function => .left 192 | .word rule => rule | _ => .any
      let value ← match initial with
        | .value value memory => do
          let word ← abiWord value
          if let .function := shape then pure (if memory then word &&& BitVec.ofNat 256 (2^256-2^64) else word <<< 64)
          else if memory then abiClean word cleanup else pure word
        | .memory address => do
          let word ← abiInputWord cfg false address
          abiClean word cleanup
        | .calldata head _ validate => do
          let word ← abiInputWord cfg true head
          if validate then abiRequire (!abiCanonical cleanup word)
          pure word
      abiStore head value
      return tail
    | _ => pure ()
    let originalTail := tail
    let fromCalldata := match initial with | .calldata .. => true | _ => false
    let mut source := initial
    if shape.dynamic then
      if let .calldata head base _ := source then
        let value ← abiEncodeCalldataTail cfg shape head base
        if let .array (.word cleanup) none _ _ := shape then
          if cleanup != .any then
            let (data,length) ← abiSlice value
            abiSpend length.toNat
            for index in List.range length.toNat do
              let word ← abiInputWord cfg true (data+BitVec.ofNat 256 (index*32))
              abiRequire (!abiCanonical cleanup word)
        source := .value value false
      abiStore head (tail-tupleBase)
    let destination := if shape.dynamic then tail else head
    match shape with
    | .word _ | .function => throw (.unsupported "ABI scalar dispatch")
    | .bytes location =>
      let .value value _ := source | throw (.unsupported "ABI byte source")
      if location = 3 then abiEncodeStorageBytes cfg (← abiWord value) destination else do
        let (location,data,length) ← abiEncodeRange cfg value (if fromCalldata then 1 else location) true
        let padded := (length+31) &&& ~~~(31 : Word)
        abiStore (destination+padded) 0
        abiStore destination length
        abiCopyFrom cfg location (destination+32) data length
        return destination+32+padded
    | .tuple fields =>
      let mut head := destination
      let mut tail := destination+BitVec.ofNat 256 ((fields.map (·.head)).sum)
      let mut sourceHead : Word := 0
      let mut sourceBase : Word := 0
      let mut calldata := false
      let mut encodedMemory := false
      let mut validate := false
      let mut pointer : Word := 0
      let mut values : List Word := []
      match source with
      | .memory input => sourceHead := input; encodedMemory := true
      | .calldata input base valid => sourceHead := input; sourceBase := base; calldata := true; validate := valid
      | .value (.aggregate [base,_]) _ => sourceHead ← abiWord base; sourceBase := sourceHead; calldata := true; validate := shape.dynamic
      | .value value _ =>
        pointer ← abiWord value
        if shape.dynamic then
          for index in List.range fields.length do
            values := values ++ [← abiInputWord cfg false (pointer+BitVec.ofNat 256 (index*32))]
      for (field,index) in fields.zipIdx do
        let next ← if encodedMemory then pure (.memory sourceHead) else if calldata then pure (.calldata sourceHead sourceBase validate) else do
          let word ← if shape.dynamic then match values[index]? with | some value => pure value | none => throw (.unsupported "ABI tuple field missing")
            else abiInputWord cfg false (pointer+BitVec.ofNat 256 (index*32))
          pure (.value (abiScalar word) true)
        tail ← abiEncodeNode fuel cfg field next head destination tail
        head := head+BitVec.ofNat 256 field.head
        sourceHead := sourceHead+BitVec.ofNat 256 field.head
      return if shape.dynamic then tail else originalTail
    | .array element length location blockCopy =>
      let (location,sourceHead,count,validate,encodedMemory) ← match source with
        | .calldata head _ validate =>
          let some count := length | throw (.unsupported "ABI array source")
          pure (1,head,count,validate,false)
        | .memory input =>
          let some count := length | throw (.unsupported "ABI array source")
          pure (0,input,count,false,true)
        | .value value _ =>
          match length with
          | none =>
            let (location,data,count) ← abiEncodeRange cfg value (if fromCalldata then 1 else location) true
            abiStore destination count
            pure (location,data,count.toNat,false,false)
          | some count =>
            match value with
            | .scalar pointer => pure (0,pointer.word,count,false,false)
            | _ => let (base,_) ← abiSlice value; pure (1,base,count,false,false)
      if count > (← get).work then throw (.unsupported "ABI encoding work limit")
      let calldata := location == 1
      let area := destination + if length.isNone then 32 else 0
      let bytes := BitVec.ofNat 256 (count*element.head)
      let mut tail := area+bytes
      let bulk :=
        if !shape.dynamic then calldata && blockCopy else
        match element with | .word .any => true | .word _ => location != 0 | .function => location == 2 | _ => false
      if bulk then
        abiSpend count
        abiCopyFrom cfg location area sourceHead bytes
      else
        if location == 2 then throw (.unsupported "ABI returndata aggregate layout")
        for index in List.range count do
          let child ← if encodedMemory then pure (.memory (sourceHead+BitVec.ofNat 256 (index*element.head))) else if calldata then pure (.calldata (sourceHead+BitVec.ofNat 256 (index*element.head)) sourceHead (validate || shape.dynamic)) else do
            let word ← abiInputWord cfg false (sourceHead+BitVec.ofNat 256 (index*32))
            pure (.value (abiScalar word) true)
          tail ← abiEncodeNode fuel cfg element child (area+BitVec.ofNat 256 (index*element.head)) area tail
      return if shape.dynamic then tail else originalTail

private def abiEncodeFields (cfg : Config) (mode : Nat) (hasBase hasSelector : Bool)
    (shapes : List AbiEncodeShape) (memorySlices : List Bool) (args : List Value) : AbiM Value := do
  let (base,args) ← if hasBase then do
      let value ← match args[0]? with | some value => pure value | none => throw (.unsupported "missing ABI base")
      pure (some (← abiWord value),args.drop 1)
    else pure (none,args)
  let (selector,args) ← if hasSelector then do
      let value ← match args[0]? with | some value => pure value | none => throw (.unsupported "missing ABI selector")
      pure (some (← abiWord value),args.drop 1)
    else pure (none,args)
  if args.length != shapes.length || memorySlices.length != shapes.length || mode > 2 then throw (.unsupported "ABI encode shape")
  let selectorBytes := if hasSelector then 4 else 0
  let headSize := (shapes.map (·.head)).sum
  let dynamic := shapes.any (·.dynamic)
  let header := mode == 1 || !dynamic && mode == 0 && !hasSelector
  let pointer ← if dynamic || mode == 2 then do
      match base with | some base => pure base | none => abiInputWord cfg false 64
    else abiReserve (BitVec.ofNat 256 (((headSize+selectorBytes+31)/32*32) + if header then 32 else 0)) false
  if !dynamic && header then abiStore pointer (BitVec.ofNat 256 (headSize+selectorBytes))
  let data := pointer + if header then 32 else 0
  if let some selector := selector then abiStore data selector
  let start := data+BitVec.ofNat 256 selectorBytes
  let mut head := start
  let mut tail := start+BitVec.ofNat 256 headSize
  for ((shape,value),memorySlice) in (shapes.zip args).zip memorySlices do
    let source ← if memorySlice then do
        let (pointer,_) ← abiSlice value
        pure (if shape.dynamic then .value (abiScalar pointer) false else .memory pointer)
      else pure (.value value false)
    tail ← abiEncodeNode 33 cfg shape source head start tail
    head := head+BitVec.ofNat 256 shape.head
  let total := tail-start+BitVec.ofNat 256 selectorBytes
  let result ← if dynamic && mode != 2 then do
      let size := if header then ((total+63) &&& ~~~(31 : Word)) else tail-start+BitVec.ofNat 256 (if hasSelector then 32 else 0)
      if header && total.toNat+63 ≥ 2^256 then throw (.reverted (.panic 0x41))
      let pointer ← abiReserve size false
      if header then abiStore pointer total
      pure pointer
    else pure pointer
  return if mode == 1 then abiScalar result else .aggregate [abiScalar (result + if header then 32 else 0),abiScalar total]

def abiEncodeTree (cfg : Config) (frame : Frame) (mode : Nat) (hasBase hasSelector : Bool)
    (shapes : List AbiEncodeShape) (memorySlices : List Bool) (args : List Value) : Except String (Outcome (Value × Frame)) :=
  match (abiEncodeFields cfg mode hasBase hasSelector shapes memorySlices args).run ⟨frame,2048⟩ with
  | .error (.unsupported reason) => .error reason
  | .error (.reverted data) => .ok (.reverted data)
  | .error .invalid => .ok .invalid
  | .ok (value,state) => .ok (.returned (value,state.frame))

/- Exact staging of selector-bearing, static error payloads. The mode-2 encoder
   reads the cursor and does not reserve or advance it. -/

private def abiStageWords : Word → List Word → AbiM Unit
  | _, [] => pure ()
  | address, word :: words => do
    abiSpend 1
    abiStore address word
    abiStageWords (address + 32) words

private def abiEncodeStatic (cfg : Config) (selector : Word) (words : List Word) : AbiM Value := do
  let base ← abiInputWord cfg false 64
  abiStore base selector
  abiStageWords (base + 4) words
  return .aggregate [abiScalar base, abiScalar (BitVec.ofNat 256 (4 + 32 * words.length))]

private theorem abiEncodeNode_word (cfg : Config) (rule : AbiRule)
    (word head tupleBase tail : Word) :
    abiEncodeNode 33 cfg (.word rule) (.value (.scalar ⟨256,word⟩) false)
        head tupleBase tail = (do abiSpend 1; abiStore head word; pure tail) := by
  rfl

private theorem static_total (base size : Word) : base + size - (base + 4#256) + 4#256 = size := by
  rw [← BitVec.sub_sub, BitVec.sub_add_cancel, BitVec.add_comm base size, BitVec.add_sub_cancel]

private theorem abiEncodeFields_one (cfg : Config) (rule : AbiRule) (selector a : Word) :
    abiEncodeFields cfg 2 false true [.word rule] [false] [abiScalar selector, abiScalar a] =
      abiEncodeStatic cfg selector [a] := by
  simp [abiEncodeFields, abiEncodeStatic, abiStageWords, abiEncodeNode_word, abiWord, abiScalar,
    Value.word, Scalar.ofWord, AbiEncodeShape.head, AbiEncodeShape.dynamic,
    AbiEncodeShape.layout, BitVec.add_assoc]
  apply congrArg (fun (next : Word → AbiM Value) => abiInputWord cfg false 64 >>= next)
  funext base
  rw [static_total base 36#256]

private theorem abiEncodeFields_three (cfg : Config) (ra rb rc : AbiRule)
    (selector a b c : Word) :
    abiEncodeFields cfg 2 false true [.word ra, .word rb, .word rc] [false, false, false]
        [abiScalar selector, abiScalar a, abiScalar b, abiScalar c] =
      abiEncodeStatic cfg selector [a,b,c] := by
  simp [abiEncodeFields, abiEncodeStatic, abiStageWords, abiEncodeNode_word, abiWord, abiScalar,
    Value.word, Scalar.ofWord, AbiEncodeShape.head, AbiEncodeShape.dynamic,
    AbiEncodeShape.layout, BitVec.add_assoc]
  apply congrArg (fun (next : Word → AbiM Value) => abiInputWord cfg false 64 >>= next)
  funext base
  rw [static_total base 100#256]

private theorem abiInputWord_expanded (cfg : Config) (state : AbiState) (active : List Byte)
    (step : expand state.frame.memory 64 32 = .ok active) :
    (abiInputWord cfg false 64).run state =
      .ok (readWord active 64, {state with frame := {state.frame with memory := active}}) := by
  simp [abiInputWord, abiMemory, StateT.run, Bind.bind, StateT.bind, Except.bind,
    MonadState.get, getThe, MonadStateOf.get, StateT.get, MonadStateOf.set, StateT.set,
    Pure.pure, StateT.pure, Except.pure, step]

private theorem abiStore_expanded (state : AbiState) (address word : Word) (active : List Byte)
    (step : expand state.frame.memory address.toNat 32 = .ok active) :
    (abiStore address word).run state =
      .ok ((), {state with frame := {state.frame with
        memory := writeBytes active address.toNat (wordBytes word)}}) := by
  simp [abiStore, abiMemory, StateT.run, Bind.bind, StateT.bind, Except.bind,
    MonadState.get, getThe, MonadStateOf.get, StateT.get, MonadStateOf.set, StateT.set,
    Pure.pure, StateT.pure, Except.pure, step]

private theorem abiSpend_one (state : AbiState) (positive : 0 < state.work) :
    (abiSpend 1).run state = .ok ((), {state with work := state.work - 1}) := by
  simp [abiSpend, StateT.run, Bind.bind, StateT.bind, Except.bind, MonadState.get,
    getThe, MonadStateOf.get, StateT.get, MonadStateOf.set, StateT.set,
    Pure.pure, Except.pure, Nat.ne_of_gt positive]

def stageWord (memory : List Byte) (address : Nat) (word : Word) : Except String (List Byte) := do
  let active ← expand memory address 32
  return writeBytes active address (wordBytes word)

def stageWords (memory : List Byte) (address : Nat) : List Word → Except String (List Byte)
  | [] => .ok memory
  | word :: words => do
    let memory ← stageWord memory address word
    stageWords memory (address + 32) words

private theorem abiStore_staged (state : AbiState) (address word : Word) (next : List Byte)
    (step : stageWord state.frame.memory address.toNat word = .ok next) :
    (abiStore address word).run state =
      .ok ((), {state with frame := {state.frame with memory := next}}) := by
  cases h : expand state.frame.memory address.toNat 32 with
  | error reason => simp [stageWord, h, Bind.bind, Except.bind] at step
  | ok active =>
    simp only [stageWord, Bind.bind, Except.bind, h, Pure.pure, Except.pure, Except.ok.injEq] at step
    subst next
    exact abiStore_expanded _ _ _ _ h

private theorem bind_abiRun {α β : Type} (action : AbiM α) (next : α → AbiM β) (state : AbiState) :
    (StateT.bind action next).run state = (do let (value, state) ← action.run state; (next value).run state) := rfl

private theorem abiStageWords_run (state : AbiState) (address : Word) (words : List Word)
    (next : List Byte) (work : words.length ≤ state.work)
    (fits : address.toNat + 32 * words.length < 2^256)
    (staged : stageWords state.frame.memory address.toNat words = .ok next) :
    (abiStageWords address words).run state = .ok ((), {state with
      frame := {state.frame with memory := next}, work := state.work - words.length}) := by
  induction words generalizing state address with
  | nil =>
    simp only [stageWords, Except.ok.injEq] at staged
    subst next
    cases state
    rfl
  | cons word words ih =>
    cases step : stageWord state.frame.memory address.toNat word with
    | error reason => simp [stageWords, Bind.bind, Except.bind, step] at staged
    | ok updated =>
      have positive : 0 < state.work := by simp only [List.length_cons] at work; omega
      let frame := {state.frame with memory := updated}
      let spent : AbiState := ⟨frame, state.work - 1⟩
      have advance : (address + 32).toNat = address.toNat + 32 := by
        rw [BitVec.toNat_add]
        have within : address.toNat + 32 < 2^256 := by
          simp only [List.length_cons, Nat.mul_succ] at fits
          omega
        exact Nat.mod_eq_of_lt within
      have fitsTail : (address + 32).toNat + 32 * words.length < 2^256 := by
        rw [advance]
        simp only [List.length_cons, Nat.mul_succ] at fits
        omega
      have workTail : words.length ≤ spent.work := by simp only [List.length_cons] at work; dsimp [spent]; omega
      have rest : stageWords spent.frame.memory (address + 32).toNat words = .ok next := by
        rw [advance]
        simpa only [stageWords, Bind.bind, Except.bind, step] using staged
      have store := abiStore_staged {state with work := state.work - 1} address word updated step
      rw [abiStageWords]
      dsimp only [Bind.bind]
      rw [bind_abiRun, abiSpend_one state positive]
      dsimp only [Bind.bind, Except.bind]
      rw [bind_abiRun, store]
      change (abiStageWords (address + 32) words).run spent = _
      rw [ih spent _ workTail fitsTail rest]
      have remaining : spent.work - words.length = state.work - (word :: words).length := by
        simp only [List.length_cons]
        dsimp [spent]
        omega
      cases state
      simp [spent, frame, remaining]

private theorem abiEncodeStatic_run (cfg : Config) (frame : Frame) (selector : Word)
    (words : List Word) (active staged final : List Byte)
    (read : expand frame.memory 64 32 = .ok active)
    (written : stageWord active (readWord active 64).toNat selector = .ok staged)
    (rest : stageWords staged ((readWord active 64).toNat + 4) words = .ok final)
    (work : words.length ≤ 2048)
    (fits : (readWord active 64).toNat + 4 + 32 * words.length < 2^256) :
    (abiEncodeStatic cfg selector words).run ⟨frame,2048⟩ =
      .ok (.aggregate [abiScalar (readWord active 64), abiScalar (BitVec.ofNat 256 (4 + 32 * words.length))],
        ⟨{frame with memory := final},2048 - words.length⟩) := by
  let base := readWord active 64
  have advance : (base + 4).toNat = base.toNat + 4 := by
    rw [BitVec.toNat_add]
    exact Nat.mod_eq_of_lt (by change (readWord active 64).toNat + 4 < 2^256; omega)
  have done := abiStageWords_run ⟨{frame with memory := staged},2048⟩
    (base + 4) words final work (by simpa only [advance] using fits) (by simpa only [advance] using rest)
  rw [abiEncodeStatic]
  dsimp only [Bind.bind]
  rw [bind_abiRun, abiInputWord_expanded cfg ⟨frame,2048⟩ active read]
  dsimp only [Bind.bind, Except.bind]
  rw [bind_abiRun, abiStore_staged _ _ _ _ written]
  dsimp only [Bind.bind, Except.bind]
  rw [bind_abiRun, done]
  rfl

theorem abiEncodeTree_static_one (cfg : Config) (frame : Frame) (rule : AbiRule)
    (selector a : Word) (active staged final : List Byte)
    (read : expand frame.memory 64 32 = .ok active)
    (written : stageWord active (readWord active 64).toNat selector = .ok staged)
    (rest : stageWords staged ((readWord active 64).toNat + 4) [a] = .ok final)
    (fits : (readWord active 64).toNat + 36 ≤ 65536) :
    abiEncodeTree cfg frame 2 false true [.word rule] [false] [abiScalar selector, abiScalar a] =
      .ok (.returned (.aggregate [abiScalar (readWord active 64), abiScalar 36],
        {frame with memory := final})) := by
  have bound : (readWord active 64).toNat + 4 + 32 * ([a] : List Word).length < 2^256 := by
    change (readWord active 64).toNat + 36 < 2^256
    omega
  have work : ([a] : List Word).length ≤ 2048 := by
    change 1 ≤ 2048
    decide
  unfold abiEncodeTree
  rw [abiEncodeFields_one, abiEncodeStatic_run _ _ _ _ _ _ _ read written rest work bound]
  rfl

theorem abiEncodeTree_static_three (cfg : Config) (frame : Frame) (ra rb rc : AbiRule)
    (selector a b c : Word) (active staged final : List Byte)
    (read : expand frame.memory 64 32 = .ok active)
    (written : stageWord active (readWord active 64).toNat selector = .ok staged)
    (rest : stageWords staged ((readWord active 64).toNat + 4) [a, b, c] = .ok final)
    (fits : (readWord active 64).toNat + 100 ≤ 65536) :
    abiEncodeTree cfg frame 2 false true [.word ra, .word rb, .word rc] [false, false, false] [abiScalar selector, abiScalar a, abiScalar b, abiScalar c] =
      .ok (.returned (.aggregate [abiScalar (readWord active 64), abiScalar 100],
        {frame with memory := final})) := by
  have bound : (readWord active 64).toNat + 4 + 32 * ([a, b, c] : List Word).length < 2^256 := by
    change (readWord active 64).toNat + 100 < 2^256
    omega
  have work : ([a, b, c] : List Word).length ≤ 2048 := by
    change 3 ≤ 2048
    decide
  unfold abiEncodeTree
  rw [abiEncodeFields_three, abiEncodeStatic_run _ _ _ _ _ _ _ read written rest work bound]
  rfl

end SolarTV.MIR.State
