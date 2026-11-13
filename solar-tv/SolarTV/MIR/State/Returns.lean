import SolarTV.MIR.State.Layout

/-! The lowered memory-return convention at an explicit MIR call boundary.
The first scalar is returned through the call result; later components occupy
buffer + 32*k, and word 0x20 publishes that buffer. The fixture supplies its
address, not the backend. Tail calls retain the original callee's convention.
At the selected outer SSA boundary, return components are observed logically;
there is no invented caller or implicit buffer write. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

def publishReturns (cfg : Config) (owner : Function) (values : List Value) (frame : Frame) : Except String Frame := do
  if values.length ≤ 1 then return frame
  let name ← required owner.returnBuffer "missing MIR memory-return convention"
  let address ← required (← placementBinding cfg name).returnBuffer "missing MIR return buffer placement"
  let mut memory := frame.memory
  for (value,index) in values.drop 1 |>.zipIdx do
    let word ← value.word
    let target := address+BitVec.ofNat 256 ((index+1)*32)
    memory ← expand memory target.toNat 32
    memory := writeBytes memory target.toNat (wordBytes word)
  memory ← expand memory 32 32
  return {frame with memory := writeBytes memory 32 (wordBytes address)}

end SolarTV.MIR.State
