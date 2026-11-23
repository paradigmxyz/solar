import SolarTV.EVMIR.ContinuationFlow
import SolarTV.State.ByteMemory
import SolarTV.State.Calls

/-! Observe EVM IR at interaction boundaries before opcode admission.

The caller supplies a shared implementation of ordinary opcodes and shared
responses to preceding interactions. Stopping at the requested ordinal makes
every finite prefix observable, including requests before an eventual halt
or in a region that loops. This model prices neither transfers nor messages.
It does not relate code images, execute callees or establish EIP-150 budgets. -/

namespace SolarTV.EVM.InteractionTrace
open EvmRules

inductive Kind where
  | gas | extcodesize | call | staticcall

def Kind.named : String → Option Kind
  | "gas" => some .gas
  | "extcodesize" => some .extcodesize
  | "call" => some .call
  | "staticcall" => some .staticcall
  | _ => none

def Kind.inputs : Kind → Nat
  | .gas => 0
  | .extcodesize => 1
  | .call => 7
  | .staticcall => 6

def signatureFits (signatures : StackGraph.Signatures) (name : String) : Bool :=
  match Kind.named name, signatures name with
  | some kind, some signature => signature == (kind.inputs, 1)
  | _, _ => true

structure Range where
  offset : Word
  size : Word
  deriving DecidableEq

inductive Request where
  | gas
  | codeSize (address : World.Address)
  | message (kind : Calls.Kind) (requestedGas : Word) (destination : World.Address)
      (value : Word) (input output : Range) (data : List World.Byte)
  deriving DecidableEq

private def decode (memory : List World.Byte) : Kind → Stack → Option Request
  | .gas, [] => some .gas
  | .extcodesize, [address] => some (.codeSize (address.setWidth 160))
  | .call, [gas, address, value, offset, size, output, count] =>
    some (.message .call gas (address.setWidth 160) value ⟨offset,size⟩ ⟨output,count⟩
      (Execution.readBytes memory offset.toNat size.toNat))
  | .staticcall, [gas, address, offset, size, output, count] =>
    some (.message .staticcall gas (address.setWidth 160) 0 ⟨offset,size⟩ ⟨output,count⟩
      (Execution.readBytes memory offset.toNat size.toNat))
  | _, _ => none

structure View where
  stack : Stack
  memory : List World.Byte

def request (kind : Kind) (view : View) : Option Request :=
  decode view.memory kind (view.stack.take kind.inputs)

/-- An admitted interaction cannot observe relocation tags buried below
its operands. This covers calldata read from the supplied memory as well
as the requested gas, address, value and memory ranges. -/
theorem stable_request (kind : Kind) (memory : List World.Byte)
    (shape : ContinuationFlow.Shape) (slots : List ContinuationFlow.Slot)
    (left right : Nat → Word) (related : ContinuationFlow.Matches shape slots)
    (safe : (shape.take kind.inputs).all Option.isNone = true) :
    request kind ⟨slots.map (ContinuationFlow.Slot.encode left), memory⟩ =
    request kind ⟨slots.map (ContinuationFlow.Slot.encode right), memory⟩ := by
  unfold request
  rw [ContinuationFlow.checked_operands shape slots kind.inputs left right related safe]

structure Cursor (Payload : Type) where
  remaining : Nat
  payload : Payload

inductive Observation (Halt : Type) where
  | request (request : Request)
  | halted (outcome : Halt)
  | missingOperands
  deriving DecidableEq

/-- Run ordinary effects and resume past earlier interactions with the
supplied replies. Return the next selected request before running its reply. -/
def handlers (view : Payload → View) (ordinary : StackGraph.Handlers Payload Halt)
    (reply : Request → Payload → Except Halt Payload) :
    StackGraph.Handlers (Cursor Payload) (Observation Halt) where
  operation label name height state :=
    match Kind.named name with
    | none =>
      (ordinary.operation label name height state.payload).map
        (fun payload => {state with payload}) |>.mapError .halted
    | some kind =>
      match request kind (view state.payload) with
      | none => .error .missingOperands
      | some event =>
        match state.remaining with
        | 0 => .error (.request event)
        | remaining + 1 =>
          (reply event state.payload).map (Cursor.mk remaining) |>.mapError .halted
  branch label height state :=
    (ordinary.branch label height state.payload).map
      (fun (taken,payload) => (taken, {state with payload})) |>.mapError .halted
  terminal label name height state :=
    .halted (ordinary.terminal label name height state.payload)

end SolarTV.EVM.InteractionTrace
