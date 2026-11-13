import SolarTV.EVMIR.State.Decode

/-! EIP-7702 execution after authorization processing. Exactly 23-byte designators
resolve once from Prague onward. The resolved target access costs 100/2600 gas,
after warming the authority and before the EIP-150 gas split, even if a call is
later rejected for balance/depth. Resolution warmth survives child revert.
Code introspection reads the original account bytes; execution uses the resolved
image. Delegation to a precompile does not invoke it. A second designator is
executed as ordinary (invalid) bytecode, not recursively resolved.
Authorization signatures, transaction intrinsic gas and authorization refunds
precede this entry model; the fixture supplies their resulting account state.
https://eips.ethereum.org/EIPS/eip-7702#delegation-indicator -/

namespace SolarTV.EVM.State

structure ExecutionCode where
  address : Address
  bytes : List Byte
  delegated : Bool

def delegationTarget (fork : Fork) (code : List Byte) : Option Address :=
  if fork.atLeast .prague && code.length = 23 && code.take 3 == [0xef,0x01,0x00] then
    some ((readWord (List.replicate 12 0 ++ code.drop 3) 0).setWidth 160)
  else none

def executionCode (fork : Fork) (world : Journal) (destination : Address) :
    ExecutionCode × Nat × Journal :=
  let code := (account world destination).code
  let world := warmAddress world destination
  match delegationTarget fork code with
  | none => (⟨destination,code,false⟩,0,world)
  | some target =>
    let price := if world.warmAddresses.contains target then 100 else 2600
    (⟨target,(account world target).code,true⟩,price,warmAddress world target)

end SolarTV.EVM.State
