import SolarTV.MIR.State.IR

/-! Erasing named-type declarations after resolving all executable type uses.

The source modules are independently parsed and validated. Their complete typed
function tables include resolved parameter, result, operand and operation types;
no name lookup remains in this representation. Globals, phase, interfaces,
attributes, function order and retained source text must also agree. Only the
entire named-type presentation table may disappear.

The kernel checks both independently constructed resolved modules for equality.
Every interpreter of that data consequently agrees, for every state and work
bound, including stateful loops and early failures. No unknown opcode is executed
or assigned a default meaning merely to establish identity.

Compiler parsing, schema export and the typed constructor renderer remain
trusted. This structural certificate does not establish primitive instruction
implementations, physical gas or backend layout. Actual calls and GAS execution
retain their normal interpreter restrictions.
-/

namespace SolarTV.MIR.TypeDCE

structure Program where
  code : List String
  functions : State.Functions

abbrev Interpreter (State Observation : Type) :=
  List String → SolarTV.MIR.State.Functions → Nat → State → Observation

def execute (program : Program) (interpret : Interpreter State Observation)
    (fuel : Nat) (state : State) : Observation :=
  interpret program.code program.functions fuel state

/-- Identity is checked on resolved program data, not matching execution errors
or a host assertion that source aliases do not matter. -/
theorem certificate (before after : Program) (same : before = after)
    (interpret : Interpreter State Observation) (fuel : Nat) (state : State) :
    execute before interpret fuel state = execute after interpret fuel state := by
  rw [same]

end SolarTV.MIR.TypeDCE
