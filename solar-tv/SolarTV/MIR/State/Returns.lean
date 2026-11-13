import SolarTV.MIR.State.Layout

/-! The lowered memory-return convention at an explicit MIR call boundary.
The first scalar leaf is returned through the physical call result; later leaves occupy
buffer + 32*k, and word 0x20 publishes that buffer. The fixture supplies its
address, not the backend. Tail calls retain the original callee's convention.
At the selected outer SSA boundary, return components are observed logically;
there is no invented caller or implicit buffer write. A placement selects physical
publication for one logical aggregate or slice. Without one, a single logical
result remains an SSA value. SSA callers retain their typed logical component
before lowering. Only stored extras must be defined. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

/-- Flatten shape without choosing bits for undefined leaves, including the unused first word. -/
def returnLeaves : Nat → ValueType → Value → Except String (List Value)
  | 0,_,_ => .error "MIR aggregate type depth limit"
  | fuel + 1,ty,value => do
    match ty.fields with
    | none => return [value]
    | some types =>
      let values ← match value with
        | .undefined => pure (List.replicate types.length Value.undefined)
        | .aggregate fields => pure fields
        | _ => .error "MIR aggregate return shape mismatch"
      if types.length != values.length then .error "MIR aggregate return arity" else
      return (← (types.zip values).mapM (fun (ty,value) => returnLeaves fuel ty value)).flatten

def publishReturns (cfg : Config) (owner : Function) (values : List Value) (frame : Frame) : Except String Frame := do
  let placement := owner.returnBuffer.bind (fun name =>
    (cfg.layout.functions.find? (·.1 == name)).bind (·.2.returnBuffer))
  if values.length ≤ 1 && placement.isNone then return frame
  if owner.returns.length != values.length then .error "MIR return owner arity mismatch" else
  let leaves ← (owner.returns.zip values).mapM (fun (ty,value) => returnLeaves 34 ty value)
  let extras := leaves.flatten.drop 1
  if extras.isEmpty then return frame
  let name ← required owner.returnBuffer "missing MIR memory-return convention"
  let address ← required (← placementBinding cfg name).returnBuffer "missing MIR return buffer placement"
  let words ← extras.mapM Value.word
  let mut memory := frame.memory
  for (word,index) in words.zipIdx do
    let target := address+BitVec.ofNat 256 ((index+1)*32)
    memory ← expand memory target.toNat 32
    memory := writeBytes memory target.toNat (wordBytes word)
  memory ← expand memory 32 32
  return {frame with memory := writeBytes memory 32 (wordBytes address)}

/-- A placed pair publishes exactly the same bytes before and after product lowering. -/
theorem publish_pair (cfg : Config) (frame : Frame) (name : String) (address : Word)
    (first extra : Value)
    (placed : ((cfg.layout.functions.find? (·.1 == name)).bind (·.2.returnBuffer)) = some address) :
    publishReturns cfg ⟨[],[.product [.word 256,.word 256]],[],some name⟩
      [.aggregate [first,extra]] frame =
    publishReturns cfg ⟨[],[.word 256,.word 256],[],some name⟩ [first,extra] frame := by
  unfold publishReturns
  simp only [Option.bind_some,placed]
  rfl

/-- Slice publication retains pointer/length order through the same physical pair convention. -/
theorem publish_slice (cfg : Config) (frame : Frame) (name : String) (address : Word)
    (location : Nat) (pointer length : Value)
    (placed : ((cfg.layout.functions.find? (·.1 == name)).bind (·.2.returnBuffer)) = some address) :
    publishReturns cfg ⟨[],[.slice location],[],some name⟩ [.aggregate [pointer,length]] frame =
    publishReturns cfg ⟨[],[.pointer location,.word 256],[],some name⟩ [pointer,length] frame := by
  unfold publishReturns
  simp only [Option.bind_some,placed]
  rfl

end SolarTV.MIR.State
