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
these items. -/

namespace SolarTV.Milestone.W1

open SolarTV.Assembly

/-- The opcode of a stack operation, or none for one these items do not spell: EXCHANGE, whose
immediate operand is no item. -/
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
  | .dup index => some (.op (0x80 + index.val))
  | .swap index => some (.op (0x90 + index.val))
  | .exchange _ _ => none

/-- The opcode of a memory instruction, or none for one these items do not spell: the exported
programs copy and hash memory with effects (`.effect .mcopy`, `.effect .keccak256`), never with
these operations. -/
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
  -- NOTE: assembly lowers a jump table to a packed table of labels, outside these items.
  | .jumpTable _ => none
  | .stop => some [.op 0x00]
  | .ret => some [.op 0xf3]
  | .revert => some [.op 0xfd]
  | .invalid => some [.op 0xfe]

/-- The items of a list of instructions. -/
def instrsItems : List EVM.Program.Instr → Option (List Item)
  | [] => some []
  | instr :: rest => do
    let head ← instrItems instr
    let tail ← instrsItems rest
    pure (head ++ tail)

/-- The items of a block, given the block that follows it in the layout (none for the last
block): a final transfer to `next` falls through instead of jumping, and the last block's final
stop is the end of the code. -/
def blockItems (next : Option Nat) : List EVM.Program.Instr → Option (List Item)
  | [] => some []
  | [.stop] =>
    -- the last block's stop: the code ends
    if next = none then some []
    -- STOP
    else some [.op 0x00]
  | [.jump target] =>
    -- jump next
    if next = some target then some []
    -- push target; JUMP
    else some [.pushLabel target, .op 0x56]
  | [.jumpi yes, .jump no] =>
    -- push yes; JUMPI
    if next = some no then some [.pushLabel yes, .op 0x57]
    -- ISZERO; push no; JUMPI
    else if next = some yes then some [.op 0x15, .pushLabel no, .op 0x57]
    -- push yes; JUMPI; push no; JUMP
    else some [.pushLabel yes, .op 0x57, .pushLabel no, .op 0x56]
  | instr :: rest => do
    let head ← instrItems instr
    let tail ← blockItems next rest
    pure (head ++ tail)

/-- The blocks' items in layout order from block `index`, each with the block after it. -/
def bodiesOf : Nat → List (List EVM.Program.Instr) → Option (List (List Item))
  | _, [] => some []
  | index, block :: rest => do
    let body ← blockItems (if rest.isEmpty then none else some (index + 1)) block
    let tail ← bodiesOf (index + 1) rest
    pure (body :: tail)

/-- The labels some item pushes. -/
def pushedLabels (bodies : List (List Item)) : List Nat :=
  bodies.flatMap fun body => body.filterMap fun
    | .pushLabel label => some label
    | _ => none

/-- Each body from block `index` on, opening with its label when some item pushes it. -/
def labelBodies (pushed : List Nat) : Nat → List (List Item) → List Item
  | _, [] => []
  | index, body :: rest =>
    (if index ∈ pushed then [.label index] else []) ++ body ++ labelBodies pushed (index + 1) rest

/-- The program as assembly items in layout order, each block some item jumps to opening with
its label. -/
def itemsOf (p : EVM.Program.Program) : Option (List Item) := do
  let bodies ← bodiesOf 0 p.blocks
  pure (labelBodies (pushedLabels bodies) 0 bodies)

end SolarTV.Milestone.W1
