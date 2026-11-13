import SolarTV.MemorySymbolic

/-! Kernel-checked uses and negatives of the symbolic memory laws, in the shapes ABI
encoding and order hashing take: words stored at a computed free pointer and hashed as one
range, a calldata copy of variable length hashed in place, the free pointer advanced past
it, and an `MCOPY` read back. The negatives show that a wrong offset, a wrong length or an
overlapping write changes the bytes, and that `mem_simp` refuses to move a read past a write
it cannot prove disjoint. -/

namespace SolarTV.Machine.Mem
open EvmRules

/-- Read-over-write by the laws, each side condition by `omega`. -/
macro "mem_simp" : tactic => `(tactic| repeat (first
  | simp (disch := omega) only [read_expand, read_store_at, read_store_disjoint, load_store_at,
      load_store_disjoint, read_storeByte_disjoint, load_storeByte_disjoint, read_write_disjoint,
      load_write_of_disjoint, read_write_same, read_write_inside, read_copy_inside, load_expand,
      read_store_within, read_write_within, read_of_eq_zero, read_write_at, List.append_nil, List.nil_append,
      List.append_assoc]
  | simp only [read_split, Nat.reduceLT, Nat.reduceLeDiff, Nat.reduceSub, Nat.reduceAdd]))

/-- `abi.encode(typehash, a, b)` at a free pointer `p` above the scratch and pointer words,
hashed over its 96 bytes: the hash input is the three words, and the pointer word is intact. -/
theorem encode3_read (m : Mem) (p : Nat) (h : 0x60 ≤ p) (t a b : Word) :
    (((m.store p t).store (p + 32) a).store (p + 64) b).read p 96 =
      wordBytes t ++ wordBytes a ++ wordBytes b ∧
    (((m.store p t).store (p + 32) a).store (p + 64) b).load 0x40 = m.load 0x40 := by
  constructor <;> mem_simp

/-- A calldata copy of symbolic length `n` at `p`, the next word stored at `p + n` (the
pointer advanced by the copy), then the copy hashed: the hash reads exactly the copied bytes. -/
theorem copy_then_store_read (m : Mem) (p n : Nat) (src : Nat → Memory.Byte) (w : Word) :
    ((m.write p n src).store (p + n) w).read p n = (List.range n).map src := by
  mem_simp

/-- An order hash over `p .. p+n` followed by an encoding at the advanced pointer: the
encoding's word reads back, the copy is untouched. -/
theorem copy_encode (m : Mem) (p n : Nat) (src : Nat → Memory.Byte) (t w : Word) :
    (((m.write p n src).store (p + n) t).store (p + n + 32) w).read (p + n) 64 =
      wordBytes t ++ wordBytes w ∧
    (((m.write p n src).store (p + n) t).store (p + n + 32) w).read p n = (List.range n).map src := by
  constructor <;> mem_simp

/-- `MCOPY` of `n` bytes from `s` to `d`, read back at a computed offset inside. -/
theorem mcopy_read (m : Mem) (d s n k : Nat) (hk : k + 32 ≤ n) :
    let src := m.expand s n
    (src.write d n (fun i => src.bytes (s + i))).read (d + k) 32 = m.read (s + k) 32 := by
  intro src
  rw [read_copy_inside _ _ _ _ _ _ _ (by omega) (by omega)]
  simp [src, Nat.add_sub_cancel_left]

/-! Negatives: concrete memories where the wrong claim is false, checked by the kernel. -/

set_option maxRecDepth 8192 in
/-- Wrong offset: the encoding read one byte late is not the encoded words. -/
theorem wrong_offset : ¬ ∀ (m : Mem) (p : Nat) (t a : Word),
    ((m.store p t).store (p + 32) a).read (p + 1) 64 = wordBytes t ++ wordBytes a := by
  intro h
  have := congrArg (·[31]?) (h initial 0 1 0)
  revert this
  decide

/-- Wrong length: 64 bytes are not three words. -/
theorem wrong_length : ¬ ∀ (m : Mem) (p : Nat) (t a b : Word),
    (((m.store p t).store (p + 32) a).store (p + 64) b).read p 64 =
      wordBytes t ++ wordBytes a ++ wordBytes b := by
  intro h
  have := congrArg List.length (h initial 0 0 0 0)
  simp at this

set_option maxRecDepth 8192 in
/-- Overlapping write: a store sixteen bytes into a word replaces its low half. -/
theorem overlapping_write : ¬ ∀ (m : Mem) (p : Nat) (a b : Word),
    ((m.store p a).store (p + 16) b).read p 32 = wordBytes a := by
  intro h
  have := congrArg (·[31]?) (h initial 0 1 0)
  revert this
  decide

/-- The procedure fails closed on the overlap: it cannot move the read past the second store. -/
example (m : Mem) (p : Nat) (a b : Word) : True := by
  fail_if_success
    have : ((m.store p a).store (p + 16) b).read p 32 = wordBytes a := by mem_simp
  trivial

/-- ... and on a store it cannot place: an unrelated pointer `q` may alias `p`. -/
example (m : Mem) (p q : Nat) (a b : Word) : True := by
  fail_if_success
    have : ((m.store p a).store q b).read p 32 = wordBytes a := by mem_simp
  trivial

/-- With the path's bound on `q`, the same read moves. -/
example (m : Mem) (p q : Nat) (a b : Word) (h : p + 32 ≤ q) :
    ((m.store p a).store q b).read p 32 = wordBytes a := by mem_simp

/-! Exact expansion at symbolic offsets. -/

example (m : Mem) (p : Nat) (v : Word) (h : (BitVec.ofNat 251 ((p + 63) / 32)).toNat ≤ m.words.toNat) :
    (m.store p v).words = m.words := by
  rw [words_store]; exact grow_of_le _ _ (by simpa using h)

/-! Containment: a hash range that holds stores and copies at computed offsets. -/

/-- An order-hash shape: a type hash at `p`, a copied array of symbolic length `n` after it,
hashed as one range of `32 + n` bytes. -/
theorem typehash_array_read (m : Mem) (p n : Nat) (t : Word) (src : Nat → Memory.Byte) :
    ((m.store p t).write (p + 32) n src).read p (32 + n) = wordBytes t ++ (List.range n).map src := by
  mem_simp

/-- A word stored at a computed offset `q` inside a hashed range of symbolic length. -/
theorem store_within_read (m : Mem) (p q k : Nat) (v : Word) (lo : p ≤ q) (hi : q + 32 ≤ p + k) :
    (m.store q v).read p k = m.read p (q - p) ++ wordBytes v ++ m.read (q + 32) (p + k - (q + 32)) := by
  mem_simp

/-- Wrong split: a word that starts inside the range but ends past it is not inside it. -/
theorem straddling_store : ¬ ∀ (m : Mem) (p : Nat) (v : Word),
    (m.store (p + 16) v).read p 32 = m.read p 16 ++ wordBytes v ++ m.read (p + 48) 0 := by
  intro h
  have := congrArg List.length (h initial 0 0)
  simp at this

/-- The procedure refuses the straddling store. -/
example (m : Mem) (p : Nat) (v : Word) : True := by
  fail_if_success
    have : (m.store (p + 16) v).read p 32 = m.read p 16 ++ wordBytes v := by mem_simp
  trivial

end SolarTV.Machine.Mem

/-- info: 'SolarTV.Machine.Mem.encode3_read' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms SolarTV.Machine.Mem.encode3_read
/-- info: 'SolarTV.Machine.Mem.copy_then_store_read' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms SolarTV.Machine.Mem.copy_then_store_read
/-- info: 'SolarTV.Machine.Mem.copy_encode' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms SolarTV.Machine.Mem.copy_encode
/-- info: 'SolarTV.Machine.Mem.mcopy_read' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms SolarTV.Machine.Mem.mcopy_read
/-- info: 'SolarTV.Machine.Mem.wrong_offset' depends on axioms: [propext] -/
#guard_msgs in #print axioms SolarTV.Machine.Mem.wrong_offset
/-- info: 'SolarTV.Machine.Mem.wrong_length' depends on axioms: [propext] -/
#guard_msgs in #print axioms SolarTV.Machine.Mem.wrong_length
/-- info: 'SolarTV.Machine.Mem.overlapping_write' depends on axioms: [propext] -/
#guard_msgs in #print axioms SolarTV.Machine.Mem.overlapping_write
/-- info: 'SolarTV.Machine.Mem.read_store_disjoint' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms SolarTV.Machine.Mem.read_store_disjoint
/-- info: 'SolarTV.Machine.Mem.load_store_disjoint' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms SolarTV.Machine.Mem.load_store_disjoint
/-- info: 'SolarTV.Machine.Mem.read_storeByte_disjoint' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms SolarTV.Machine.Mem.read_storeByte_disjoint
/-- info: 'SolarTV.Machine.Mem.read_write_disjoint' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms SolarTV.Machine.Mem.read_write_disjoint
/-- info: 'SolarTV.Machine.Mem.read_store_same' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms SolarTV.Machine.Mem.read_store_same
/-- info: 'SolarTV.Machine.Mem.read_write_inside' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms SolarTV.Machine.Mem.read_write_inside
/-- info: 'SolarTV.Machine.Mem.read_write_same' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms SolarTV.Machine.Mem.read_write_same
/-- info: 'SolarTV.Machine.Mem.read_copy_inside' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms SolarTV.Machine.Mem.read_copy_inside
/-- info: 'SolarTV.Machine.Mem.read_store_head' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms SolarTV.Machine.Mem.read_store_head
/-- info: 'SolarTV.Machine.Mem.read_split' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms SolarTV.Machine.Mem.read_split
/-- info: 'SolarTV.Machine.Mem.read_append' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms SolarTV.Machine.Mem.read_append
/-- info: 'SolarTV.Machine.Mem.grow_of_le' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms SolarTV.Machine.Mem.grow_of_le
/-- info: 'SolarTV.Machine.Mem.grow_of_lt' depends on axioms: [propext] -/
#guard_msgs in #print axioms SolarTV.Machine.Mem.grow_of_lt
/-- info: 'SolarTV.Machine.Mem.words_store' depends on axioms: [propext] -/
#guard_msgs in #print axioms SolarTV.Machine.Mem.words_store
/-- info: 'SolarTV.Machine.Mem.read_store' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms SolarTV.Machine.Mem.read_store
/-- info: 'SolarTV.Machine.Mem.typehash_array_read' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms SolarTV.Machine.Mem.typehash_array_read
/-- info: 'SolarTV.Machine.Mem.store_within_read' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms SolarTV.Machine.Mem.store_within_read
/-- info: 'SolarTV.Machine.Mem.straddling_store' depends on axioms: [propext] -/
#guard_msgs in #print axioms SolarTV.Machine.Mem.straddling_store
/-- info: 'SolarTV.Machine.Mem.read_store_within' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms SolarTV.Machine.Mem.read_store_within
/-- info: 'SolarTV.Machine.Mem.read_write_within' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms SolarTV.Machine.Mem.read_write_within
