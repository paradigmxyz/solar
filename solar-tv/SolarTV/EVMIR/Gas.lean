import SolarTV.EVMIR.Memory
import SolarTV.State.Fork

/-! Gas-aware execution of scheduled EVM IR windows.
The modeled costs follow EVMYulLean 047f630, EVM/Gas.lean:18-60, 87-90,
272-297 and GasConstants.lean:7-12, 40. They are unchanged for our opcode
subset from Cancun through Osaka. Memory charges use natural arithmetic.
Each instruction pays before executing; GAS returns the budget after its own
charge. Exceptions consume the remaining gas, whereas REVERT preserves it.

The window starts after any entry JUMPDEST. Static terminals include the label
PUSH and JUMP/JUMPI, but not execution of the outgoing successor. Exact equality
is compositional for this projection. Resource refinement permits extra gas and
success where the original runs out; it requires a gas-insensitive continuation.
Calls, storage, refunds and transaction intrinsic gas are outside this fragment. -/

namespace SolarTV.EVM.Gas
open EvmRules

inductive Op where
  | instruction (op : Memory.Op)
  | gas

inductive Observation where
  | execution (result : Memory.Execution)
  | outOfGas
  deriving DecidableEq

structure Result where
  observation : Observation
  remaining : Nat
  deriving DecidableEq

def halted (result : Memory.Execution) (remaining : Nat) : Result :=
  match result with
  | .failed _ => ⟨.execution result, 0⟩
  | _ => ⟨.execution result, remaining⟩

def exhausted : Result := ⟨.outOfGas, 0⟩

def memoryCost (words : Nat) : Nat := 3 * words + words * words / 512

def expansionCost (before after : SolarTV.Memory.State) : Nat :=
  memoryCost after.words.toNat - memoryCost before.words.toNat

def pureCost (op : EVM.Op) (fork : Fork := .cancun) : Nat :=
  match op with
  | .mul | .div | .sdiv | .mod | .smod | .signextend | .clz => 5
  | .addmod | .mulmod => 8
  | .exp => 10
  | .exchange _ _ => if fork.atLeast .amsterdam then 3 else 9
  | .pop | .calldatasize | .returndatasize => 2
  | .push value => if value = 0 && fork.atLeast .shanghai then 2 else 3
  | .eq | .sub | .add | .and | .or | .xor | .iszero | .not | .dup _ | .swap _
    | .lt | .gt | .slt | .sgt | .byte | .shl | .shr | .sar => 3

def baseCost (op : Memory.Op) (fork : Fork := .cancun) : Nat :=
  match op with
  | .stack op => pureCost op fork
  | .size => 2
  | .load | .store | .storeByte | .copy => 3
  | .hash => 30

def touched (op : Memory.Op) (stack : Stack) (memory : SolarTV.Memory.State) :
    SolarTV.Memory.State :=
  match op, stack with
  | .load, offset :: _ | .store, offset :: _ => SolarTV.Memory.expand memory offset.toNat 32
  | .storeByte, offset :: _ => SolarTV.Memory.expand memory offset.toNat 1
  | .copy, destination :: source :: count :: _ =>
    SolarTV.Memory.expand (SolarTV.Memory.expand memory source.toNat count.toNat)
      destination.toNat count.toNat
  | .hash, offset :: count :: _ => SolarTV.Memory.expand memory offset.toNat count.toNat
  | _, _ => memory

def cost (op : Memory.Op) (stack : Stack) (memory : SolarTV.Memory.State)
    (fork : Fork := .cancun) : Nat :=
  let dynamic := match op, stack with
    | .stack .exp, _ :: exponent :: _ =>
      (if fork.atLeast .spuriousDragon then 50 else 10) * ((256 - exponent.clz.toNat + 7) / 8)
    | .copy, _ :: _ :: count :: _ => 3 * ((count.toNat + 31) / 32)
    | .hash, _ :: count :: _ => 6 * ((count.toNat + 31) / 32)
    | _, _ => 0
  baseCost op fork + dynamic + expansionCost memory (touched op stack memory)

def finish (terminal : Memory.Terminal) (stack : Stack) (memory : SolarTV.Memory.State)
    (gas : Nat) : Result :=
  match terminal with
  | .control (.jump _) | .control (.branch _ _) | .control (.branchStop _) =>
    -- The label PUSH costs 3 and can fail before the transfer executes.
    if gas < 3 then exhausted
    else if stack.length + 1 > 1024 then halted (.failed .overflow) (gas - 3)
    else
      let transfer := match terminal with | .control (.jump _) => 8 | _ => 10
      if gas - 3 < transfer then exhausted
      else halted (Memory.finish terminal stack memory) (gas - 3 - transfer)
  | .output _ =>
    let expanded := match stack with
      | offset :: size :: _ => SolarTV.Memory.expand memory offset.toNat size.toNat
      | _ => memory
    let charge := expansionCost memory expanded
    if gas < charge then exhausted
    else halted (Memory.finish terminal stack memory) (gas - charge)
  | .control .stop | .control .invalid | .control .returnEmpty | .control .revertEmpty =>
    halted (Memory.finish terminal stack memory) gas

def execute (ctx : Context) (ops : List Op) (terminal : Memory.Terminal) (stack : Stack)
    (memory : SolarTV.Memory.State) (gas : Nat) (fork : Fork := .cancun) : Result :=
  match ops with
  | [] => finish terminal stack memory gas
  | .gas :: ops =>
    if gas < 2 then exhausted
    else if stack.length + 1 > 1024 then halted (.failed .overflow) (gas - 2)
    else execute ctx ops terminal (BitVec.ofNat 256 (gas - 2) :: stack) memory (gas - 2) fork
  | .instruction op :: ops =>
    let charge := cost op stack memory fork
    if gas < charge then exhausted
    else match Memory.step ctx op stack memory with
      | .error reason => halted (.failed reason) (gas - charge)
      | .ok (stack, memory) => execute ctx ops terminal stack memory (gas - charge) fork
termination_by structural ops

/-- This resource policy is deliberately weaker than exact EVM equivalence.
Only original out-of-gas is unconstrained; reverts and other failures stay defined.
An outgoing continuation must not distinguish extra remaining gas. -/
def refines (original candidate : Result) : Prop :=
  original.observation = .outOfGas ∨
    (original.observation = candidate.observation ∧ original.remaining ≤ candidate.remaining)

instance (original candidate : Result) : Decidable (refines original candidate) :=
  inferInstanceAs (Decidable (_ ∨ (_ ∧ _)))

def code (result : Result) : List Word :=
  BitVec.ofNat 256 result.remaining :: (match result.observation with
    | .outOfGas => [6]
    | .execution result => Memory.code result)

def describe (result : Result) : String :=
  let outcome := match result.observation with
    | .outOfGas => "out-of-gas"
    | .execution result => Memory.describe result
  s!"{outcome}, gas={result.remaining}"

end SolarTV.EVM.Gas
