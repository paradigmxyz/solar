import EvmRules.Word

/-! Keccak-256 with the EVM's original Keccak padding (0x01, not SHA3's 0x06).
The state is 25 little-endian 64-bit lanes, indexed x + 5*y. Each of the 24
Keccak-f rounds performs theta, rho/pi, chi and iota; the rate is 136 bytes.
There is no hash oracle or collision-freedom axiom. Symbolic hashes may exceed
the prover's limits, which must remain inconclusive rather than uninterpreted.
Reference: https://keccak.team/files/Keccak-reference-3.0.pdf, Algorithms 1-6. -/

namespace SolarTV.Keccak

abbrev Byte := BitVec 8
abbrev Lane := BitVec 64
abbrev State := List Lane

def constants : List Lane := [
  0x0000000000000001, 0x0000000000008082, 0x800000000000808A, 0x8000000080008000,
  0x000000000000808B, 0x0000000080000001, 0x8000000080008081, 0x8000000000008009,
  0x000000000000008A, 0x0000000000000088, 0x0000000080008009, 0x000000008000000A,
  0x000000008000808B, 0x800000000000008B, 0x8000000000008089, 0x8000000000008003,
  0x8000000000008002, 0x8000000000000080, 0x000000000000800A, 0x800000008000000A,
  0x8000000080008081, 0x8000000000008080, 0x0000000080000001, 0x8000000080008008]

def rotations : List Nat := [
  0, 1, 62, 28, 27, 36, 44, 6, 55, 20, 3, 10, 43, 25, 39,
  41, 45, 15, 21, 8, 18, 2, 61, 56, 14]

def lane (state : State) (x y : Nat) : Lane := state[x % 5 + 5 * (y % 5)]?.getD 0

def rotate (value : Lane) (amount : Nat) : Lane :=
  (value <<< (amount % 64)) ||| (value >>> ((64 - amount % 64) % 64))

def round (state : State) (constant : Lane) : State :=
  let parity := (List.range 5).map (fun x =>
    (List.range 5).foldl (fun acc y => acc ^^^ lane state x y) 0)
  let theta := (List.range 25).map (fun i =>
    state[i]?.getD 0 ^^^ parity[(i % 5 + 4) % 5]?.getD 0 ^^^
      rotate (parity[(i % 5 + 1) % 5]?.getD 0) 1)
  let rhoPi := (List.range 25).foldl (fun result i =>
    let x := i % 5
    let y := i / 5
    result.set (y + 5 * ((2 * x + 3 * y) % 5))
      (rotate (theta[i]?.getD 0) (rotations[i]?.getD 0))) (List.replicate 25 0)
  let chi := (List.range 25).map (fun i =>
    lane rhoPi (i % 5) (i / 5) ^^^
      ((~~~lane rhoPi (i % 5 + 1) (i / 5)) &&& lane rhoPi (i % 5 + 2) (i / 5)))
  chi.set 0 (chi[0]?.getD 0 ^^^ constant)

def permutation (state : State) : State := constants.foldl round state

def pad (data : List Byte) : List Byte :=
  if data.length % 136 = 135 then data ++ [0x81]
  else data ++ [1] ++ List.replicate (134 - data.length % 136) 0 ++ [0x80]

def littleWord (data : List Byte) (offset : Nat) : Lane :=
  (List.range 8).foldl (fun acc i =>
    acc ||| ((data[offset + i]?.getD 0).setWidth 64 <<< (8 * i))) 0

def absorb (state : State) (block : List Byte) : State :=
  permutation ((List.range 25).map (fun i =>
    state[i]?.getD 0 ^^^ if i < 17 then littleWord block (8 * i) else 0))

def hash (data : List Byte) : EvmRules.Word :=
  let padded := pad data
  let state := (List.range (padded.length / 136)).foldl (fun state i =>
    absorb state ((padded.drop (136 * i)).take 136)) (List.replicate 25 0)
  (List.range 32).foldl (fun acc i => (acc <<< 8) |||
    (((state[i / 8]?.getD 0) >>> (8 * (i % 8))).setWidth 8).setWidth 256) 0

end SolarTV.Keccak
