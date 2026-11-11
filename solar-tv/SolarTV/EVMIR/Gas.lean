import SolarTV.EVMIR.Memory

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

def pureCost : EVM.Op → Nat
  | .mul | .div | .sdiv | .mod | .smod | .signextend => 5
  | .addmod | .mulmod => 8
  | .exp => 10
  | .pop | .calldatasize | .returndatasize => 2
  | .push value => if value = 0 then 2 else 3
  | .eq | .sub | .add | .and | .or | .xor | .iszero | .not | .dup _ | .swap _
  | .lt | .gt | .slt | .sgt | .byte | .shl | .shr | .sar => 3

/-- The bytes of a word's minimal big-endian encoding, which EXP charges for. -/
def byteLength (value : Word) : Nat :=
  if value = 0 then 0 else Nat.log2 value.toNat / 8 + 1

/-- EXP's dynamic charge: 50 gas per byte of the exponent, its second operand. -/
def exponentCost : Memory.Op → Stack → Nat
  | .stack .exp, _ :: exponent :: _ => 50 * byteLength exponent
  | _, _ => 0

def baseCost : Memory.Op → Nat
  | .stack op => pureCost op
  | .size => 2
  | .load | .store | .storeByte => 3

def touched (op : Memory.Op) (stack : Stack) (memory : SolarTV.Memory.State) :
    SolarTV.Memory.State :=
  match op, stack with
  | .load, offset :: _ | .store, offset :: _ => SolarTV.Memory.expand memory offset.toNat 32
  | .storeByte, offset :: _ => SolarTV.Memory.expand memory offset.toNat 1
  | _, _ => memory

def cost (op : Memory.Op) (stack : Stack) (memory : SolarTV.Memory.State) : Nat :=
  baseCost op + exponentCost op stack + expansionCost memory (touched op stack memory)

def finish (terminal : Memory.Terminal) (stack : Stack) (memory : SolarTV.Memory.State)
    (gas : Nat) : Result :=
  match terminal with
  | .control (.jump _) | .control (.branch _ _) =>
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

def execute (ctx : Context) : List Op → Memory.Terminal → Stack → SolarTV.Memory.State → Nat → Result
  | [], terminal, stack, memory, gas => finish terminal stack memory gas
  | .gas :: ops, terminal, stack, memory, gas =>
    if gas < 2 then exhausted
    else if stack.length + 1 > 1024 then halted (.failed .overflow) (gas - 2)
    else execute ctx ops terminal (BitVec.ofNat 256 (gas - 2) :: stack) memory (gas - 2)
  | .instruction op :: ops, terminal, stack, memory, gas =>
    let charge := cost op stack memory
    if gas < charge then exhausted
    else match Memory.step ctx op stack memory with
      | .error reason => halted (.failed reason) (gas - charge)
      | .ok (stack, memory) => execute ctx ops terminal stack memory (gas - charge)

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
