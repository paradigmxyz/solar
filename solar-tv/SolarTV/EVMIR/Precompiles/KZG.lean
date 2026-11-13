import SolarTV.EVMIR.Precompiles.BLS12381
import SolarTV.EVMIR.Precompiles.SHA256

/-! EIP-4844 point evaluation against Ethereum's fixed KZG ceremony G2 point.
Compressed points are decoded canonically and checked for subgroup membership;
infinity is permitted. Scalars are canonical Fr values. Verification evaluates
the actual pairing product e(C-yG1,-G2) * e(proof,tauG2-zG2) = 1.
The trusted setup is public protocol data, not a result oracle or pairing axiom.
https://eips.ethereum.org/EIPS/eip-4844#point-evaluation-precompile
https://github.com/ethereum/consensus-specs/blob/dev/specs/deneb/polynomial-commitments.md -/

namespace SolarTV.Precompiles.KZG
open EVM.State BLS12381

def compressed1 (input : List Byte) : Option (Point Nat) := do
  if input.length != 48 then none else
    let flags := input[0]?.getD 0 |>.toNat
    let x := readBE input 0 48 % 2^381
    if flags/128 != 1 || x ≥ p then none
    else if flags/64%2 = 1 then
      if x = 0 && flags/32%2 = 0 then some (infinity f) else none
    else
      let y ← sqrt p (f.add (f.mul (f.square x) x) 4)
      let y := if (y > p/2) == (flags/32%2 = 1) then y else f.neg y
      let point := affine f x y
      if isInfinity f (multiply f 0 point order) then some point else none

def compressed2 (input : List Byte) : Option (Point F2) := do
  if input.length != 96 then none else
    let flags := input[0]?.getD 0 |>.toNat
    let xi := readBE input 0 48 % 2^381
    let xr := readBE input 48 48
    if flags/128 != 1 || xi ≥ p || xr ≥ p then none
    else if flags/64%2 = 1 then
      if xi = 0 && xr = 0 && flags/32%2 = 0 then some (infinity f2) else none
    else
      let x := (xr,xi)
      let y ← sqrt2 (f2.add (f2.mul (f2.square x) x) (4,4))
      let sign := y.2 > p/2 || (y.2 = 0 && y.1 > p/2)
      let y := if sign == (flags/32%2 = 1) then y else f2.neg y
      let point := affine f2 x y
      if isInfinity f2 (multiply f2 f2.zero point order) then some point else none

def tau : Option (Point F2) := compressed2 (be 96
  0xb5bfd7dd8cdeb128843bc287230af38926187075cbfbefa81009a2ce615ac53d2914e5870cb452d2afaaab24f3499f72185cbfee53492714734429b7b38608e23926c911cceceac9a36851477ba4c60b087041de621000edc98edada20c1def2)

def evaluate (input : List Byte) : Option (List Byte) := do
  if input.length != 192 then none else
    let commitment := (input.drop 96).take 48
    let expected := [1] ++ (SHA256.hash commitment).drop 1
    if input.take 32 != expected then none else
      let z := readBE input 32 32; let y := readBE input 64 32
      if z ≥ order || y ≥ order then none else
        let c ← compressed1 commitment
        let proof ← compressed1 (input.drop 144)
        let tau ← tau
        let left := add f 0 c (negate f (multiply f 0 generator1 y))
        let right := add f2 f2.zero tau (negate f2 (multiply f2 f2.zero generator2 z))
        if Pairing.check p order true [(left,negate f2 generator2),(proof,right)] then
          some (be 32 4096 ++ be 32 order)
        else none

end SolarTV.Precompiles.KZG
