import SolarTV.MIR.State.Messages

/-! The explicit-gas, zero-value address call with memory-object input lowers
to a header load and a raw CALL. This local lemma preserves the complete call
request, including the requested gas word, and the frame after loading the
header. It does not price the caller prefix that produced the gas word or
certify the code that consumes the return buffer. -/

namespace SolarTV.MIR.LowerAddressCall
open EvmRules SolarTV.MIR.State

def loweredMemoryObjectArgs (cfg : Config) (frame : Frame)
    (target requested object : Word) : Except String (Nat × List Word × Frame × Option Word) := do
  let result ← memoryOperation cfg frame .load [object]
  match result with
  | .returned (some length,next) =>
    pure (0xf1,[target,0,object+32,length,0,0],next,some requested)
  | _ => .error "expected CALL input length"

theorem memoryObjectArguments (cfg : Config) (frame : Frame)
    (address input gas : Operand) (target requested : Word) (object : Scalar)
    (ha : read frame address = .ok target)
    (hi : readValue frame input = .ok (.scalar object))
    (hg : read frame gas = .ok requested) :
    messageArguments cfg frame (.address 0xf1 true false true false false) [address,input,gas] =
      loweredMemoryObjectArgs cfg frame target requested object.word := by
  simp [messageArguments,loweredMemoryObjectArgs,memoryOperation,ha,hi,hg,
    Except.bind,Bind.bind,Functor.map,Except.map,
    Pure.pure,Except.pure,required,value]
  cases Execution.expand frame.memory object.word.toNat 32 <;> rfl

/-- A sample before a caller prefix still requests the full EIP-150 allowance
when the gas available after the CALL charges is at most that sample. -/
theorem forwardSample (fork : Fork) (available : Nat) (requested : Word)
    (hasValue : Bool) (eip150 : fork.atLeast .tangerineWhistle = true)
    (remaining : available ≤ requested.toNat) :
    Calls.forwardGas fork available requested.toNat hasValue =
      Calls.forwardGas fork available available hasValue := by
  have cap : available - available / 64 ≤ available := Nat.sub_le _ _
  simp [Calls.forwardGas, eip150, Nat.min_eq_right cap,
    Nat.min_eq_right (Nat.le_trans cap remaining)]

end SolarTV.MIR.LowerAddressCall
