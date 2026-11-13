import SolarTV.MIR.State.Allocation

/-! Placement/link context is an explicit input of a MIR window, not backend
output synthesized by the validator. Code extents and constructor ranges are
validated at import and independently at replay. Immutables use staging memory
in construction, or a supplied typed runtime relocation; writes cannot change
runtime relocations. Missing bindings remain unsupported. GAS samples are single
opcode windows: the input budget precedes the opcode's own two-gas charge and
no physical function entry/return or general MIR caller pricing is claimed. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

def layoutBinding (entries : List (Nat × α)) (id : Nat) : Option α :=
  (entries.find? (·.1 == id)).map Prod.snd

def placementBinding (cfg : Config) (function : String) : Except String FunctionPlacement :=
  required ((cfg.layout.functions.find? (·.1 == function)).map Prod.snd) "missing MIR function placement"

def activationBase (cfg : Config) (function : String) (activation : Nat) : Except String Word := do
  let placement ← placementBinding cfg function
  if placement.frameBases.isEmpty then
    if activation = 0 then required placement.frameBase "missing MIR frame base"
    else .error "missing MIR activation frame base"
  else required placement.frameBases[activation]? "missing MIR activation frame base"

/-- A fixed placement does not read or bump the FMP; initialization remains explicit. -/
def deferredAllocate (cfg : Config) (frame : Frame) (function : String) (site : Nat)
    (size : Word) (zeroed checked : Bool) : Step := do
  let sites ← required ((cfg.layout.allocations.find? (·.1 == function)).map Prod.snd) "missing deferred allocation placements"
  let placement ← required (layoutBinding sites site) "missing deferred allocation site"
  let extent := match placement with | .fixed _ size | .heap size => size
  if extent != size.toNat then .error "deferred allocation extent disagrees with placement" else
  match placement with
  | .heap _ => allocate frame size zeroed checked
  | .fixed address _ =>
    if address.toNat+extent ≥ 2^64 then .error "invalid deferred allocation placement" else
    let memory ← if zeroed then do
        let memory ← expand frame.memory address.toNat extent
        pure (writeBytes memory address.toNat (List.replicate extent 0))
      else pure frame.memory
    return .returned (some address,{frame with memory})

def layoutOperation (cfg : Config) (frame : Frame) (op : ContextOp) (args : List Word) (activation : Nat := 0) : Step := do
  let result (word : Word) := Except.ok (Outcome.returned (some word,frame))
  match op,args with
  | .heapFloor name,[] => result (← required (← placementBinding cfg name).heapFloor "missing MIR heap floor")
  | .frameAddress name offset,[] =>
    result ((← activationBase cfg name activation) + BitVec.ofNat 256 offset)
  | .constructorRange isEnd,[] =>
    let (base,size) ← required cfg.layout.constructor "missing constructor range"
    result (BitVec.ofNat 256 (base + if isEnd then size else 0))
  | .dataSize id addend aligned,[] =>
    let (_,size) ← required (layoutBinding cfg.layout.data id) "missing linked data extent"
    let size := size + addend
    result (BitVec.ofNat 256 (if aligned then size/32*32 else size))
  | .library id,[] => result (← required (layoutBinding cfg.layout.libraries id) "missing library relocation")
  | .immutableRead id,[] =>
    match layoutBinding cfg.layout.immutableSlots id with
    | some address =>
      let memory ← expand frame.memory address 32
      return .returned (some (readWord memory address),{frame with memory})
    | none => result (← required (layoutBinding cfg.layout.immutableValues id) "missing immutable relocation")
  | .immutableStore id,[word] =>
    let address ← required (layoutBinding cfg.layout.immutableSlots id) "immutable write requires constructor staging"
    let memory ← expand frame.memory address 32
    return .returned (none,{frame with memory := writeBytes memory address (wordBytes word)})
  | .gasSample,[] =>
    let budget ← required cfg.layout.gasBefore "missing GAS window budget"
    if budget < 2 then return .invalid else result (BitVec.ofNat 256 (budget-2))
  | _,_ => .error "MIR layout operation operands"

end SolarTV.MIR.State
