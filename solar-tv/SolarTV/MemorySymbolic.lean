import SolarTV.Machine

/-! Read-over-write laws of `Machine.Mem` at symbolic addresses.

`Mem` is a total byte map with an active-word count, so a computed offset is just a
natural number: nothing here bounds an address or assumes two pointers are distinct.
Every law that moves a read past a write takes the disjointness, containment or exact
match of the two ranges as a hypothesis over natural numbers, for `omega` (or a caller's
`omega`-based discharger over word `toNat`s) to prove from the path's cases. When it
cannot, the law does not fire and the read stays over the write: overlapping or unproved
ranges fail closed. `read_store` and `bytes_store` give the byte-exact fallback for an
overlap. Expansion stays exact: `words_store` and `words_write` are the active-word
count the semantics defines, with no rounding or bound. -/

namespace SolarTV.Machine.Mem
open EvmRules

/-- The 32 big-endian bytes `store` writes. -/
def wordBytes (value : Word) : List Memory.Byte := (List.range 32).map (Memory.byte value)

@[simp] theorem wordBytes_length (value : Word) : (wordBytes value).length = 32 := by
  simp [wordBytes]

@[simp] theorem read_length (m : Mem) (offset size : Nat) : (m.read offset size).length = size := by
  simp [read]

@[simp] theorem read_zero (m : Mem) (offset : Nat) : m.read offset 0 = [] := rfl

/-! Bytes and active size after each operation. -/

@[simp] theorem bytes_expand (m : Mem) (offset size : Nat) : (m.expand offset size).bytes = m.bytes := by
  unfold expand; split <;> rfl

theorem words_expand (m : Mem) (offset size : Nat) : (m.expand offset size).words =
    if size = 0 then m.words else Memory.grow m.words (BitVec.ofNat 251 ((offset + size + 31) / 32)) := by
  unfold expand; split <;> rfl

theorem bytes_store (m : Mem) (offset : Nat) (value : Word) (i : Nat) :
    (m.store offset value).bytes i =
      if offset ≤ i ∧ i < offset + 32 then Memory.byte value (i - offset) else m.bytes i := by
  simp [store]

theorem words_store (m : Mem) (offset : Nat) (value : Word) :
    (m.store offset value).words = Memory.grow m.words (BitVec.ofNat 251 ((offset + 32 + 31) / 32)) := by
  simp [store, words_expand]

theorem bytes_storeByte (m : Mem) (offset : Nat) (value : Word) (i : Nat) :
    (m.storeByte offset value).bytes i = if i = offset then value.setWidth 8 else m.bytes i := by
  simp [storeByte]

theorem words_storeByte (m : Mem) (offset : Nat) (value : Word) :
    (m.storeByte offset value).words = Memory.grow m.words (BitVec.ofNat 251 ((offset + 1 + 31) / 32)) := by
  simp [storeByte, words_expand]

theorem bytes_write (m : Mem) (offset size : Nat) (source : Nat → Memory.Byte) (i : Nat) :
    (m.write offset size source).bytes i =
      if offset ≤ i ∧ i < offset + size then source (i - offset) else m.bytes i := by
  simp [write]

theorem words_write (m : Mem) (offset size : Nat) (source : Nat → Memory.Byte) :
    (m.write offset size source).words = (m.expand offset size).words := by
  simp [write, words_expand]

/-- Growth to a size memory already has changes nothing. -/
theorem grow_of_le (words required : BitVec 251) (h : required.toNat ≤ words.toNat) :
    Memory.grow words required = words := by
  unfold Memory.grow
  simp only [BitVec.ult_iff_toNat_lt]
  split <;> first | rfl | omega

/-- Growth past the active size is exactly the required size. -/
theorem grow_of_lt (words required : BitVec 251) (h : words.toNat < required.toNat) :
    Memory.grow words required = required := by
  unfold Memory.grow
  simp only [BitVec.ult_iff_toNat_lt]
  split <;> first | rfl | omega

/-! Reads. -/

@[simp] theorem read_expand (m : Mem) (offset size address count : Nat) :
    (m.expand offset size).read address count = m.read address count := by
  simp [read]

/-- Reading two adjacent ranges is reading their union. -/
theorem read_append (m : Mem) (address k l : Nat) :
    m.read address (k + l) = m.read address k ++ m.read (address + k) l := by
  unfold read
  rw [List.range_add, List.map_append, List.map_map]
  congr 1
  apply List.map_congr_left
  intro i _
  simp [Function.comp, Nat.add_assoc]

/-- A range longer than a word as its first word and the rest, for word-by-word reads of
`abi.encode` output. The lower bound keeps the rewrite from unfolding a word into itself; the
upper bound (32 words) keeps it to the short encodings a hash reads, so a long or symbolic
range is never unfolded byte-word by byte-word. -/
theorem read_split (m : Mem) (address count : Nat) (h : 32 < count) (bounded : count ≤ 1024) :
    m.read address count = m.read address 32 ++ m.read (address + 32) (count - 32) := by
  have := read_append m address 32 (count - 32)
  rwa [Nat.add_sub_cancel' (by omega)] at this

/-- A read is unchanged by a store outside it. -/
theorem read_store_disjoint (m : Mem) (offset : Nat) (value : Word) (address count : Nat)
    (h : address + count ≤ offset ∨ offset + 32 ≤ address) :
    (m.store offset value).read address count = m.read address count := by
  unfold read
  apply List.map_congr_left
  intro i hi
  simp only [List.mem_range] at hi
  rw [bytes_store]
  have : ¬(offset ≤ address + i ∧ address + i < offset + 32) := by omega
  simp only [this, ite_false]

/-- A load is unchanged by a store whose word it does not overlap. -/
theorem load_store_disjoint (m : Mem) (offset : Nat) (value : Word) (address : Nat)
    (h : address + 32 ≤ offset ∨ offset + 32 ≤ address) :
    (m.store offset value).load address = m.load address := by
  unfold load
  rw [read_store_disjoint m offset value address 32 h]

/-- A read is unchanged by a byte store outside it. -/
theorem read_storeByte_disjoint (m : Mem) (offset : Nat) (value : Word) (address count : Nat)
    (h : address + count ≤ offset ∨ offset < address) :
    (m.storeByte offset value).read address count = m.read address count := by
  unfold read
  apply List.map_congr_left
  intro i hi
  simp only [List.mem_range] at hi
  rw [bytes_storeByte]
  have : ¬(address + i = offset) := by omega
  simp only [this, ite_false]

theorem load_storeByte_disjoint (m : Mem) (offset : Nat) (value : Word) (address : Nat)
    (h : address + 32 ≤ offset ∨ offset < address) :
    (m.storeByte offset value).load address = m.load address := by
  unfold load
  rw [read_storeByte_disjoint m offset value address 32 h]

/-- A read is unchanged by a copy, calldata or return data write outside it. -/
theorem read_write_disjoint (m : Mem) (offset size : Nat) (source : Nat → Memory.Byte)
    (address count : Nat) (h : address + count ≤ offset ∨ offset + size ≤ address) :
    (m.write offset size source).read address count = m.read address count := by
  unfold read
  apply List.map_congr_left
  intro i hi
  simp only [List.mem_range] at hi
  rw [bytes_write]
  have : ¬(offset ≤ address + i ∧ address + i < offset + size) := by omega
  simp only [this, ite_false]

/-- The word a store wrote, read back as bytes, as `KECCAK256` over it reads them. -/
theorem read_store_same (m : Mem) (offset : Nat) (value : Word) :
    (m.store offset value).read offset 32 = wordBytes value := by
  unfold read wordBytes
  apply List.map_congr_left
  intro i hi
  simp only [List.mem_range] at hi
  rw [bytes_store]
  have : offset ≤ offset + i ∧ offset + i < offset + 32 := by omega
  simp only [this, and_self, ite_true, Nat.add_sub_cancel_left]

/-- A word stored at an offset equal to the read's, as `omega` proves it (such as
`(p + 32).toNat` and `p.toNat + 32` when the sum does not wrap). -/
theorem read_store_at (m : Mem) (offset : Nat) (value : Word) (address count : Nat)
    (h : address = offset) (hc : count = 32) :
    (m.store offset value).read address count = wordBytes value := by
  subst h hc; exact read_store_same m address value

theorem load_store_at (m : Mem) (offset : Nat) (value : Word) (address : Nat) (h : address = offset) :
    (m.store offset value).load address = value := by
  subst h; exact load_store_same m address value

/-- A read inside the range a write wrote reads the source at the same distance. -/
theorem read_write_inside (m : Mem) (offset size : Nat) (source : Nat → Memory.Byte)
    (address count : Nat) (lo : offset ≤ address) (hi : address + count ≤ offset + size) :
    (m.write offset size source).read address count =
      (List.range count).map (fun i => source (address - offset + i)) := by
  unfold read
  apply List.map_congr_left
  intro i hi'
  simp only [List.mem_range] at hi'
  rw [bytes_write]
  have c : offset ≤ address + i ∧ address + i < offset + size := by omega
  have e : address + i - offset = address - offset + i := by omega
  simp only [c, and_self, ite_true, e]

/-- The whole range a write wrote reads its source, at any symbolic length. -/
theorem read_write_same (m : Mem) (offset size : Nat) (source : Nat → Memory.Byte) :
    (m.write offset size source).read offset size = (List.range size).map source := by
  rw [read_write_inside m offset size source offset size (Nat.le_refl _) (Nat.le_refl _)]
  simp

/-- An `MCOPY` read back inside its destination: the source memory's bytes, at the source
offset moved by the same distance. `from` is the memory the copy read, which `apply_mcopy`
expands first; `read_expand` then removes that expansion. -/
theorem read_copy_inside (m src : Mem) (offset size source address count : Nat)
    (lo : offset ≤ address) (hi : address + count ≤ offset + size) :
    (m.write offset size (fun i => src.bytes (source + i))).read address count =
      src.read (source + (address - offset)) count := by
  rw [read_write_inside m offset size _ address count lo hi]
  unfold read
  apply List.map_congr_left
  intro i _
  simp [Nat.add_assoc]

/-- A store's word followed by bytes it does not touch. -/
theorem read_store_head (m : Mem) (offset : Nat) (value : Word) (count : Nat) :
    (m.store offset value).read offset (32 + count) = wordBytes value ++ m.read (offset + 32) count := by
  rw [read_append, read_store_same,
    read_store_disjoint m offset value (offset + 32) count (Or.inr (Nat.le_refl _))]

/-- Byte-exact reading over any store, overlapping or not: the fallback when neither
disjointness nor containment holds. -/
theorem read_store (m : Mem) (offset : Nat) (value : Word) (address count : Nat) :
    (m.store offset value).read address count = (List.range count).map (fun i =>
      if offset ≤ address + i ∧ address + i < offset + 32 then
        Memory.byte value (address + i - offset) else m.bytes (address + i)) := by
  unfold read
  apply List.map_congr_left
  intro i _
  rw [bytes_store]

/-- A range of length zero, as `omega` proves it. -/
theorem read_of_eq_zero (m : Mem) (address count : Nat) (h : count = 0) : m.read address count = [] := by
  subst h; rfl

/-- The whole range a write wrote, at offsets and lengths `omega` proves equal. -/
theorem read_write_at (m : Mem) (offset size : Nat) (source : Nat → Memory.Byte) (address count : Nat)
    (h : address = offset) (hc : count = size) :
    (m.write offset size source).read address count = (List.range size).map source := by
  subst h hc; exact read_write_same m address count source

/-- A range that contains a stored word: the bytes before it, the word, the bytes after it.
This is how a hash over a region built word by word at computed offsets meets its words. -/
theorem read_store_within (m : Mem) (offset : Nat) (value : Word) (address count : Nat)
    (lo : address ≤ offset) (hi : offset + 32 ≤ address + count) :
    (m.store offset value).read address count =
      m.read address (offset - address) ++ wordBytes value ++
        m.read (offset + 32) (address + count - (offset + 32)) := by
  have hk : count = (offset - address) + (32 + (address + count - (offset + 32))) := by omega
  have ho : address + (offset - address) = offset := by omega
  conv => lhs; rw [hk, read_append, read_append, ho]
  rw [read_store_disjoint m offset value address (offset - address) (Or.inl (by omega)),
    read_store_same,
    read_store_disjoint m offset value (offset + 32) _ (Or.inr (Nat.le_refl _)),
    List.append_assoc]

/-- A range that contains a copy's destination: the bytes before it, the copied bytes at any
symbolic length, the bytes after it, as a hash over a header and a copied array reads them. -/
theorem read_write_within (m : Mem) (offset size : Nat) (source : Nat → Memory.Byte)
    (address count : Nat) (lo : address ≤ offset) (hi : offset + size ≤ address + count) :
    (m.write offset size source).read address count =
      m.read address (offset - address) ++ (List.range size).map source ++
        m.read (offset + size) (address + count - (offset + size)) := by
  have hk : count = (offset - address) + (size + (address + count - (offset + size))) := by omega
  have ho : address + (offset - address) = offset := by omega
  conv => lhs; rw [hk, read_append, read_append, ho]
  rw [read_write_disjoint m offset size source address (offset - address) (Or.inl (by omega)),
    read_write_same,
    read_write_disjoint m offset size source (offset + size) _ (Or.inr (Nat.le_refl _)),
    List.append_assoc]

end SolarTV.Machine.Mem
