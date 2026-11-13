import SolarTV.Stages.Assembly
import SolarTV.Machine

/-! Linking immutables into the runtime bytecode.

An immutable is fixed at deployment: the deployment code writes its value over each of its
placeholders, the zero bytes of a `PUSH<bytes>` in the runtime code, as the big-endian encoding
of its low `bytes` bytes (`link`). The bytecode a stage is checked against is the code before
that, as the compiler emits it, with each placeholder an `Item.immutable` that walks as a push of
zeros. `placeholdersOf` reads the placeholders off a walk: each at the byte after its push.

`walk_link` shows that for every value of the immutables, the linear decoding of the linked code
is the walk's stream with each placeholder pushing its value modulo `256 ^ bytes`
(`linkedStream`), and `linked_instructions` that those are the program's items with each
placeholder linked. `placed`, which the kernel evaluates, checks what that takes of the walk:
each placeholder is a `PUSH<bytes>` of its width. So a proof that the walk accepts the unlinked
bytecode binds every bytecode a deployment runs: its hash is that of `link` of the code with the
placeholders the manifest lists. `Machine.immutableWord_toNat` connects the pushed value with
the word the stages read. No axioms beyond the standard three. -/

namespace SolarTV.Assembly

/-- An immutable placeholder: the immutable, the offset of its bytes and their count. -/
structure Placeholder where
  id : Nat
  start : Nat
  bytes : Nat
  deriving DecidableEq, Repr

/-- Whether byte `i` of the code is one of the placeholder's. -/
def Placeholder.covers (p : Placeholder) (i : Nat) : Bool :=
  decide (p.start ≤ i) && decide (i < p.start + p.bytes)

/-- Byte `j` of the `n`-byte big-endian encoding of `value`. -/
def beByte (value n j : Nat) : Nat := value / 256 ^ (n - 1 - j) % 256

/-- Byte `i` of the linked code, `b` before linking. -/
def linkByte (ps : List Placeholder) (values : Nat → Nat) (i b : Nat) : Nat :=
  match ps.find? (·.covers i) with
  | some p => beByte (values p.id) p.bytes (i - p.start)
  | none => b

/-- `code` as a deployment runs it: the bytes of each placeholder the big-endian encoding of its
immutable's value. -/
def link (code : List Nat) (ps : List Placeholder) (values : Nat → Nat) : List Nat :=
  code.mapIdx (linkByte ps values)

/-- The placeholders of a walk's immutable items, each at the byte after its push. -/
def placeholdersOf : List (Nat × Instr) → List Item → List Placeholder
  | (pc, _) :: stream, item :: items =>
    match item with
    | .immutable id bytes => ⟨id, pc + 1, bytes⟩ :: placeholdersOf stream items
    | _ => placeholdersOf stream items
  | _, _ => []

/-- Whether each immutable item of a walk is a `PUSH<bytes>` of its width, `1 ≤ bytes ≤ 32`. -/
def placed (code : List Nat) : List (Nat × Instr) → List Item → Bool
  | (pc, _) :: stream, item :: items =>
    match item with
    | .immutable _ bytes =>
      decide (1 ≤ bytes) && decide (bytes ≤ 32) && code[pc]? == some (PUSH1 + bytes - 1) &&
        placed code stream items
    | _ => placed code stream items
  | _, _ => true

/-- A walk's stream with each placeholder pushing its immutable's value. -/
def linkedStream (values : Nat → Nat) : List (Nat × Instr) → List Item → List (Nat × Instr)
  | (pc, instr) :: stream, item :: items =>
    match item with
    | .immutable id bytes => (pc, .push (values id % 256 ^ bytes)) :: linkedStream values stream items
    | _ => (pc, instr) :: linkedStream values stream items
  | _, _ => []

/-- The instruction an item is once a deployment links it. -/
def Item.linked (offset : Nat → Nat) (values : Nat → Nat) : Item → Instr
  | .immutable id bytes => .push (values id % 256 ^ bytes)
  | item => item.expected offset

theorem bigEndian_append (xs : List Nat) (b : Nat) :
    bigEndian (xs ++ [b]) = bigEndian xs * 256 + b := by
  simp [bigEndian, List.foldl_append]

/-- The big-endian bytes of a value are the value modulo the bytes' range. -/
theorem bigEndian_beBytes (n value : Nat) :
    bigEndian ((List.range n).map (beByte value n)) = value % 256 ^ n := by
  induction n generalizing value with
  | zero => simp [bigEndian, Nat.mod_one]
  | succ n ih =>
    rw [List.range_succ, List.map_append, List.map_singleton, bigEndian_append]
    have hpre : (List.range n).map (beByte value (n + 1)) =
        (List.range n).map (beByte (value / 256) n) := by
      apply List.map_congr_left
      intro j hj
      have hj := List.mem_range.1 hj
      unfold beByte
      have e : n + 1 - 1 - j = (n - 1 - j) + 1 := by omega
      rw [e, Nat.pow_succ, Nat.mul_comm, ← Nat.div_div_eq_div_mul]
    rw [hpre, ih]
    unfold beByte
    simp only [Nat.add_sub_cancel, Nat.sub_self, Nat.pow_zero, Nat.div_one]
    rw [Nat.pow_succ, Nat.mul_comm (256 ^ n) 256, Nat.mod_mul]
    omega

theorem link_length (code : List Nat) (ps : List Placeholder) (values : Nat → Nat) :
    (link code ps values).length = code.length := by
  simp [link]

theorem link_getElem? (code : List Nat) (ps : List Placeholder) (values : Nat → Nat) (i : Nat) :
    (link code ps values)[i]? = code[i]?.map (linkByte ps values i) := by
  simp [link, List.getElem?_mapIdx]

/-- A byte no placeholder covers is the code's. -/
theorem link_getElem?_of_uncovered {code : List Nat} {ps : List Placeholder} {values : Nat → Nat}
    {i : Nat} (h : ∀ p ∈ ps, p.covers i = false) : (link code ps values)[i]? = code[i]? := by
  rw [link_getElem?]
  have : ps.find? (·.covers i) = none := List.find?_eq_none.2 (fun p hp => by simp [h p hp])
  cases code[i]? <;> simp [linkByte, this]

/-- The placeholder that covers a byte, when every one that does is the same. -/
theorem find?_covers {ps : List Placeholder} {p : Placeholder} {i : Nat} (mem : p ∈ ps)
    (covers : p.covers i = true) (unique : ∀ q ∈ ps, q.covers i = true → q = p) :
    ps.find? (·.covers i) = some p := by
  induction ps with
  | nil => simp at mem
  | cons q qs ih =>
    by_cases hq : q.covers i = true
    · have := unique q (List.mem_cons_self) hq
      subst this
      simp [hq]
    · have hq' : q.covers i = false := by simpa using hq
      simp only [List.find?_cons, hq']
      rcases List.mem_cons.1 mem with rfl | later
      · exact absurd covers hq
      · exact ih later (fun r hr => unique r (List.mem_cons_of_mem _ hr))

/-- Decoding reads only the bytes of the instruction and the code's length. -/
theorem decode_congr {code code' : List Nat} {pc next : Nat} {instr : Instr}
    (len : code'.length = code.length)
    (same : ∀ i, pc ≤ i → i < next → code'[i]? = code[i]?)
    (h : decode code pc = some (instr, next)) : decode code' pc = some (instr, next) := by
  have adv := decode_advances h
  have hpc := same pc (Nat.le_refl _) adv
  unfold decode at h ⊢
  rw [hpc]
  cases hc : code[pc]? with
  | none => rw [hc] at h; simp at h
  | some byte =>
    rw [hc] at h
    simp only at h ⊢
    by_cases h0 : byte = PUSH0
    · simp only [h0, ↓reduceIte] at h ⊢; exact h
    · simp only [h0, ↓reduceIte] at h ⊢
      by_cases hr : PUSH1 ≤ byte ∧ byte ≤ PUSH32
      · simp only [hr, and_self, ↓reduceIte] at h ⊢
        by_cases fits : pc + 1 + (byte - PUSH1 + 1) ≤ code.length
        · simp only [fits, ↓reduceIte] at h
          simp only [Option.some.injEq, Prod.mk.injEq] at h
          obtain ⟨rfl, rfl⟩ := h
          have take : (code'.drop (pc + 1)).take (byte - PUSH1 + 1) =
              (code.drop (pc + 1)).take (byte - PUSH1 + 1) := by
            apply List.ext_getElem?
            intro j
            simp only [List.getElem?_take, List.getElem?_drop]
            split
            · exact same _ (by omega) (by omega)
            · rfl
          simp only [len, fits, take, ↓reduceIte]
        · simp [fits] at h
      · simp only [hr, ↓reduceIte] at h ⊢; exact h

/-- The offsets a walk reaches are at least where it starts. -/
theorem walk_ge {code : List Nat} {offset : Nat → Nat} {pc : Nat} {items : List Item}
    {stream : List (Nat × Instr)} (w : Walk code offset pc items stream) :
    ∀ e ∈ stream, pc ≤ e.1 := by
  induction w with
  | nil => simp
  | cons decoded _ _ ih =>
    have := decode_advances decoded
    intro e he
    rcases List.mem_cons.1 he with rfl | later
    · exact Nat.le_refl _
    · have := ih e later
      omega

/-- The placeholders of a walk start after where it starts. -/
theorem placeholdersOf_gt {code : List Nat} {offset : Nat → Nat} {pc : Nat} {items : List Item}
    {stream : List (Nat × Instr)} (w : Walk code offset pc items stream) :
    ∀ p ∈ placeholdersOf stream items, pc < p.start := by
  induction w with
  | nil => simp [placeholdersOf]
  | @cons pc next item items stream decoded _ rest ih =>
    have adv := decode_advances decoded
    intro p hp
    cases item <;> simp only [placeholdersOf] at hp
    case immutable id bytes =>
      rcases List.mem_cons.1 hp with rfl | later
      · simp
      · have := ih p later
        omega
    all_goals
      have := ih p hp
      omega

/-- A placeholder's push decodes to its width. -/
theorem decode_placeholder {code : List Nat} {pc next bytes : Nat}
    (h : decode code pc = some (.push 0, next)) (b1 : 1 ≤ bytes) (b32 : bytes ≤ 32)
    (op : code[pc]? = some (PUSH1 + bytes - 1)) :
    next = pc + 1 + bytes ∧ pc + 1 + bytes ≤ code.length := by
  unfold decode at h
  rw [op] at h
  simp only [PUSH0, PUSH1, PUSH32] at h
  have n0 : ¬ (96 + bytes - 1 = 95) := by omega
  have r : 96 ≤ 96 + bytes - 1 ∧ 96 + bytes - 1 ≤ 127 := by omega
  have e : 96 + bytes - 1 - 96 + 1 = bytes := by omega
  simp only [n0, ite_false, r, and_self, ite_true, e] at h
  split at h
  · rename_i fits
    simp only [Option.some.injEq, Prod.mk.injEq] at h
    exact ⟨h.2.symm, fits⟩
  · simp at h

/-- Linear decoding of the linked code from `pc` is the walk's stream with each placeholder
pushing its value, when each placeholder is a push of its width, every placeholder of the walk is
linked, and every placeholder linked is before `pc` or one of the walk's. -/
theorem walk_link {code : List Nat} {offset : Nat → Nat} {pc : Nat} {items : List Item}
    {stream : List (Nat × Instr)} (w : Walk code offset pc items stream) (ps : List Placeholder)
    (values : Nat → Nat) :
    placed code stream items = true →
    (∀ p ∈ placeholdersOf stream items, p ∈ ps) →
    (∀ p ∈ ps, p.start + p.bytes ≤ pc ∨ p ∈ placeholdersOf stream items) →
    ∀ fuel, items.length < fuel →
      decodeFrom (link code ps values) fuel pc = linkedStream values stream items := by
  induction w with
  | nil =>
    intro _ _ _ fuel h
    cases fuel with
    | zero => simp at h
    | succ fuel =>
      have : (link code ps values)[code.length]? = none := by
        rw [link_getElem?]; simp
      simp [decodeFrom, decode, this, linkedStream]
  | @cons pc next item items stream decoded labels rest ih =>
    intro hplaced hmem hps fuel h
    cases fuel with
    | zero => simp at h
    | succ fuel =>
    simp only [List.length_cons] at h
    have adv := decode_advances decoded
    have later := placeholdersOf_gt rest
    cases item
    case immutable id bytes =>
      simp only [placed, Bool.and_eq_true, decide_eq_true_eq, beq_iff_eq] at hplaced
      obtain ⟨⟨⟨b1, b32⟩, op⟩, hplaced⟩ := hplaced
      obtain ⟨hnext, fits⟩ := decode_placeholder decoded b1 b32 op
      subst hnext
      simp only [placeholdersOf] at hmem hps
      have here : ∀ p ∈ ps, p.covers pc = false := by
        intro p hp
        rcases hps p hp with before | mine
        · (simp [Placeholder.covers]; try omega)
        · rcases List.mem_cons.1 mine with rfl | tail
          · (simp [Placeholder.covers]; try omega)
          · have := later p tail
            (simp [Placeholder.covers]; try omega)
      have opByte : (link code ps values)[pc]? = some (PUSH1 + bytes - 1) := by
        rw [link_getElem?_of_uncovered here, op]
      have imm : ((link code ps values).drop (pc + 1)).take bytes =
          (List.range bytes).map (beByte (values id) bytes) := by
        apply List.ext_getElem?
        intro j
        simp only [List.getElem?_take, List.getElem?_drop, List.getElem?_map]
        split
        · rename_i hj
          rw [link_getElem?]
          have lt : pc + 1 + j < code.length := by omega
          rw [List.getElem?_eq_getElem lt]
          simp only [Option.map_some]
          have cov : (⟨id, pc + 1, bytes⟩ : Placeholder).covers (pc + 1 + j) = true := by
            (simp [Placeholder.covers]; try omega)
          have find := find?_covers (ps := ps) (i := pc + 1 + j) (p := ⟨id, pc + 1, bytes⟩)
            (hmem _ List.mem_cons_self) cov (by
              intro q hq c
              rcases hps q hq with before | mine
              · simp [Placeholder.covers] at c; omega
              · rcases List.mem_cons.1 mine with e | tail
                · exact e
                · have := later q tail
                  simp [Placeholder.covers] at c; omega)
          simp [linkByte, find, hj]
        · rename_i hj
          simp [hj]
      have d : decode (link code ps values) pc =
          some (.push (values id % 256 ^ bytes), pc + 1 + bytes) := by
        unfold decode
        rw [opByte]
        simp only [PUSH0, PUSH1, PUSH32]
        have n0 : ¬ (96 + bytes - 1 = 95) := by omega
        have r : 96 ≤ 96 + bytes - 1 ∧ 96 + bytes - 1 ≤ 127 := by omega
        have e : 96 + bytes - 1 - 96 + 1 = bytes := by omega
        have fits' : pc + 1 + bytes ≤ (link code ps values).length := by
          rw [link_length]; exact fits
        simp only [n0, ite_false, r, and_self, ite_true, e, fits']
        rw [imm, bigEndian_beBytes]
      simp only [decodeFrom, d, linkedStream]
      rw [ih hplaced (fun p hp => hmem p (List.mem_cons_of_mem _ hp)) (by
        intro p hp
        rcases hps p hp with before | mine
        · left; omega
        · rcases List.mem_cons.1 mine with rfl | tail
          · left; simp
          · right; exact tail) fuel (by omega)]
    all_goals
      simp only [placed] at hplaced
      simp only [placeholdersOf] at hmem hps
      have uncovered : ∀ i, pc ≤ i → i < next → (link code ps values)[i]? = code[i]? := by
        intro i h1 h2
        apply link_getElem?_of_uncovered
        intro p hp
        rcases hps p hp with before | mine
        · (simp [Placeholder.covers]; try omega)
        · have := later p mine
          (simp [Placeholder.covers]; try omega)
      have d := decode_congr (link_length code ps values) uncovered decoded
      simp only [decodeFrom, d, linkedStream]
      rw [ih hplaced hmem (by
        intro p hp
        rcases hps p hp with before | mine
        · left; omega
        · right; exact mine) fuel (by omega)]

/-- Linear decoding of the code linked at the walk's placeholders is the walk's stream with each
placeholder pushing its value. -/
theorem decodes_linked {code : List Nat} {offset : Nat → Nat} {items : List Item}
    {stream : List (Nat × Instr)} (w : Walk code offset 0 items stream)
    (hplaced : placed code stream items = true) (values : Nat → Nat) :
    decodeFrom (link code (placeholdersOf stream items) values) (code.length + 1) 0 =
      linkedStream values stream items :=
  walk_link w _ values hplaced (fun _ hp => hp) (fun _ hp => .inr hp) _
    (by have := walk_length w; omega)

/-- The linked stream's instructions are the program's items, each placeholder linked. -/
theorem linked_instructions {code : List Nat} {offset : Nat → Nat} {pc : Nat} {items : List Item}
    {stream : List (Nat × Instr)} (w : Walk code offset pc items stream) (values : Nat → Nat) :
    (linkedStream values stream items).map Prod.snd = items.map (Item.linked offset values) := by
  induction w with
  | nil => simp [linkedStream]
  | @cons pc next item items stream decoded labels rest ih =>
    clear decoded labels rest
    cases item <;> simp [linkedStream, Item.linked, ih]

end SolarTV.Assembly
