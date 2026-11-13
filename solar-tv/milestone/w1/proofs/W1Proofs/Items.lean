import SolarTV.Stages.AssemblyCheck
import SolarTV.EVMIR.Program

/-! EVM IR programs as the assembly items the assembler solves.

`itemsOf` spells an EVM IR program as the assembly the assembler solves: each instruction as
its opcode, a push as a push of its constant, a static jump as a push of its target's label
and the jump, a pushed label as its push, a dynamic jump as `JUMP`, and a `JUMPDEST` label
at the start of each block some pushed label targets. A
block's transfer to the block after it is spelled as `lower_terminator` in
`crates/codegen/src/backend/assembler/assembly/lower.rs` spells it: a final `jump` to the next
block is no instruction, and of a final `jumpi then; jump else` that falls through, only the
`JUMPI` to the other block stays, after an `ISZERO` when `then` is the one that follows. The
EVM IR semantics enters the next block past the end of a block, so the spelling runs the same
blocks. The last block's final `stop` is no instruction either, as `lower_terminator` leaves
it out of runtime code that nothing follows: the code ends there, which halts as STOP does. It
is partial: an effect outside the EVM's opcodes (an oracle's environment read or
call) or with another arity than its opcode's has no item, so a program outside that scope
fails the check. Every contract's `Assembly` module (w1's and the ones
`milestone/certify.py` writes) evaluates `SolarTV.Assembly.walk` over its bytecode against
these items.

A jump table (`jumpTable`) is spelled as `indexed_jump.rs` lowers it, in the encoding the
bytecode shows (`Spelling`), which `itemsWith` takes for each table in layout order: packed,
a word of one-byte offsets, the first target's in its high byte, that `BYTE` picks the entry the
index numbers from, after adding the zero bytes above it (`PUSH 32-n; ADD; PUSHn table; SWAP1;
BYTE; JUMP`); or a stride table, `PUSH stride; MUL; PUSH entry0; ADD; JUMP` into entry stubs
`JUMPDEST; PUSH target; JUMP` laid out after the last block, `stride` bytes apart, each named by
a label past every block's. The walk checks that each packed offset fits its byte and that each
stub sits its index of strides past the first, so that the pick lands on the entry's target. With
stubs after it, the last block's final `stop` is a `STOP` again. The other encodings, relative
to a base or packed wider than a byte (`-Zpack-wide-jump-tables`, the size objective), have no
spelling, so the check refuses them, as it does a table with no spelling given. EVM IR requires a
table's index to be in range, as every lowering of a switch checks before the jump; out of range
the EVM IR semantics halts, while the bytecode jumps wherever the pick lands. -/

namespace SolarTV.Milestone.W1

open SolarTV.Assembly

/-- The opcode of a stack operation, or `none` for one without a one-byte spelling here: a
`dup` or `swap` past the sixteenth word, and `exchange`. -/
def stackItem : EVM.Op → Option Item
  | .add => some (.op 0x01) | .mul => some (.op 0x02) | .sub => some (.op 0x03)
  | .div => some (.op 0x04) | .sdiv => some (.op 0x05) | .mod => some (.op 0x06)
  | .smod => some (.op 0x07) | .addmod => some (.op 0x08) | .mulmod => some (.op 0x09)
  | .exp => some (.op 0x0a) | .signextend => some (.op 0x0b)
  | .lt => some (.op 0x10) | .gt => some (.op 0x11) | .slt => some (.op 0x12)
  | .sgt => some (.op 0x13) | .eq => some (.op 0x14) | .iszero => some (.op 0x15)
  | .and => some (.op 0x16) | .or => some (.op 0x17) | .xor => some (.op 0x18)
  | .not => some (.op 0x19) | .byte => some (.op 0x1a) | .shl => some (.op 0x1b)
  | .shr => some (.op 0x1c) | .sar => some (.op 0x1d) | .clz => some (.op 0x1e)
  | .calldatasize => some (.op 0x36) | .returndatasize => some (.op 0x3d)
  | .pop => some (.op 0x50)
  | .push value => some (.push value.toNat)
  | .dup index => if index.val < 16 then some (.op (0x80 + index.val)) else none
  | .swap index => if index.val < 16 then some (.op (0x90 + index.val)) else none
  | .exchange _ _ => none

/-- The opcode of a memory instruction, or `none` for a copy or a hash, which `stepOp` does
not run. -/
def memoryItem : EVM.Memory.Op → Option Item
  | .stack op => stackItem op
  | .load => some (.op 0x51)
  | .store => some (.op 0x52)
  | .storeByte => some (.op 0x53)
  | .size => some (.op 0x59)
  | .copy | .hash => none

/-- The opcode of an effect with its arity, or `none` for one that is no opcode. -/
def effectItem : Machine.Effect → Nat → Option Item
  | .address, 0 => some (.op 0x30)
  | .caller, 0 => some (.op 0x33)
  | .callvalue, 0 => some (.op 0x34)
  | .calldataload, 1 => some (.op 0x35)
  | .calldatasize, 0 => some (.op 0x36)
  | .returndatasize, 0 => some (.op 0x3d)
  | .mload, 1 => some (.op 0x51)
  | .mstore, 2 => some (.op 0x52)
  | .mstore8, 2 => some (.op 0x53)
  | .sload, 1 => some (.op 0x54)
  | .sstore, 2 => some (.op 0x55)
  | .msize, 0 => some (.op 0x59)
  | .tload, 1 => some (.op 0x5c)
  | .tstore, 2 => some (.op 0x5d)
  | .keccak256, 2 => some (.op 0x20)
  | .calldatacopy, 3 => some (.op 0x37)
  | .returndatacopy, 3 => some (.op 0x3e)
  | .gas, 0 => some (.op 0x5a)
  | .mcopy, 3 => some (.op 0x5e)
  | .call .call, 7 => some (.op 0xf1)
  | .call .callcode, 7 => some (.op 0xf2)
  | .call .delegatecall, 6 => some (.op 0xf4)
  | .call .staticcall, 6 => some (.op 0xfa)
  -- NOTE: the runtime code's placeholder, a PUSH32 of zeros that the manifest lists, which the
  -- deployment code fills with the immutable.
  | .immutable _, 0 => some (.push 0)
  | .codecopy, 3 => some (.op 0x39)
  | .create, 3 => some (.op 0xf0)
  | .create2, 4 => some (.op 0xf5)
  | .log n, arity => if n ≤ 4 ∧ arity = n + 2 then some (.op (0xa0 + n)) else none
  | _, _ => none

/-- The items of an instruction. -/
def instrItems : EVM.Program.Instr → Option (List Item)
  | .op op => (memoryItem op).map ([·])
  | .effect effect arity => (effectItem effect arity).map ([·])
  | .jump target => some [.pushLabel target, .op 0x56]
  | .jumpi target => some [.pushLabel target, .op 0x57]
  | .pushLabel target => some [.pushLabel target]
  | .jumpDyn => some [.op 0x56]
  | .jumpiDyn => some [.op 0x57]
  -- NOTE: a jump table's items depend on its encoding, which `tableItems` spells.
  | .jumpTable _ => none
  | .stop => some [.op 0x00]
  | .ret => some [.op 0xf3]
  | .revert => some [.op 0xfd]
  | .invalid => some [.op 0xfe]

/-- How the assembler encoded a jump table, as the bytecode shows: `packed`, one-byte offsets
in a word that `BYTE` picks from, or `stride`, entry stubs whose target pushes are `width`
bytes wide. -/
inductive Spelling where
  | packed
  | stride (width : Nat)
  deriving DecidableEq, Repr

def Spelling.isStride : Spelling → Bool
  | .stride _ => true
  | .packed => false

/-- The jump tables' spellings not yet used, the label of the next stride table's first entry,
and the entry stubs of the stride tables so far, in layout order. -/
structure Tables where
  spellings : List Spelling
  fresh : Nat
  stubs : List Item

/-- The entry stubs of a stride table from entry `index`: `JUMPDEST; PUSH target; JUMP`. -/
def entries (table stride : Nat) : Nat → List Nat → List Item
  | _, [] => []
  | index, target :: rest =>
    [.entry table index stride, .pushLabel target, .op 0x56] ++ entries table stride (index + 1) rest

/-- The items of a jump table in the next spelling, as `indexed_jump::lower` writes it. -/
def tableItems (targets : List Nat) (t : Tables) : Option (List Item × Tables) :=
  match t.spellings with
  | [] => none
  | .packed :: rest =>
    if 2 ≤ targets.length ∧ targets.length ≤ 32 then
      -- PUSH 32-n; ADD (when n < 32); PUSHn table; SWAP1; BYTE; JUMP
      some ((if targets.length < 32 then [.push (32 - targets.length), .op 0x01] else [])
          ++ [.pushTable targets, .op 0x90, .op 0x1a, .op 0x56], { t with spellings := rest })
    else none
  | .stride width :: rest =>
    if 1 ≤ width ∧ width ≤ 32 ∧ targets ≠ [] then
      -- PUSH stride; MUL; PUSH entry0; ADD; JUMP
      some ([.push (width + 3), .op 0x02, .pushLabel t.fresh, .op 0x01, .op 0x56],
        { spellings := rest, fresh := t.fresh + 1,
          stubs := t.stubs ++ entries t.fresh (width + 3) 0 targets })
    else none

/-- The items of an instruction, a jump table in the next spelling. -/
def instrTableItems (t : Tables) : EVM.Program.Instr → Option (List Item × Tables)
  | .jumpTable targets => tableItems targets t
  | instr => (instrItems instr).map (·, t)

/-- The items of a list of instructions. -/
def instrsItems : List EVM.Program.Instr → Option (List Item)
  | [] => some []
  | instr :: rest => do
    let head ← instrItems instr
    let tail ← instrsItems rest
    pure (head ++ tail)

/-- The items of a block, given the block that follows it in the layout (none for the last
block) and whether the code ends after it: a final transfer to `next` falls through instead of
jumping, and a final stop where the code ends is the end of the code. -/
def blockItems (next : Option Nat) (ends : Bool) (t : Tables) :
    List EVM.Program.Instr → Option (List Item × Tables)
  | [] => some ([], t)
  | [.stop] =>
    -- the last block's stop: the code ends
    if ends then some ([], t)
    -- STOP
    else some ([.op 0x00], t)
  | [.jump target] =>
    -- jump next
    if next = some target then some ([], t)
    -- push target; JUMP
    else some ([.pushLabel target, .op 0x56], t)
  | [.jumpi yes, .jump no] =>
    -- push yes; JUMPI
    if next = some no then some ([.pushLabel yes, .op 0x57], t)
    -- ISZERO; push no; JUMPI
    else if next = some yes then some ([.op 0x15, .pushLabel no, .op 0x57], t)
    -- push yes; JUMPI; push no; JUMP
    else some ([.pushLabel yes, .op 0x57, .pushLabel no, .op 0x56], t)
  | instr :: rest => do
    let (head, t) ← instrTableItems t instr
    let (tail, t) ← blockItems next ends t rest
    pure (head ++ tail, t)

/-- The blocks' items in layout order from block `index`, each with the block after it; the code
ends after the last block when `ends`. -/
def bodiesOf (ends : Bool) : Nat → Tables → List (List EVM.Program.Instr) →
    Option (List (List Item) × Tables)
  | _, t, [] => some ([], t)
  | index, t, block :: rest => do
    let (body, t) ←
      blockItems (if rest.isEmpty then none else some (index + 1)) (ends && rest.isEmpty) t block
    let (tail, t) ← bodiesOf ends (index + 1) t rest
    pure (body :: tail, t)

/-- The labels some item pushes, a packed table's each. -/
def pushedLabels (bodies : List (List Item)) : List Nat :=
  bodies.flatMap fun body => body.flatMap fun
    | .pushLabel label => [label]
    | .pushTable labels => labels
    | _ => []

/-- Each body from block `index` on, opening with its label when some item pushes it. -/
def labelBodies (pushed : List Nat) : Nat → List (List Item) → List Item
  | _, [] => []
  | index, body :: rest =>
    (if index ∈ pushed then [.label index] else []) ++ body ++ labelBodies pushed (index + 1) rest

/-- The program as assembly items in layout order, each block some item jumps to opening with
its label, its jump tables in `spellings`, one each in layout order, and the stride tables'
entry stubs after the last block. -/
def itemsWith (spellings : List Spelling) (p : EVM.Program.Program) : Option (List Item) := do
  let (bodies, t) ←
    bodiesOf (!spellings.any Spelling.isStride) 0 ⟨spellings, p.blocks.length, []⟩ p.blocks
  if t.spellings.isEmpty then
    pure (labelBodies (pushedLabels (bodies ++ [t.stubs])) 0 bodies ++ t.stubs)
  else none

/-- The program as assembly items, without jump tables. -/
def itemsOf (p : EVM.Program.Program) : Option (List Item) := itemsWith [] p

/-! Refusals: a jump table without its encoding has no items, and a packed table whose offsets do
not fit their bytes is placed nowhere, so no walk accepts code for it (`SolarTV.Assembly.no_walk`). -/

theorem table_unspelled : itemsOf ⟨[[.jumpTable [1, 1]], [.stop]]⟩ = none := by decide

/-- A packed table: the bytes above its two entries added to the index, then the BYTE pick. -/
theorem table_spelled :
    itemsWith [.packed] ⟨[[.jumpTable [1, 1]], [.stop]]⟩ =
      some [.push 30, .op 0x01, .pushTable [1, 1], .op 0x90, .op 0x1a, .op 0x56, .label 1] := by
  decide

/-- A stride table: its entry stubs after the last block, whose final stop is a STOP again. -/
theorem table_stride :
    itemsWith [.stride 2] ⟨[[.jumpTable [1, 1]], [.stop]]⟩ =
      some [.push 5, .op 0x02, .pushLabel 2, .op 0x01, .op 0x56, .label 1, .op 0x00,
        .entry 2 0 5, .pushLabel 1, .op 0x56, .entry 2 1 5, .pushLabel 1, .op 0x56] := by
  decide

/-- A table of one entry is never packed. -/
theorem table_too_few : itemsWith [.packed] ⟨[[.jumpTable [1]], [.stop]]⟩ = none := by decide

/-- An offset of 256 does not fit its byte. -/
theorem table_wide (pc : Nat) : (Item.pushTable [1]).placed (fun _ => 256) pc = false := rfl

end SolarTV.Milestone.W1
