import SolarTV.State.World

/-! Message-call entry and journal commit/rollback, shared by MIR and EVM IR.
CALL changes the storage owner and transfers value; CALLCODE changes only code;
DELEGATECALL preserves the original caller/value and storage owner; STATICCALL
forces static permission and zero value. All four keep transaction origin.

Child execution is separate from its boundary. A successful arbitrary child is
not a semantics for CALL; EVMIR/State/Interpreter executes the real child in the
CLI's explicit-world mode. The gas forwarding helper starts after base/memory/value
charges; it does not stand in for the full CALL price or precompile pricing.

Reference: EVMYulLean 047f630, EVM/Semantics.lean:138-218, 364-418 and 719-815.
https://eips.ethereum.org/EIPS/eip-150 -/

namespace SolarTV.Calls
open EvmRules

inductive Kind where
  | call | callcode | delegatecall | staticcall
  deriving DecidableEq

structure Message where
  kind : Kind
  destination : World.Address
  value : Word
  input : List World.Byte
  deriving DecidableEq

/-- Whether this opcode explicitly supplies a value operand, not CALLVALUE as
observed by the child. DELEGATECALL can inherit nonzero value in static mode. -/
def hasValue (message : Message) : Bool :=
  (message.kind == .call || message.kind == .callcode) && message.value != 0

def childEnvironment (parent : World.Environment) (message : Message) : World.Environment :=
  {parent with
    address := if message.kind == .call || message.kind == .staticcall then
      message.destination else parent.address
    codeAddress := message.destination
    caller := if message.kind == .delegatecall then parent.caller else parent.address
    value := if message.kind == .delegatecall then parent.value
      else if message.kind == .staticcall then 0 else message.value
    input := message.input
    isStatic := parent.isStatic || message.kind == .staticcall
    depth := parent.depth + 1}

structure Entry where
  /-- Snapshot after warming the destination, before any value transfer. -/
  checkpoint : World.State
  world : World.State
  environment : World.Environment

inductive Preparation where
  | enter (entry : Entry)
  | rejected (world : World.State)
  | exception (reason : Storage.Error)

def prepare (world : World.State) (parent : World.Environment) (message : Message)
    (fork : Fork := .cancun) : Preparation :=
  if message.kind == .staticcall && !fork.atLeast .byzantium then .exception .unavailable
  else if parent.isStatic && message.kind == .call && message.value != 0 then .exception .staticViolation
  else
    let checkpoint := World.warmAccount world message.destination
    if parent.depth ≥ 1024 then .rejected checkpoint
    else if hasValue message && (world.accounts parent.address).balance.ult message.value then
      .rejected checkpoint
    else
      let entered := if message.kind == .call then
        World.transfer checkpoint parent.address message.destination message.value
      else some checkpoint
      match entered with
      | none => .rejected checkpoint
      | some world =>
        let world := if message.kind == .call && !fork.atLeast .spuriousDragon then
          World.setAccount world message.destination
            {(world.accounts message.destination) with present := true}
          else world
        .enter ⟨checkpoint, world, childEnvironment parent message⟩

inductive ChildResult where
  | success (world : World.State) (data : List World.Byte) (remaining : Nat)
  | reverted (data : List World.Byte) (remaining : Nat)
  | exception

structure Result where
  world : World.State
  success : Bool
  returnData : List World.Byte
  remaining : Nat

/-- Revert keeps return data and unused gas but restores the journal. An
exception has empty return data and burns the complete supplied child budget. -/
def complete (checkpoint : World.State) : ChildResult → Result
  | .success world data remaining => ⟨world, true, data, remaining⟩
  | .reverted data remaining => ⟨checkpoint, false, data, remaining⟩
  | .exception => ⟨checkpoint, false, [], 0⟩

structure Budget where
  charged : Nat
  supplied : Nat
  deriving DecidableEq

def forwardGas (fork : Fork) (available requested : Nat) (value : Bool) : Option Budget :=
  let cap := if fork.atLeast .tangerineWhistle then available - available / 64 else available
  if !fork.atLeast .tangerineWhistle && requested > available then none
  else
    let forwarded := min requested cap
    some ⟨forwarded, forwarded + if value then 2300 else 0⟩

@[simp] theorem reverted_restores_journal (checkpoint : World.State)
    (data : List World.Byte) (gas : Nat) :
    (complete checkpoint (.reverted data gas)).world = checkpoint := rfl

@[simp] theorem exceptional_call_burns_gas (checkpoint : World.State) :
    (complete checkpoint .exception).remaining = 0 := rfl

@[simp] theorem delegate_keeps_owner (parent : World.Environment) (to : World.Address)
    (value : Word) (input : List World.Byte) :
    (childEnvironment parent ⟨.delegatecall, to, value, input⟩).address = parent.address := by
  simp [childEnvironment]

@[simp] theorem delegate_keeps_caller (parent : World.Environment) (to : World.Address)
    (value : Word) (input : List World.Byte) :
    (childEnvironment parent ⟨.delegatecall, to, value, input⟩).caller = parent.caller := by
  simp [childEnvironment]

@[simp] theorem delegate_keeps_value (parent : World.Environment) (to : World.Address)
    (value : Word) (input : List World.Byte) :
    (childEnvironment parent ⟨.delegatecall, to, value, input⟩).value = parent.value := by
  simp [childEnvironment]

/-- A delegate call in a static frame keeps static permission even if its
inherited CALLVALUE is nonzero. The value is not a new transfer. -/
theorem static_is_inherited (parent : World.Environment) (message : Message)
    (permission : parent.isStatic = true) :
    (childEnvironment parent message).isStatic = true := by simp [childEnvironment, permission]

example : forwardGas .homestead 6400 6401 false = none := by decide
example : forwardGas .tangerineWhistle 6400 6401 false = some ⟨6300, 6300⟩ := by decide
example : forwardGas .cancun 6400 6401 true = some ⟨6300, 8600⟩ := by decide

end SolarTV.Calls
