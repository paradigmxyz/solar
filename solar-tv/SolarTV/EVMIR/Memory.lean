import SolarTV.Memory
import SolarTV.EVMIR.Ops

/-! Scheduled EVM memory instructions and memory-backed terminal output.
The stack operand order is offset then value for stores, and offset then size
for RETURN/REVERT. All accesses expand active memory, including MLOAD. Terminal
halts discard private memory; outgoing edges retain the projected bytes and size.
Pure operations keep the existing stack-limit and first-exception semantics. -/

namespace SolarTV.EVM.Memory
open EvmRules

inductive Op where
  | stack (op : EVM.Op)
  | load | store | storeByte | size | copy | hash

inductive Terminal where
  | control (terminal : EVM.Terminal)
  | output (reverted : Bool)

inductive Execution where
  | edge (destination : Word) (stack : Stack) (memory : SolarTV.Memory.State)
  | success (data : List SolarTV.Memory.Byte)
  | reverted (data : List SolarTV.Memory.Byte)
  | failed (reason : Failure)
  deriving DecidableEq

def step (ctx : Context) (op : Op) (stack : Stack) (memory : SolarTV.Memory.State) :
    Except Failure (Stack × SolarTV.Memory.State) :=
  if stack.length > 1024 then .error .overflow
  else match op, stack with
    | .stack op, stack => (stepChecked ctx op stack).map (fun next => (next, memory))
    | .load, offset :: rest => .ok
      (SolarTV.Memory.load memory offset.toNat :: rest, SolarTV.Memory.expand memory offset.toNat 32)
    | .store, offset :: value :: rest => .ok (rest, SolarTV.Memory.store memory offset.toNat value)
    | .storeByte, offset :: value :: rest => .ok (rest, SolarTV.Memory.storeByte memory offset.toNat value)
    | .copy, destination :: source :: count :: rest => .ok
      (rest, SolarTV.Memory.copy memory destination.toNat source.toNat count.toNat)
    | .hash, offset :: count :: rest => .ok
      (SolarTV.Memory.hash memory offset.toNat count.toNat :: rest,
        SolarTV.Memory.expand memory offset.toNat count.toNat)
    | .size, stack =>
      if stack.length + 1 > 1024 then .error .overflow
      else .ok (SolarTV.Memory.msize memory :: stack, memory)
    | _, _ => .error .underflow

def finish (terminal : Terminal) (stack : Stack) (memory : SolarTV.Memory.State) : Execution :=
  if stack.length > 1024 then .failed .overflow
  else match terminal with
    | .control terminal => match EVM.finish terminal stack with
      | .edge destination stack => .edge destination stack memory
      | .success => .success []
      | .reverted => .reverted []
      | .failed reason => .failed reason
    | .output reverted => match stack with
      | offset :: size :: _ =>
        let memory := SolarTV.Memory.expand memory offset.toNat size.toNat
        let data := SolarTV.Memory.read memory offset.toNat size.toNat
        if reverted then .reverted data else .success data
      | _ => .failed .underflow

def execute (ctx : Context) : List Op → Terminal → Stack → SolarTV.Memory.State → Execution
  | [], terminal, stack, memory => finish terminal stack memory
  | op :: ops, terminal, stack, memory => match step ctx op stack memory with
    | .error reason => .failed reason
    | .ok (stack, memory) => execute ctx ops terminal stack memory

def code : Execution → List Word
  | .edge destination stack memory =>
    0 :: destination :: BitVec.ofNat 256 stack.length :: stack ++ SolarTV.Memory.snapshot memory
  | .success data => 1 :: data.map (fun b => b.setWidth 256)
  | .reverted data => 2 :: data.map (fun b => b.setWidth 256)
  | .failed .underflow => [3]
  | .failed .overflow => [4]
  | .failed .invalid => [5]

def describe : Execution → String
  | .edge destination stack memory =>
    let edge := if destination = 0 then "fallthrough" else s!"bb{destination.toNat}"
    s!"{edge}, stack={stack.map BitVec.toNat}, {SolarTV.Memory.describe memory}"
  | .success data => if data.isEmpty then "success(empty)" else s!"success(data={data.map BitVec.toNat})"
  | .reverted data => if data.isEmpty then "revert(empty)" else s!"revert(data={data.map BitVec.toNat})"
  | .failed .underflow => "exception(stack-underflow)"
  | .failed .overflow => "exception(stack-overflow)"
  | .failed .invalid => "exception(invalid-instruction)"

end SolarTV.EVM.Memory
