import SolarTV.EVMIR.Precompiles.Bytes

/-! RIPEMD-160's two parallel 80-round compression lanes, with little-endian
words, length padding and digest encoding. The precompile adds its twelve zero
output bytes separately. Constants follow the designers' reference specification.
https://homes.esat.kuleuven.be/~bosselae/ripemd160.html -/

namespace SolarTV.Precompiles.RIPEMD160
open EVM.State
abbrev W := BitVec 32

def leftIndex : List Nat := [
  0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,
  7,4,13,1,10,6,15,3,12,0,9,5,2,14,11,8,
  3,10,14,4,9,15,8,1,2,7,0,6,13,11,5,12,
  1,9,11,10,0,8,12,4,13,3,7,15,14,5,6,2,
  4,0,5,9,7,12,2,10,14,1,3,8,11,6,15,13]
def rightIndex : List Nat := [
  5,14,7,0,9,2,11,4,13,6,15,8,1,10,3,12,
  6,11,3,7,0,13,5,10,14,15,8,12,4,9,1,2,
  15,5,1,3,7,14,6,9,11,8,12,2,10,0,4,13,
  8,6,4,1,3,11,15,0,5,12,2,13,9,7,10,14,
  12,15,10,4,1,5,8,7,6,2,13,14,0,3,9,11]
def leftShift : List Nat := [
  11,14,15,12,5,8,7,9,11,13,14,15,6,7,9,8,
  7,6,8,13,11,9,7,15,7,12,15,9,11,7,13,12,
  11,13,6,7,14,9,13,15,14,8,13,6,5,12,7,5,
  11,12,14,15,14,15,9,8,9,14,5,6,8,6,5,12,
  9,15,5,11,6,8,13,12,5,12,13,14,11,8,5,6]
def rightShift : List Nat := [
  8,9,9,11,13,15,15,5,7,7,8,11,14,14,12,6,
  9,13,15,7,12,8,9,11,7,7,12,7,6,15,13,11,
  9,7,15,11,8,6,6,14,12,13,5,14,13,13,7,5,
  15,5,8,11,14,14,6,14,6,9,12,9,12,5,15,8,
  8,5,12,9,12,5,14,6,8,13,6,5,15,13,11,11]
def constant (right : Bool) (round : Nat) : W :=
  (if right then [0x50a28be6,0x5c4dd124,0x6d703ef3,0x7a6d76e9,0]
    else [0,0x5a827999,0x6ed9eba1,0x8f1bbcdc,0xa953fd4e])[round]?.getD 0
def boolean (round : Nat) (x y z : W) : W :=
  match round with
  | 0 => x ^^^ y ^^^ z
  | 1 => (x &&& y) ||| (~~~x &&& z)
  | 2 => (x ||| ~~~y) ^^^ z
  | 3 => (x &&& z) ||| (y &&& ~~~z)
  | _ => x ^^^ (y ||| ~~~z)
def lane (right : Bool) (words state : List W) : List W :=
  (List.range 80).foldl (fun s j =>
    let a := s[0]?.getD 0; let b := s[1]?.getD 0; let c := s[2]?.getD 0
    let d := s[3]?.getD 0; let e := s[4]?.getD 0
    let index := (if right then rightIndex else leftIndex)[j]?.getD 0
    let shift := (if right then rightShift else leftShift)[j]?.getD 0
    let f := boolean (if right then 4-j/16 else j/16) b c d
    let t := (a + f + words[index]?.getD 0 + constant right (j/16)).rotateLeft shift + e
    [e,t,b,c.rotateLeft 10,d]) state
def initial : List W := [0x67452301,0xefcdab89,0x98badcfe,0x10325476,0xc3d2e1f0]
def compress (s : List W) (block : List Byte) : List W :=
  let words := (List.range 16).map (fun i => BitVec.ofNat 32 (readLE block (4*i) 4))
  let a := lane false words s
  let b := lane true words s
  [s[1]?.getD 0 + a[2]?.getD 0 + b[3]?.getD 0,
   s[2]?.getD 0 + a[3]?.getD 0 + b[4]?.getD 0,
   s[3]?.getD 0 + a[4]?.getD 0 + b[0]?.getD 0,
   s[4]?.getD 0 + a[0]?.getD 0 + b[1]?.getD 0,
   s[0]?.getD 0 + a[1]?.getD 0 + b[2]?.getD 0]
def hash (input : List Byte) : List Byte :=
  let pad := (64 - ((input.length + 9) % 64)) % 64
  let padded := input ++ [128] ++ List.replicate pad 0 ++ le 8 (input.length * 8)
  let state := (List.range (padded.length / 64)).foldl
    (fun s i => compress s ((padded.drop (i*64)).take 64)) initial
  state.flatMap (fun w => le 4 w.toNat)

end SolarTV.Precompiles.RIPEMD160
