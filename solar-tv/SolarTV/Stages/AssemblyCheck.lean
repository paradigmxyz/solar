import SolarTV.Stages.Assembly

/-! The checker's walk as a function, which the kernel evaluates.

`walk` runs `-Zvalidate-assembly`'s lockstep decoding (`Walk`) on concrete bytes and
items: it gives the stream of instructions the walk reaches, or `none` where an item does not
decode to its instruction at the layout, an item is not placed as the layout says, or the walk
does not end exactly at the end of the code. `walk_sound` and `walk_complete` show it is
`Walk`, so a kernel-checked `walk … = some stream` proves the walk, and `walk … = none`
refutes every walk (`no_walk`), as the checker rejects the code. -/

namespace SolarTV.Assembly

/-- The checker's walk from `pc`: the stream of instructions it reaches, or `none`. -/
def walk (code : List Nat) (offset : Nat → Nat) : Nat → List Item → Option (List (Nat × Instr))
  | pc, [] => if pc = code.length then some [] else none
  | pc, item :: items =>
    match decode code pc with
    | none => none
    | some (instr, next) =>
      if instr = item.expected offset ∧ item.placed offset pc = true then
        match walk code offset next items with
        | some rest => some ((pc, instr) :: rest)
        | none => none
      else none

theorem walk_sound {code : List Nat} {offset : Nat → Nat} :
    ∀ {pc : Nat} {items : List Item} {stream : List (Nat × Instr)},
      walk code offset pc items = some stream → Walk code offset pc items stream
  | pc, [], stream, h => by
    simp only [walk] at h
    by_cases e : pc = code.length
    · simp only [e, ite_true] at h
      cases h
      rw [e]
      exact .nil
    · simp only [e, ite_false] at h
      cases h
  | pc, item :: items, stream, h => by
    simp only [walk] at h
    cases hd : decode code pc with
    | none => rw [hd] at h; cases h
    | some r =>
      obtain ⟨instr, next⟩ := r
      rw [hd] at h
      simp only at h
      by_cases ok : instr = item.expected offset ∧ item.placed offset pc = true
      · simp only [ok, and_self, ite_true] at h
        cases hw : walk code offset next items with
        | none => rw [hw] at h; cases h
        | some rest =>
          rw [hw] at h
          cases h
          rw [ok.1] at hd
          exact .cons hd ok.2 (walk_sound hw)
      · simp only [ok, ite_false] at h
        cases h

theorem walk_complete {code : List Nat} {offset : Nat → Nat} {pc : Nat} {items : List Item}
    {stream : List (Nat × Instr)} (w : Walk code offset pc items stream) :
    walk code offset pc items = some stream := by
  induction w with
  | nil => simp [walk]
  | cons decoded labels _ ih =>
    simp [walk, decoded, labels, ih]

/-- A walk the function refutes does not exist. -/
theorem no_walk {code : List Nat} {offset : Nat → Nat} {pc : Nat} {items : List Item}
    (h : walk code offset pc items = none) (stream : List (Nat × Instr)) :
    ¬ Walk code offset pc items stream := fun w => by
  rw [walk_complete w] at h
  cases h

end SolarTV.Assembly
