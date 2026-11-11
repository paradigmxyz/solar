import SolarTV.Machine
import SolarTV.EVMIR.Ops

/-! Lowered MIR functions as a layer.

This is MIR at the `lowered` phase, the input of `-Zvalidate-passes`: word SSA values,
phis, branches, the pure EVM operations, effects, and the halts of a contract entry.
`lower-abi` and `lower-dispatch` run before it, so the semantic-phase pipeline that
builds the dispatcher is outside this semantics. A function runs block by block with one
unit of fuel per block, from block 0 with every value zero. A phi reads the incoming
environment and selects by predecessor. Pure operations are the word semantics of
`EVM.step`, the same `EvmRules.Evm` definitions the agreement theorems relate to the
encoder. Malformed code (an unknown block, a phi without the predecessor, an effect of
the wrong arity) has no end, so it has no observable result. -/

namespace SolarTV.MIR.Lowered

open EvmRules Machine

inductive Operand where
  | value (id : Nat)
  | const (word : Word)

inductive Inst where
  | pure (dst : Nat) (op : EVM.Op) (args : List Operand)
  | effect (dst : Option Nat) (effect : Effect) (args : List Operand)

inductive Terminator where
  | jump (target : Nat)
  | branch (condition : Operand) (yes no : Nat)
  | ret (offset size : Operand)
  | revert (offset size : Operand)
  | stop
  | invalid

structure Block where
  phis : List (Nat × List (Nat × Operand))
  insts : List Inst
  term : Terminator

structure Function where
  blocks : List Block

abbrev Env := Nat → Word

def Env.set (env : Env) (id : Nat) (value : Word) : Env := fun j => if j = id then value else env j

def Operand.eval (env : Env) : Operand → Word
  | .value id => env id
  | .const word => word

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

def run (o : Oracle) (f : Function) (call : Call) (fuel : Nat) : Option (Halt × State) :=
  iterate (block o call f) fuel (0, 0, fun _ => 0, State.initial call)

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
    run o ⟨[⟨[], insts, term⟩]⟩ call (n + 1) =
      match execInsts o call insts (fun _ => 0) (State.initial call) with
      | none => none
      | some (env, s) =>
        match term with
        | .jump target => iterate (block o call ⟨[⟨[], insts, term⟩]⟩) n (0, target, env, s)
        | .branch condition yes no =>
          iterate (block o call ⟨[⟨[], insts, term⟩]⟩) n
            (0, if condition.eval env = 0 then no else yes, env, s)
        | .ret offset size => some (halt s (offset.eval env) (size.eval env) false)
        | .revert offset size => some (halt s (offset.eval env) (size.eval env) true)
        | .stop => some (.stop, s)
        | .invalid => some (.invalid, s) := by
  simp only [run, iterate, block, List.getElem?_cons_zero, bindPhis]
  cases execInsts o call insts (fun _ => 0) (State.initial call) with
  | none => rfl
  | some r => cases term <;> rfl

end SolarTV.MIR.Lowered
