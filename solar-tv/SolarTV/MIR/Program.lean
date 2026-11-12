import SolarTV.Machine
import SolarTV.EVMIR.Ops

/-! Lowered MIR functions and modules as layers.

This is MIR at the `lowered` phase, the input of `-Zvalidate-passes`: word SSA values,
phis, branches, switches, the pure EVM operations, `ne`, effects, facts (`assume`), tail
calls and the halts of a contract entry. `lower-abi` and `lower-dispatch` run before it, so the semantic-phase
pipeline that builds the dispatcher is outside this semantics. A function runs block by
block with one unit of fuel per block, from block 0 with every value zero. A phi reads the
incoming environment and selects by predecessor. Pure operations are the word semantics of
`EVM.step`, the same `EvmRules.Evm` definitions the agreement theorems relate to the
encoder, and `ne` is `Evm.ne`. A switch enters the first case whose operand equals its key,
else its default. Malformed code (an unknown block, a phi without the predecessor, an
effect of the wrong arity) has no end, so it has no observable result, and neither has a
run that reaches a fact that does not hold. Casts and `select` have the semantics the
operation schema declares (`crates/mir/src/semantics.rs`): a zero extension or a pointer cast
leaves the word as it is, since every `iN` value keeps the bits above N zero, a truncation
keeps the low bits (`lowBits`), and a select picks by a nonzero condition.

Two words of memory layout are the backend's, which MIR does not show and the MIR interpreter
reads from the backend memory records: the free memory pointer a runtime entry stores in the
pointer's slot `0x40` when it starts (`Function.entryHeap`, stored on entering the function,
`Function.start`), and the heap floor every `heap_floor` of a function reads, the initial
pointer of the entries that run it (`Inst.heapFloor`). `-Zemit-lean-stages` writes the words
codegen placed, so a stage holds for that placement. So are the allocations (`alloc`) codegen
placed: at a fixed address, or by bumping the free memory pointer by a size, which loads the
pointer and stores it raised as codegen's `mload 0x40; dup1; push size; add; push 0x40; mstore`
does.

A module is the functions of a contract and the one its runtime code enters
(`Module.run`). A `tail_call` enters the callee's block 0 with a fresh environment that
binds its parameters to the arguments, and keeps the machine state; control never comes
back. An internal call (`icall`) does the same but first pushes a `Return`: the rest of its
block and the caller's values. The callee's `ret` (`leave` here) pops it, binds the call's
result to the returned value and runs the rest of the caller's block, for one more unit of
fuel. A `leave` with no caller, or whose values do not match the call's result, has no end.
A function that returns several words leaves all of them, as stack scheduling returns them on
the stack, and the call binds each to the value that reads it (`bindResults`), where MIR reads
the further ones from the multi-return buffer; those reads are not in the exported block. When
codegen also publishes them to the buffer, the call stores them there (`publishResults`). Block, value and function numbers are the compiler's indices (`BlockId`, `ValueId`,
`FunctionId`), as `-Zemit-lean-stages` writes them. The single-function layer (`layer`)
has no calls: a tail call, an internal call or a `leave` has no end there. -/

namespace SolarTV.MIR.Lowered

open EvmRules Machine

inductive Operand where
  | value (id : Nat)
  | const (word : Word)

/-- Where codegen placed an allocation. -/
inductive Placement where
  /-- At a fixed address, below the heap. -/
  | static (address : Word)
  /-- At the free memory pointer, which it raises by `size`. -/
  | dynamic (size : Word)

inductive Inst where
  | pure (dst : Nat) (op : EVM.Op) (args : List Operand)
  | effect (dst : Option Nat) (effect : Effect) (args : List Operand)
  /-- One when the operands differ, zero otherwise. -/
  | ne (dst : Nat) (a b : Operand)
  /-- A fact a pass relied on (`assume`): `op` leaves a nonzero word on the operands, or zero
  when `negated`. Where it fails the run has no end, so the fact is checked, never trusted. -/
  | assume (negated : Bool) (op : EVM.Op) (args : List Operand)
  /-- The operand's word, unchanged: `zext`, `inttoptr` and `ptrtoint` to 256 bits, whose
  operands already keep every bit above their width zero. -/
  | word (dst : Nat) (arg : Operand)
  /-- An internal call of a function of the module, binding its result, if any. A callee that
  returns several words leaves them all, as stack scheduling does: the call binds the first to
  `dst` and each further one to its value in `extras`, if any, and when `publish` names
  the callee's return buffer, stores them there too (`publishResults`). -/
  | call (dst : Option Nat) (callee : Nat) (args : List Operand)
      (extras : List (Option Nat) := []) (publish : Option Word := none)
  /-- The operand's low `bits` bits, zero above them: `trunc` and a narrower `ptrtoint`. -/
  | low (dst : Nat) (bits : Nat) (arg : Operand)
  /-- The second operand when the first is nonzero, else the third: `select`. -/
  | select (dst : Nat) (condition yes no : Operand)
  /-- The heap floor codegen placed for the function (`heap_floor`). -/
  | heapFloor (dst : Nat) (floor : Word)
  /-- An allocation where codegen placed it (`alloc`), binding its address. -/
  | alloc (dst : Nat) (placement : Placement)

inductive Terminator where
  | jump (target : Nat)
  | branch (condition : Operand) (yes no : Nat)
  | ret (offset size : Operand)
  | revert (offset size : Operand)
  | stop
  | invalid
  /-- The first case whose operand equals the key, else the default. -/
  | switch (key : Operand) (default : Nat) (cases : List (Operand × Nat))
  /-- Control moves to the callee's entry for good, binding its parameters to the arguments. -/
  | tailCall (callee : Nat) (args : List Operand)
  /-- An internal function's return with its values, to the instruction after the call. -/
  | leave (values : List Operand)

structure Block where
  phis : List (Nat × List (Nat × Operand))
  insts : List Inst
  term : Terminator

structure Function where
  blocks : List Block
  /-- The value numbers a call binds to its arguments, in order. -/
  params : List Nat := []
  /-- The free memory pointer codegen stores at `0x40` when the function starts as a runtime
  entry, if any. -/
  entryHeap : Option Word := none

/-- The state a function starts in: its entry heap stored in the free memory pointer's slot. -/
def Function.start (f : Function) (s : State) : State :=
  match f.entryHeap with
  | none => s
  | some heap => { s with memory := s.memory.store 64 heap }

abbrev Env := Nat → Word

def Env.set (env : Env) (id : Nat) (value : Word) : Env := fun j => if j = id then value else env j

def Operand.eval (env : Env) : Operand → Word
  | .value id => env id
  | .const word => word

/-- The low `bits` bits of a word, zero above them, as the mask a narrow integer keeps. -/
def lowBits (bits : Nat) (w : Word) : Word := Evm.and w (BitVec.ofNat 256 (2 ^ bits - 1))

/-- The second word when the first is nonzero, else the third. -/
def selectWord (condition yes no : Word) : Word := if condition = 0 then no else yes

/-- A pure operation on its operands in pop order: the single word `EVM.step` leaves. -/
def evalPure (op : EVM.Op) (args : List Word) : Option Word :=
  match EVM.step ⟨0, 0⟩ op args with
  | some [value] => some value
  | _ => none

def execInsts (o : Oracle) (call : Call) : List Inst → Env → State → Option (Env × State)
  | [], env, s => some (env, s)
  | .pure dst op args :: rest, env, s =>
    match evalPure op (args.map (Operand.eval env)) with
    | some value => execInsts o call rest (env.set dst value) s
    | none => none
  | .effect dst effect args :: rest, env, s =>
    match effect.apply o call (args.map (Operand.eval env)) s with
    | some (result, s) =>
      let env := match dst, result with
        | some dst, some value => env.set dst value
        | _, _ => env
      execInsts o call rest env s
    | none => none
  | .ne dst a b :: rest, env, s => execInsts o call rest (env.set dst (Evm.ne (a.eval env) (b.eval env))) s
  | .assume negated op args :: rest, env, s =>
    match evalPure op (args.map (Operand.eval env)) with
    | some value =>
      if value = 0 then (if negated then execInsts o call rest env s else none)
      else (if negated then none else execInsts o call rest env s)
    | none => none
  | .word dst a :: rest, env, s => execInsts o call rest (env.set dst (a.eval env)) s
  | .low dst bits a :: rest, env, s => execInsts o call rest (env.set dst (lowBits bits (a.eval env))) s
  | .select dst c yes no :: rest, env, s =>
    execInsts o call rest (env.set dst (selectWord (c.eval env) (yes.eval env) (no.eval env))) s
  | .heapFloor dst floor :: rest, env, s => execInsts o call rest (env.set dst floor) s
  | .alloc dst (.static address) :: rest, env, s => execInsts o call rest (env.set dst address) s
  | .alloc dst (.dynamic size) :: rest, env, s =>
    let pointer := s.memory.load 64
    let memory := (s.memory.expand 64 32).store 64 (Evm.add size pointer)
    execInsts o call rest (env.set dst pointer) { s with memory }
  | .call .. :: _, _, _ => none

/-- Result `k` of a call, from `k`, stored at `buffer + 32 * k`. -/
def publishWords (memory : Mem) (buffer : Nat) :
    Nat → List Word → Mem
  | _, [] => memory
  | k, value :: rest => publishWords (memory.store (buffer + 32 * k) value) buffer (k + 1) rest

/-- The further results of a call returning several words, published where codegen publishes
them: result `k` at `buffer + 32 * k` for `k ≥ 1`, then `buffer` at the multi-return buffer
pointer's slot `0x20`, as its `swap1; push buffer+32k; mstore` and
`push buffer; push 0x20; mstore` do. -/
def publishResults (s : State) : Option Word → List Word → State
  | none, _ => s
  | some _, [] => s
  | some buffer, _ :: rest =>
    { s with memory := (publishWords s.memory buffer.toNat 1 rest).store 32 buffer }

/-- The target of a switch on `key`: the first case whose operand equals it, else `default`. -/
def switchTarget (env : Env) (key : Word) (default : Nat) : List (Operand × Nat) → Nat
  | [] => default
  | (case, target) :: rest => if case.eval env = key then target else switchTarget env key default rest

/-- Every phi reads the incoming environment, then all are bound. -/
def bindPhis (incoming : Env) (predecessor : Nat) :
    List (Nat × List (Nat × Operand)) → Env → Option Env
  | [], env => some env
  | (dst, arms) :: rest, env =>
    match arms.find? (fun arm => arm.1 == predecessor) with
    | some (_, operand) => bindPhis incoming predecessor rest (env.set dst (operand.eval incoming))
    | none => none

def halt (s : State) (offset size : Word) (reverted : Bool) : Halt × State :=
  let memory := s.memory.expand offset.toNat size.toNat
  let data := memory.read offset.toNat size.toNat
  ((if reverted then .revert data else .ret data), { s with memory })

/-- One block from a predecessor: its end, or the next block with the state there. -/
def block (o : Oracle) (call : Call) (f : Function) :
    Nat × Nat × Env × State → Option ((Halt × State) ⊕ (Nat × Nat × Env × State))
  | (predecessor, b, env, s) =>
    match f.blocks[b]? with
    | none => none
    | some blk =>
      match bindPhis env predecessor blk.phis env with
      | none => none
      | some env =>
        match execInsts o call blk.insts env s with
        | none => none
        | some (env, s) =>
          match blk.term with
          | .jump target => some (.inr (b, target, env, s))
          | .branch condition yes no =>
            some (.inr (b, if condition.eval env = 0 then no else yes, env, s))
          | .ret offset size => some (.inl (halt s (offset.eval env) (size.eval env) false))
          | .revert offset size => some (.inl (halt s (offset.eval env) (size.eval env) true))
          | .stop => some (.inl (.stop, s))
          | .invalid => some (.inl (.invalid, s))
          | .switch key default cases =>
            some (.inr (b, switchTarget env (key.eval env) default cases, env, s))
          | .tailCall _ _ => none
          | .leave _ => none

def run (o : Oracle) (f : Function) (call : Call) (fuel : Nat) : Option (Halt × State) :=
  iterate (block o call f) fuel (0, 0, fun _ => 0, f.start (State.initial call))

def layer : Layer where
  Prog := Function
  run := run
  mono _ _ _ n r := iterate_mono _ n _ r

/-- The fuel-free semantics of lowered MIR under an oracle. -/
noncomputable def sem (o : Oracle) : Sem := layer.sem o

theorem sem_failure (o : Oracle) (f : Function) (call : Call) (obs : Observable)
    (ran : (sem o).run f call = some obs) (failed : obs.success = false) :
    obs.logs = [] ∧ obs.storage = slotValue call.storage := by
  obtain ⟨_, _, logs, storage, _⟩ := layer.sem_failure o f call obs ran failed
  exact ⟨logs, storage⟩

theorem failureConvention (o : Oracle) : (sem o).FailureConvention := layer.failureConvention o

/-- A function of one block without phis runs it at fuel one. -/
theorem run_straight (o : Oracle) (call : Call) (insts : List Inst) (term : Terminator)
    (n : Nat) :
    run o ⟨[⟨[], insts, term⟩], [], none⟩ call (n + 1) =
      match execInsts o call insts (fun _ => 0) (State.initial call) with
      | none => none
      | some (env, s) =>
        match term with
        | .jump target => iterate (block o call ⟨[⟨[], insts, term⟩], [], none⟩) n (0, target, env, s)
        | .branch condition yes no =>
          iterate (block o call ⟨[⟨[], insts, term⟩], [], none⟩) n
            (0, if condition.eval env = 0 then no else yes, env, s)
        | .ret offset size => some (halt s (offset.eval env) (size.eval env) false)
        | .revert offset size => some (halt s (offset.eval env) (size.eval env) true)
        | .stop => some (.stop, s)
        | .invalid => some (.invalid, s)
        | .switch key default cases =>
          iterate (block o call ⟨[⟨[], insts, term⟩], [], none⟩) n
            (0, switchTarget env (key.eval env) default cases, env, s)
        | .tailCall _ _ => none
        | .leave _ => none := by
  simp only [run, iterate, block, List.getElem?_cons_zero, bindPhis, Function.start]
  cases execInsts o call insts (fun _ => 0) (State.initial call) with
  | none => rfl
  | some r => cases term <;> rfl

/-! Modules. -/

structure Module where
  functions : List Function
  /-- The function the runtime code enters. -/
  entry : Nat

/-- The environment a call enters with: each parameter bound to its argument, every other
value zero; `none` when their numbers differ. -/
def bindParams : List Nat → List Word → Env → Option Env
  | [], [], env => some env
  | param :: params, arg :: args, env => bindParams params args (env.set param arg)
  | _, _, _ => none

/-- Where an internal call returns: the caller's function and block, the value its result
binds, the instructions after the call, the block's terminator and the caller's values. -/
structure Return where
  fn : Nat
  block : Nat
  dst : Option Nat
  rest : List Inst
  term : Terminator
  env : Env
  /-- The values the further results bind, for a callee that returns several words. -/
  extras : List (Option Nat) := []
  /-- The return buffer the further results are published to, if any. -/
  publish : Option Word := none

/-- Where a module goes on: into a block of a function from a predecessor, or on with the
rest of a block after an internal call returned. -/
inductive Pos where
  | enter (fn predecessor block : Nat)
  | resume (fn block : Nat) (insts : List Inst) (term : Terminator)

/-- A configuration of a module: the returns of the calls in progress, innermost first, where
it goes on, its values and the machine state. -/
abbrev Frame := List Return × Pos × Env × State

/-- The instructions before the first internal call, and that call with the instructions
after it. -/
def splitCall : List Inst →
    List Inst × Option (Option Nat × Nat × List Operand × List (Option Nat) × Option Word × List Inst)
  | [] => ([], none)
  | .call dst callee args extras publish :: rest => ([], some (dst, callee, args, extras, publish, rest))
  | inst :: rest =>
    match splitCall rest with
    | (before, next) => (inst :: before, next)

/-- The caller's values after a return: the call's result bound to the returned value. -/
def bindResult (env : Env) : Option Nat → List Word → Option Env
  | none, [] => some env
  | some dst, [value] => some (env.set dst value)
  | _, _ => none

/-- The values the further results of a call bind, in order: none for a result no value reads. -/
def bindExtras (env : Env) : List (Option Nat) → List Word → Option Env
  | [], [] => some env
  | none :: extras, _ :: values => bindExtras env extras values
  | some dst :: extras, value :: values => bindExtras (env.set dst value) extras values
  | _, _ => none

/-- The caller's values after a return: the call's result bound to the first returned value,
and the further ones to the values that read them when the callee returns several words. -/
def bindResults (env : Env) (dst : Option Nat) : List (Option Nat) → List Word → Option Env
  | [], values => bindResult env dst values
  | extras, value :: values =>
    bindExtras (match dst with | some dst => env.set dst value | none => env) extras values
  | _ :: _, [] => none

/-- The rest of a block from its instructions: its end, or the next frame. -/
def segment (o : Oracle) (call : Call) (m : Module) (stack : List Return) (fn b : Nat)
    (insts : List Inst) (term : Terminator) (env : Env) (s : State) :
    Option ((Halt × State) ⊕ Frame) :=
  match splitCall insts with
  | (before, some (dst, callee, args, extras, publish, rest)) =>
    match execInsts o call before env s with
    | none => none
    | some (env, s) =>
      match m.functions[callee]? with
      | none => none
      | some g =>
        match bindParams g.params (args.map (Operand.eval env)) (fun _ => 0) with
        | none => none
        | some inner =>
          some (.inr (⟨fn, b, dst, rest, term, env, extras, publish⟩ :: stack, .enter callee 0 0,
            inner, g.start s))
  | (before, none) =>
    match execInsts o call before env s with
    | none => none
    | some (env, s) =>
      match term with
      | .jump target => some (.inr (stack, .enter fn b target, env, s))
      | .branch condition yes no =>
        some (.inr (stack, .enter fn b (if condition.eval env = 0 then no else yes), env, s))
      | .switch key default cases =>
        some (.inr (stack, .enter fn b (switchTarget env (key.eval env) default cases), env, s))
      | .tailCall callee args =>
        match m.functions[callee]? with
        | none => none
        | some g =>
          match bindParams g.params (args.map (Operand.eval env)) (fun _ => 0) with
          | none => none
          | some env => some (.inr (stack, .enter callee 0 0, env, g.start s))
      | .leave values =>
        match stack with
        | [] => none
        | ret :: stack =>
          let values := values.map (Operand.eval env)
          match bindResults ret.env ret.dst ret.extras values with
          | none => none
          | some env =>
            some (.inr (stack, .resume ret.fn ret.block ret.rest ret.term, env,
              publishResults s ret.publish values))
      | .ret offset size => some (.inl (halt s (offset.eval env) (size.eval env) false))
      | .revert offset size => some (.inl (halt s (offset.eval env) (size.eval env) true))
      | .stop => some (.inl (.stop, s))
      | .invalid => some (.inl (.invalid, s))

/-- One step of a module: a block entered, or the rest of a block after a return; its end, or
the next frame. -/
def moduleBlock (o : Oracle) (call : Call) (m : Module) : Frame → Option ((Halt × State) ⊕ Frame)
  | (stack, .enter fn predecessor b, env, s) =>
    match m.functions[fn]? with
    | none => none
    | some f =>
      match f.blocks[b]? with
      | none => none
      | some blk =>
        match bindPhis env predecessor blk.phis env with
        | none => none
        | some env => segment o call m stack fn b blk.insts blk.term env s
  | (stack, .resume fn b insts term, env, s) => segment o call m stack fn b insts term env s

/-- The state the module's entry function starts in. -/
def Module.startState (m : Module) (s : State) : State :=
  match m.functions[m.entry]? with
  | some f => f.start s
  | none => s

/-- A module run from its entry function's block 0, with every value zero and no call in
progress. -/
def Module.run (o : Oracle) (m : Module) (call : Call) (fuel : Nat) : Option (Halt × State) :=
  iterate (moduleBlock o call m) fuel
    ([], .enter m.entry 0 0, fun _ => 0, m.startState (State.initial call))

def moduleLayer : Layer where
  Prog := Module
  run := Module.run
  mono _ _ _ n r := iterate_mono _ n _ r

/-- The fuel-free semantics of lowered MIR modules under an oracle. -/
noncomputable def moduleSem (o : Oracle) : Sem := moduleLayer.sem o

theorem moduleFailureConvention (o : Oracle) : (moduleSem o).FailureConvention :=
  moduleLayer.failureConvention o

end SolarTV.MIR.Lowered
