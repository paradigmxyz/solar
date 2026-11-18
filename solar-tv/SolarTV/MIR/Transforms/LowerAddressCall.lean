import SolarTV.MIR.State.Messages

/-! The explicit-gas, zero-value address call with memory-object input lowers
to a header load and a raw CALL. This local lemma preserves the complete call
request, including the requested gas word, and the frame after loading the
header. It does not price the caller prefix that produced the gas word or
certify the code that consumes the return buffer. -/

namespace SolarTV.MIR.LowerAddressCall
open EvmRules SolarTV.MIR.State

theorem memoryObjectArguments (cfg : Config) (frame : Frame)
    (address input gas : Operand) (target requested : Word) (object : Scalar)
    (ha : read frame address = .ok target)
    (hi : readValue frame input = .ok (.scalar object))
    (hg : read frame gas = .ok requested) :
    messageArguments cfg frame (.address 0xf1 true false true false false) [address,input,gas] =
      (do
        let result ← memoryOperation cfg frame .load [object.word]
        match result with
        | .returned (some length,next) =>
          pure (0xf1,[target,0,object.word+32,length,0,0],next,some requested)
        | _ => .error "expected CALL input length") := by
  simp [messageArguments,memoryOperation,ha,hi,hg,Except.bind,Bind.bind,Functor.map,Except.map,
    Pure.pure,Except.pure,required,value]
  cases Execution.expand frame.memory object.word.toNat 32 <;> rfl

end SolarTV.MIR.LowerAddressCall
