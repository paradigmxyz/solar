import SolarTV.MIR.State.AbiEncode

/-! Evaluated require payloads. Passing checks leave memory untouched. Failing
checks emit exact Error(string), short-word or custom-error bytes, and dynamic
payloads use the direct recursive ABI model. An empty string is not an empty
revert. Resource limits and unsupported values remain model errors. -/

namespace SolarTV.MIR.State
open EvmRules SolarTV.Execution

def requireLiteral (bytes : List Byte) : List Byte :=
  [0x08,0xc3,0x79,0xa0] ++ wordBytes 32 ++ wordBytes (BitVec.ofNat 256 bytes.length) ++
    bytes ++ List.replicate ((32-bytes.length%32)%32) 0

def abiRequirePayload (cfg : Config) (frame : Frame) (op : RequireOp) (args : List Value) : Step := do
  let condition :: rest := args | .error "require argument count"
  if (← condition.word) != 0 then return .returned (none,frame) else
  match op with
  | .literal bytes => return .reverted (RevertData.fromBytes (requireLiteral bytes))
  | .short =>
    let [length,data] := rest | .error "require short argument count"
    return .reverted (RevertData.fromBytes
      ([0x08,0xc3,0x79,0xa0] ++ wordBytes 32 ++ wordBytes (← length.word) ++ wordBytes (← data.word)))
  | .encode custom shapes memorySlices =>
    let (selector,values) ← if custom then do
        let selector :: values := rest | .error "require selector missing"
        pure (selector,values)
      else pure (abiScalar (BitVec.ofNat 256 (0x08c379a0*2^224)),rest)
    match ← abiEncodeTree cfg frame 2 false true shapes memorySlices (selector :: values) with
    | .reverted data => return .reverted data
    | .invalid => return .invalid
    | .returned (result,next) =>
      let .aggregate [base,length] := result | .error "require ABI slice missing"
      let base ← base.word
      let length ← length.word
      let memory ← expand next.memory base.toNat length.toNat
      return .reverted (RevertData.fromBytes (readBytes memory base.toNat length.toNat))

end SolarTV.MIR.State
