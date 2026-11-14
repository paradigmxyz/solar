import SolarTV.EVMIR.Memory

/-! Acyclic scheduled regions, expanded from the original CFG by the importer.
Instructions use the existing stack/memory semantics. Transfers retain the
temporary stack growth of their concrete lowering, then execute the successor
with the same live state. Terminal outcomes discard private stack and memory.
Shared tails may appear more than once in the tree; expansion is bounded and
cycles are rejected rather than cut into arbitrary outgoing edges. Gas-priced
regions, dynamic targets and layout-observing operations are not admitted.

Reference: `TerminatorKind::lowering_stack_growth` and EVMYulLean's stack bound.
The importer preserves the original next block, including jump fallthroughs. -/

namespace SolarTV.EVM.Region
open EvmRules

inductive Code where
  | terminal (terminal : Memory.Terminal)
  | instructions (ops : List Memory.Op) (next : Code)
  | jump (growth : Nat) (next : Code)
  | branch (yes no : Code)

def instructions (ctx : Context) : List Memory.Op → Stack → SolarTV.Memory.State →
    Except Failure (Stack × SolarTV.Memory.State)
  | [], stack, memory => .ok (stack,memory)
  | op :: ops, stack, memory => do
    let (stack,memory) ← Memory.step ctx op stack memory
    instructions ctx ops stack memory

def execute (ctx : Context) : Code → Stack → SolarTV.Memory.State → Memory.Execution
  | .terminal terminal, stack, memory => Memory.finish terminal stack memory
  | .instructions ops next, stack, memory =>
    match instructions ctx ops stack memory with
    | .error reason => .failed reason
    | .ok (stack,memory) => execute ctx next stack memory
  | .jump growth next, stack, memory =>
    if stack.length + growth > 1024 then .failed .overflow
    else execute ctx next stack memory
  | .branch yes no, stack, memory =>
    if stack.length + 1 > 1024 then .failed .overflow
    else match stack with
      | condition :: rest => if condition = 0 then execute ctx no rest memory else execute ctx yes rest memory
      | _ => .failed .underflow

/-- Moving a stack swap across an unconditional transfer preserves both branches'
live state when the transfer has enough temporary stack space. -/
theorem swap_across_jump (ctx : Context) (next : Code) (a b : Word) (memory : SolarTV.Memory.State) :
    execute ctx (.instructions [.stack (.swap 0)] (.jump 1 next)) [a,b] memory =
      execute ctx (.jump 1 (.instructions [.stack (.swap 0)] next)) [a,b] memory := rfl

end SolarTV.EVM.Region
