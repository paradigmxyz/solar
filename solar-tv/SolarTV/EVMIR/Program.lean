import SolarTV.Machine
import SolarTV.EVMIR.Memory

/-! Scheduled EVM IR programs as a layer.

A program is a list of blocks of instructions over one stack, run with the checked stack
operations and memory instructions of [Memory.lean](Memory.lean) (`SolarTV.EVM.Memory.Op`)
through `stepOp`, which runs them on `Machine.Mem`; `stepOp_project` shows it is
`SolarTV.EVM.Memory.step` on the projection. Effects pop their operands, top first, and
push their result. Static transfers are fused with their label push: `jump b` and
`jumpi b` (the condition on top) need room for the label. A `jumpi` not taken goes on
with the rest of its block, as EVM IR's `push bbN; jumpi` does in the middle of a block,
and at the end of a block falls through to the next one. Running off the end of a block
enters the next one, and past the last block the program stops, as the EVM does past its
code. Block numbers are EVM IR's `BlockId`s, its layout order, as `-Zemit-lean-stages`
writes them. An internal call pushes the label of the block it returns to as a value
(`pushLabel`), and the callee returns with a dynamic `jump` (`jumpDyn`) to the label on top
of the stack. A conditional jump whose label is not pushed right before it (`jumpiDyn`), as
when blocks that share their tail jump to labels their predecessors pushed, takes the label on
top of the stack and the condition below it. A jump table (`jumpTable`) jumps to the block its
index, on top of the stack, numbers; EVM IR requires the index to be in range, and here an index
past the table halts. The label of a block is its number here, where the bytecode has its offset; no
observable shows a label. A dynamic jump to a word that numbers no block is an exceptional
halt, as a jump to a byte that is no `JUMPDEST`. A stack failure is an exceptional
halt, which observes as `invalid`. Each block costs one unit of fuel. -/

namespace SolarTV.EVM.Program

open EvmRules Machine

inductive Instr where
  | op (op : EVM.Memory.Op)
  | effect (effect : Effect) (arity : Nat)
  | jump (target : Nat)
  | jumpi (target : Nat)
  /-- The label of a block as a value. -/
  | pushLabel (target : Nat)
  /-- A jump to the label on top of the stack. -/
  | jumpDyn
  /-- A jump to the label on top of the stack when the word below it is nonzero. -/
  | jumpiDyn
  /-- A jump to the block the index on top of the stack numbers in the table (`indexed_jump`);
  an index past the table is an exceptional halt. -/
  | jumpTable (targets : List Nat)
  | stop | ret | revert | invalid

structure Program where
  blocks : List (List Instr)

/-- `SolarTV.EVM.Memory.step` on memory of every byte. -/
def stepOp (ctx : Context) (op : EVM.Memory.Op) (stack : Stack) (m : Mem) :
    Except Failure (Stack × Mem) :=
  if stack.length > 1024 then .error .overflow
  else match op, stack with
    | .stack op, stack => (stepChecked ctx op stack).map (fun next => (next, m))
    | .load, offset :: rest => .ok (m.load offset.toNat :: rest, m.expand offset.toNat 32)
    | .store, offset :: value :: rest => .ok (rest, m.store offset.toNat value)
    | .storeByte, offset :: value :: rest => .ok (rest, m.storeByte offset.toNat value)
    | .size, stack =>
      if stack.length + 1 > 1024 then .error .overflow else .ok (m.msize :: stack, m)
    | _, _ => .error .underflow

/-- A load reads inside the 256-byte projection. `stepOp` does not run copies or hashes
on `Machine.Mem`, so the projection covers neither. -/
def InBounds : EVM.Memory.Op → Stack → Prop
  | .load, offset :: _ => offset.toNat + 32 ≤ 256
  | .copy, _ | .hash, _ => False
  | _, _ => True

theorem stepOp_project (ctx : Context) (op : EVM.Memory.Op) (stack : Stack) (m : Mem)
    (bounded : InBounds op stack) :
    (stepOp ctx op stack m).map (fun next => (next.1, next.2.project)) =
      EVM.Memory.step ctx op stack m.project := by
  unfold stepOp EVM.Memory.step
  split
  · rfl
  · cases op with
    | stack op =>
      show Except.map _ (Except.map _ (stepChecked ctx op stack)) =
        Except.map _ (stepChecked ctx op stack)
      cases stepChecked ctx op stack <;> rfl
    | load =>
      cases stack with
      | nil => rfl
      | cons offset rest =>
        simp only [InBounds] at bounded
        simp only [Except.map, Mem.project_load m _ bounded, Mem.project_expand]
    | store =>
      match stack with
      | [] | [_] => rfl
      | offset :: value :: rest => simp only [Except.map, Mem.project_store]
    | storeByte =>
      match stack with
      | [] | [_] => rfl
      | offset :: value :: rest => simp only [Except.map, Mem.project_storeByte]
    | size =>
      by_cases h : stack.length + 1 > 1024 <;> simp [h, Except.map, Mem.project_msize]
    | copy => exact absurd bounded (by cases stack <;> simp [InBounds])
    | hash => exact absurd bounded (by cases stack <;> simp [InBounds])

def context (o : Oracle) (call : Call) (s : State) : Context :=
  ⟨BitVec.ofNat 256 call.calldata.size, o.env "returndatasize" s.world []⟩

def output (s : State) (stack : Stack) (reverted : Bool) : Halt × State :=
  match stack with
  | offset :: size :: _ =>
    let memory := s.memory.expand offset.toNat size.toNat
    let data := memory.read offset.toNat size.toNat
    ((if reverted then .revert data else .ret data), { s with memory })
  | _ => (.invalid, s)

/-- A block's instructions from block `b`: its end, or the next block and its stack. -/
def exec (o : Oracle) (call : Call) (b : Nat) :
    List Instr → Stack → State → (Halt × State) ⊕ (Nat × Stack × State)
  | [], stack, s => .inr (b + 1, stack, s)
  | .op op :: rest, stack, s =>
    match stepOp (context o call s) op stack s.memory with
    | .error _ => .inl (.invalid, s)
    | .ok (stack, memory) => exec o call b rest stack { s with memory }
  | .effect effect arity :: rest, stack, s =>
    if stack.length < arity then .inl (.invalid, s)
    else match effect.apply o call (stack.take arity) s with
      | none => .inl (.invalid, s)
      | some (result, s) =>
        let stack := match result with
          | some value => value :: stack.drop arity
          | none => stack.drop arity
        if stack.length > 1024 then .inl (.invalid, s) else exec o call b rest stack s
  | .jump target :: _, stack, s =>
    if stack.length + 1 > 1024 then .inl (.invalid, s) else .inr (target, stack, s)
  | .jumpi target :: rest, stack, s =>
    if stack.length + 1 > 1024 then .inl (.invalid, s)
    else match stack with
      | condition :: stack =>
        if condition = 0 then exec o call b rest stack s else .inr (target, stack, s)
      | [] => .inl (.invalid, s)
  | .pushLabel target :: rest, stack, s =>
    if stack.length + 1 > 1024 then .inl (.invalid, s)
    else exec o call b rest (BitVec.ofNat 256 target :: stack) s
  | .jumpDyn :: _, stack, s =>
    match stack with
    | target :: stack => .inr (target.toNat, stack, s)
    | [] => .inl (.invalid, s)
  | .jumpiDyn :: rest, stack, s =>
    match stack with
    | target :: condition :: stack =>
      if condition = 0 then exec o call b rest stack s else .inr (target.toNat, stack, s)
    | _ => .inl (.invalid, s)
  | .jumpTable targets :: _, stack, s =>
    match stack with
    | index :: stack =>
      match targets[index.toNat]? with
      | some target => .inr (target, stack, s)
      | none => .inl (.invalid, s)
    | [] => .inl (.invalid, s)
  | .stop :: _, _, s => .inl (.stop, s)
  | .ret :: _, stack, s => .inl (output s stack false)
  | .revert :: _, stack, s => .inl (output s stack true)
  | .invalid :: _, _, s => .inl (.invalid, s)

def block (o : Oracle) (call : Call) (p : Program) :
    Nat × Stack × State → Option ((Halt × State) ⊕ (Nat × Stack × State))
  | (b, stack, s) =>
    match p.blocks[b]? with
    -- NOTE: running off the last block reaches the number past it, where the program stops;
    -- a dynamic jump to that number stops too, where the bytecode's jump to its end fails.
    -- Labels only name blocks, so no exported program jumps there.
    | none => some (.inl (if b = p.blocks.length then .stop else .invalid, s))
    | some instrs => some (exec o call b instrs stack s)

def run (o : Oracle) (p : Program) (call : Call) (fuel : Nat) : Option (Halt × State) :=
  iterate (block o call p) fuel (0, [], State.initial call)

def layer : Layer where
  Prog := Program
  run := run
  mono _ _ _ n r := iterate_mono _ n _ r

/-- The fuel-free semantics of scheduled EVM IR under an oracle. -/
noncomputable def sem (o : Oracle) : Sem := layer.sem o

theorem sem_failure (o : Oracle) (p : Program) (call : Call) (obs : Observable)
    (ran : (sem o).run p call = some obs) (failed : obs.success = false) :
    obs.logs = [] ∧ obs.storage = slotValue call.storage := by
  obtain ⟨_, _, logs, storage, _⟩ := layer.sem_failure o p call obs ran failed
  exact ⟨logs, storage⟩

theorem failureConvention (o : Oracle) : (sem o).FailureConvention := layer.failureConvention o

/-- A program of one block that halts runs it at fuel one. -/
theorem run_straight (o : Oracle) (call : Call) (instrs : List Instr) (n : Nat)
    (result : Halt × State) (halts : exec o call 0 instrs [] (State.initial call) = .inl result) :
    run o ⟨[instrs]⟩ call (n + 1) = some result := by
  simp only [run, iterate, block, List.getElem?_cons_zero, halts]

end SolarTV.EVM.Program
