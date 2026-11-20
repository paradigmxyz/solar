import SolarTV.EVMIR.Precompiles.Bytes

/-! The BLAKE2b compression function F from RFC 7693, section 3.2. This is the
compression precompile, not a padded BLAKE2 hash. Rounds are decoded big-endian;
state, message, counters and output are little-endian 64-bit words.
https://www.rfc-editor.org/rfc/rfc7693#section-3.2
https://eips.ethereum.org/EIPS/eip-152 -/

namespace SolarTV.Precompiles.Blake2F
open EVM.State
abbrev W := BitVec 64
def iv : List W := [0x6a09e667f3bcc908,0xbb67ae8584caa73b,0x3c6ef372fe94f82b,0xa54ff53a5f1d36f1,
  0x510e527fade682d1,0x9b05688c2b3e6c1f,0x1f83d9abfb41bd6b,0x5be0cd19137e2179]
def sigma : List (List Nat) := [
  [0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15],
  [14,10,4,8,9,15,13,6,1,12,0,2,11,7,5,3],
  [11,8,12,0,5,2,15,13,10,14,3,6,7,1,9,4],
  [7,9,3,1,13,12,11,14,2,6,5,10,4,0,15,8],
  [9,0,5,7,2,4,10,15,14,1,11,12,6,8,3,13],
  [2,12,6,10,0,11,8,3,4,13,7,5,15,14,1,9],
  [12,5,1,15,14,13,4,10,0,7,6,3,9,2,8,11],
  [13,11,7,14,12,1,3,9,5,0,15,4,8,6,2,10],
  [6,15,14,9,11,3,0,8,12,2,13,7,1,4,10,5],
  [10,2,8,4,7,6,1,5,15,11,9,14,3,12,13,0]]
def mix (v : List W) (a b c d : Nat) (x y : W) : List W :=
  let va := v[a]?.getD 0 + v[b]?.getD 0 + x
  let vd := (v[d]?.getD 0 ^^^ va).rotateRight 32
  let vc := v[c]?.getD 0 + vd
  let vb := (v[b]?.getD 0 ^^^ vc).rotateRight 24
  let va := va + vb + y
  let vd := (vd ^^^ va).rotateRight 16
  let vc := vc + vd
  let vb := (vb ^^^ vc).rotateRight 63
  (((v.set a va).set b vb).set c vc).set d vd
def compress (input : List Byte) : List Byte := Id.run do
  let rounds := readBE input 0 4
  let h := (List.range 8).map (fun i => BitVec.ofNat 64 (readLE input (4+8*i) 8))
  let m := (List.range 16).map (fun i => BitVec.ofNat 64 (readLE input (68+8*i) 8))
  let mut v := h ++ iv
  v := v.set 12 (v[12]?.getD 0 ^^^ BitVec.ofNat 64 (readLE input 196 8))
  v := v.set 13 (v[13]?.getD 0 ^^^ BitVec.ofNat 64 (readLE input 204 8))
  if input[212]?.getD 0 == 1 then v := v.set 14 (~~~(v[14]?.getD 0))
  for round in List.range rounds do
    let s := sigma[round % 10]?.getD []
    let indices := [(0,4,8,12),(1,5,9,13),(2,6,10,14),(3,7,11,15),
      (0,5,10,15),(1,6,11,12),(2,7,8,13),(3,4,9,14)]
    for (j,(a,b,c,d)) in indices.zipIdx.map (fun (v,i) => (i,v)) do
      v := mix v a b c d (m[s[2*j]?.getD 0]?.getD 0) (m[s[2*j+1]?.getD 0]?.getD 0)
  return (List.range 8).flatMap (fun i => le 8 (h[i]?.getD 0 ^^^ v[i]?.getD 0 ^^^ v[i+8]?.getD 0).toNat)

end SolarTV.Precompiles.Blake2F
