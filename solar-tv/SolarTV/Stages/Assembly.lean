import SolarTV.Sem

/-! The stage from assembly to bytecode, which `-Zvalidate-assembly` checks.

The assembler solves a program of opcodes, pushes of constants and of label offsets, and label
definitions to a layout and emits bytes. `-Zvalidate-assembly` (`validate_assembly.rs`) decodes the
bytes in lockstep with the program: `Walk` is that walk, with the checker's conditions as its
constructors. Each item decodes, at the offset the walk reached, to the instruction it is at the
layout (`Item.expected`): an opcode to its byte, a constant push to a PUSH of the constant, so its
width suffices, a label push to a PUSH of the label's offset, and a label to a `JUMPDEST` at the
label's offset; and the walk ends exactly at the end of the code.

From the walk, `walk_decodes` proves that linear decoding of the bytes from offset 0 is exactly the
program's instructions at their offsets, `jump_targets` that every pushed label is the offset of a
`JUMPDEST` that decoding reaches as an instruction, and `stage` that assembly to bytecode is a
stage for any bytecode semantics that runs the code's linear decoding, against the assembly program
run at the layout the walk found. No axioms.

An immutable's placeholder is an item of its own, a push of zeros of its width, which the
deployment code links with the immutable's value: `SolarTV.Stages.Link` shows that linear
decoding of the linked code is the walk's instructions with each placeholder pushing its value.

Outside this model, as in the checker's limits: program data, library placeholders and packed
label tables, which the checker checks byte for byte against the artifact's records,
and the data flow of the deployment code's copy of the runtime code. The bytecode semantics
running the linear decoding is a hypothesis: legacy EVM code runs instruction by instruction from
offset 0 and jumps only to a `JUMPDEST` that starts an instruction, which is what the decoding
lists. -/

namespace SolarTV.Assembly

/-- An instruction as bytecode decodes it: an opcode, or a push of its immediate's value. -/
inductive Instr where
  | op (byte : Nat)
  | push (value : Nat)
  deriving DecidableEq, Repr

def PUSH0 : Nat := 0x5f
def PUSH1 : Nat := 0x60
def PUSH32 : Nat := 0x7f
def JUMPDEST : Nat := 0x5b

/-- The big-endian value of `bytes`. -/
def bigEndian (bytes : List Nat) : Nat :=
  bytes.foldl (fun value byte => value * 256 + byte) 0

/-- The instruction at `pc` of `code` and the offset after it, or `none` past the end of the
code or for a push whose immediate runs past it, as the checker reads it. -/
def decode (code : List Nat) (pc : Nat) : Option (Instr × Nat) :=
  match code[pc]? with
  | none => none
  | some byte =>
    if byte = PUSH0 then some (.push 0, pc + 1)
    else if PUSH1 ≤ byte ∧ byte ≤ PUSH32 then
      if pc + 1 + (byte - PUSH1 + 1) ≤ code.length then
        some (.push (bigEndian ((code.drop (pc + 1)).take (byte - PUSH1 + 1))),
          pc + 1 + (byte - PUSH1 + 1))
      else none
    else some (.op byte, pc + 1)

/-- Linear decoding of `code` from `pc`, each instruction at its offset, for at most `fuel`
instructions. -/
def decodeFrom (code : List Nat) : Nat → Nat → List (Nat × Instr)
  | 0, _ => []
  | fuel + 1, pc =>
    match decode code pc with
    | some (instr, next) => (pc, instr) :: decodeFrom code fuel next
    | none => []

/-- An instruction of the assembly program: an opcode, a push of a constant or of a label's
offset, a label's definition, or the `PUSH<bytes>` placeholder of immutable `id`, zeros until a
deployment links it. -/
inductive Item where
  | op (byte : Nat)
  | push (value : Nat)
  | pushLabel (label : Nat)
  | label (label : Nat)
  | immutable (id bytes : Nat)
  deriving DecidableEq, Repr

/-- The instruction an item is at a layout that puts each label at `offset`. -/
def Item.expected (offset : Nat → Nat) : Item → Instr
  | .op byte => .op byte
  | .push value => .push value
  | .pushLabel l => .push (offset l)
  | .label _ => .op JUMPDEST
  | .immutable _ _ => .push 0

/-- The checker's walk: decoding `code` from `pc` in lockstep with `items` gives each item's
instruction at the layout `offset`, with each label defined where its `JUMPDEST` is, and ends at
the end of the code. `stream` is the program's instructions at the offsets the walk reached. -/
inductive Walk (code : List Nat) (offset : Nat → Nat) :
    Nat → List Item → List (Nat × Instr) → Prop
  | nil : Walk code offset code.length [] []
  | cons {pc next : Nat} {item : Item} {items : List Item} {stream : List (Nat × Instr)} :
      decode code pc = some (item.expected offset, next) →
      (∀ label, item = .label label → offset label = pc) →
      Walk code offset next items stream →
      Walk code offset pc (item :: items) ((pc, item.expected offset) :: stream)

theorem decode_advances {code : List Nat} {pc next : Nat} {instr : Instr}
    (h : decode code pc = some (instr, next)) : pc < next := by
  unfold decode at h
  split at h
  · simp at h
  · split at h
    · rcases h with ⟨_, rfl⟩
      omega
    · split at h
      · split at h
        · rcases h with ⟨_, rfl⟩
          omega
        · simp at h
      · rcases h with ⟨_, rfl⟩
        omega

/-- Each item takes at least a byte. -/
theorem walk_length {code : List Nat} {offset : Nat → Nat} {pc : Nat} {items : List Item}
    {stream : List (Nat × Instr)} (w : Walk code offset pc items stream) :
    pc + items.length ≤ code.length := by
  induction w with
  | nil => simp
  | cons decoded _ _ ih =>
    have := decode_advances decoded
    simp only [List.length_cons]
    omega

/-- Linear decoding is the program's instructions at their offsets. -/
theorem walk_decodes {code : List Nat} {offset : Nat → Nat} {pc : Nat} {items : List Item}
    {stream : List (Nat × Instr)} (w : Walk code offset pc items stream) :
    ∀ fuel, items.length < fuel → decodeFrom code fuel pc = stream := by
  induction w with
  | nil =>
    intro fuel h
    cases fuel with
    | zero => simp at h
    | succ fuel =>
      simp [decodeFrom, decode]
  | cons decoded _ _ ih =>
    intro fuel h
    cases fuel with
    | zero => simp at h
    | succ fuel =>
      simp only [List.length_cons] at h
      simp only [decodeFrom, decoded]
      rw [ih fuel (by omega)]

/-- The decoded instructions are the program's items at the layout. -/
theorem walk_instructions {code : List Nat} {offset : Nat → Nat} {pc : Nat} {items : List Item}
    {stream : List (Nat × Instr)} (w : Walk code offset pc items stream) :
    stream.map Prod.snd = items.map (Item.expected offset) := by
  induction w with
  | nil => rfl
  | cons _ _ _ ih => simp [ih]

/-- A defined label's offset holds a `JUMPDEST` that starts an instruction. -/
theorem walk_label {code : List Nat} {offset : Nat → Nat} {pc : Nat} {items : List Item}
    {stream : List (Nat × Instr)} (w : Walk code offset pc items stream) {label : Nat}
    (defined : Item.label label ∈ items) : (offset label, Instr.op JUMPDEST) ∈ stream := by
  induction w with
  | nil => simp at defined
  | @cons pc next item items stream _ labels _ ih =>
    rcases List.mem_cons.1 defined with here | later
    · subst here
      rw [labels label rfl]
      simp [Item.expected]
    · exact List.mem_cons_of_mem _ (ih later)

/-- Every label the program pushes and defines is the offset of a `JUMPDEST` that the code's
linear decoding reaches as an instruction. -/
theorem jump_targets {code : List Nat} {offset : Nat → Nat} {items : List Item}
    {stream : List (Nat × Instr)} (w : Walk code offset 0 items stream) {label : Nat}
    (defined : Item.label label ∈ items) :
    (offset label, Instr.op JUMPDEST) ∈ decodeFrom code (code.length + 1) 0 := by
  have := walk_length w
  rw [walk_decodes w _ (by omega)]
  exact walk_label w defined

/-- Assembly to bytecode is a stage: the program `p`, run at the layout the walk found, and the
code, run by a bytecode semantics that runs its linear decoding, give the same result. -/
theorem stage {A B : Sem} {p : A.Prog} {code : B.Prog} {scope : Call → Prop}
    (bytes : List Nat) (items : List Item) (offset : Nat → Nat) (stream : List (Nat × Instr))
    (run : List (Nat × Instr) → Call → Option Observable)
    (assembly : ∀ call, A.run p call = run stream call)
    (bytecode : ∀ call, B.run code call = run (decodeFrom bytes (bytes.length + 1) 0) call)
    (checked : Walk bytes offset 0 items stream) : Stage A B p code scope := by
  intro call _ obs ran
  have := walk_length checked
  rw [bytecode, walk_decodes checked _ (by omega), ← assembly]
  exact ran

/-! A jump over a label push: `PUSH1 3; JUMP; JUMPDEST; STOP`, with label 0 at offset 3. -/

def example_items : List Item := [.pushLabel 0, .op 0x56, .label 0, .op 0x00]

def example_code : List Nat := [0x60, 3, 0x56, 0x5b, 0x00]

theorem example_walk :
    Walk example_code (fun _ => 3) 0 example_items
      [(0, .push 3), (2, .op 0x56), (3, .op 0x5b), (4, .op 0x00)] :=
  .cons (by rfl) (by intro _ h; cases h)
    (.cons (by rfl) (by intro _ h; cases h)
      (.cons (by rfl) (by intro _ _; rfl)
        (.cons (by rfl) (by intro _ h; cases h) .nil)))

/-- Off by one, the label push misses the label: no walk exists, as the checker reports. -/
theorem example_fault (stream : List (Nat × Instr)) :
    ¬ Walk [0x60, 4, 0x56, 0x5b, 0x00] (fun _ => 3) 0 example_items stream := by
  intro w
  cases w with
  | cons decoded _ _ =>
    simp [decode, bigEndian, Item.expected, PUSH0, PUSH1, PUSH32] at decoded

end SolarTV.Assembly
