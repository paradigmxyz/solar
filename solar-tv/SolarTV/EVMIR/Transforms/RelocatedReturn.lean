import SolarTV.EVMIR.State.CodeChunks

/-! A funded bytecode path with one relocated internal return. The kernel
decodes instructions and checks each physical successor from the entry.
Static transfers and JUMPDESTs do no logical work. The caller's continuation
address is tracked separately from ordinary stack words. Stateful handlers
receive only ordinary words, so the different physical address cannot affect
memory, storage or logs. Gas and code-position observers lie outside this
projection. -/

namespace SolarTV.EVM.RelocatedReturn
open EvmRules SolarTV.Execution

inductive Action where
  | push (value : Word)
  | dup (index : Nat)
  | swap (index : Nat)
  | op (opcode inputs outputs : Nat)
  | guard (taken : Bool)
  | saveReturn
  | resume
  | stop
  deriving DecidableEq, Repr

/-- The projected path admits only instructions whose data effects are
independent of the physical instruction address and byte image. Stack effects
come from the same fork-specific opcode table as concrete execution. -/
private def ordinaryAction (cfg : State.Config) (opcode : Nat) : Option Action :=
  let info := State.available cfg opcode
  if info.available && [0x01,0x10,0x11,0x14,0x15,0x17,0x1c,0x20,0x34,0x35,0x36,
      0x52,0x54,0x55,0xa3].contains opcode then
    some (.op opcode info.inputs info.outputs)
  else none

structure ByteOp where
  opcode : Nat
  immediate : Option Word
  next : Nat

def decode (image : State.CodeChunks) (pc : Nat) : Option ByteOp := do
  let opcode := (← image.get? pc).toNat
  let width := State.pushWidth opcode
  let endPc := pc+1+width
  if endPc > image.size then none else
  return ⟨opcode,if opcode = 0x5f || width > 0 then some (image.immediate pc width) else none,endPc⟩

structure Route where
  returnPc : Nat
  actions : List Action
  deriving DecidableEq, Repr

private def destination (image : State.CodeChunks) (target : Word) : Bool :=
  (image.jumps 0).contains target.toNat

private def lift (cfg : State.Config) (image : State.CodeChunks) (producer returnPc : Nat) :
    Nat → List Nat → Option (List Action)
  | 0, _ => none
  | fuel+1, pc :: next :: rest => do
    let inst ← decode image pc
    if pc = producer then
      let address ← inst.immediate
      if address.toNat = returnPc && inst.next = next then
        return .saveReturn :: (← lift cfg image producer returnPc fuel (next :: rest))
      else none
    else if let some target := inst.immediate then
      let jump ← decode image inst.next
      if next = inst.next && jump.opcode = 0x57 then
        let afterJump ← rest.head?
        if !destination image target || jump.next = target.toNat then none else
        let taken := afterJump = target.toNat
        if !taken && afterJump != jump.next then none else
        return .guard taken :: (← lift cfg image producer returnPc fuel rest)
      else if next = inst.next && jump.opcode = 0x56 then
        let afterJump ← rest.head?
        if !destination image target || afterJump != target.toNat then none
        else lift cfg image producer returnPc fuel rest
      else if inst.next = next then
        return .push target :: (← lift cfg image producer returnPc fuel (next :: rest))
      else none
    else if inst.opcode = 0x56 then
      if next = returnPc && destination image (BitVec.ofNat 256 returnPc) then
        return .resume :: (← lift cfg image producer returnPc fuel (next :: rest))
      else none
    else if inst.next != next then none
    else if inst.opcode = 0x5b then lift cfg image producer returnPc fuel (next :: rest)
    else
      let action ←
        if 0x80 ≤ inst.opcode && inst.opcode ≤ 0x8f then some (.dup (inst.opcode-0x80))
        else if 0x90 ≤ inst.opcode && inst.opcode ≤ 0x9f then some (.swap (inst.opcode-0x8f))
        else ordinaryAction cfg inst.opcode
      return action :: (← lift cfg image producer returnPc fuel (next :: rest))
  | _+1, [pc] => do
    let inst ← decode image pc
    if pc = returnPc+1 && inst.opcode = 0x00 then some [.stop] else none
  | _+1, [] => none

/-- The continuation is recovered from the encoded PUSH at the caller, and its
destination must be a real JUMPDEST in the decoded image. -/
def physicalRoute (cfg : State.Config) (image : State.CodeChunks) (producer : Nat) (pcs : List Nat) :
    Option Route := do
  if pcs.head? != some 0 then none else
  let continuation ← (← decode image producer).immediate
  if !destination image continuation then none else
  let actions ← lift cfg image producer continuation.toNat pcs.length pcs
  return ⟨continuation.toNat,actions⟩

private def swap {α : Type} (index : Nat) (stack : List α) : Option (List α) := do
  let first ← stack.head?
  let other ← stack[index]?
  return (stack.set 0 other).set index first

/-- False denotes an ordinary word, true the relocated return address.
A branch and an ordinary instruction cannot consume the address. One spare
slot also covers physical PUSH/JUMP sequences removed from the logical tape. -/
def usesReturn : List Action → List Bool → Bool → Bool → Bool
  | [], stack, saved, resumed => saved && resumed && !stack.contains true
  | action :: rest, stack, saved, resumed =>
    if stack.length ≥ 1023 then false else
    let ordinary (n : Nat) := n ≤ stack.length && !(stack.take n).contains true
    match action with
    | .push _ => usesReturn rest (false :: stack) saved resumed
    | .dup i => match stack[i]? with
      | some word => usesReturn rest (word :: stack) saved resumed
      | none => false
    | .swap i => match swap i stack with
      | some stack => usesReturn rest stack saved resumed
      | none => false
    | .op _ inputs outputs =>
      ordinary inputs && usesReturn rest (List.replicate outputs false ++ stack.drop inputs) saved resumed
    | .guard _ => ordinary 1 && usesReturn rest (stack.drop 1) saved resumed
    | .saveReturn => !saved && usesReturn rest (true :: stack) true resumed
    | .resume =>
      !resumed && stack.head? = some true && !stack.tail.contains true &&
        usesReturn rest stack.tail saved true
    | .stop => rest.isEmpty && saved && resumed && !stack.contains true

abbrev Handler (Payload : Type) :=
  Nat → List Word → Payload → Except String (List Word × Payload)

structure Cut (Payload : Type) where
  stack : List Word
  payload : Payload

private def required (value : Option α) (reason : String) : Except String α :=
  match value with | some value => .ok value | none => .error reason

private def arguments (stack : List (Option Word)) (arity : Nat) : Except String (List Word) := do
  if stack.length < arity then .error "short return-path stack"
  else (stack.take arity).mapM fun value => required value "return address used as data"

private def execute (handler : Handler Payload) :
    List Action → List (Option Word) → Payload → Except String (Cut Payload)
  | [], _, _ => .error "return path has no continuation"
  | action :: rest, stack, payload => do
    match action with
    | .push value => execute handler rest (some value :: stack) payload
    | .dup index =>
      let word ← required stack[index]? "short DUP on return path"
      execute handler rest (word :: stack) payload
    | .swap index =>
      execute handler rest (← required (swap index stack) "short SWAP on return path") payload
    | .op opcode inputs outputs =>
      let (words,next) ← handler opcode (← arguments stack inputs) payload
      if words.length != outputs then .error "invalid return-path opcode result" else
      execute handler rest (words.map some ++ stack.drop inputs) next
    | .guard taken =>
      let [condition] ← arguments stack 1 | .error "missing return-path guard"
      if (condition != 0) != taken then .error "input takes a different bytecode path"
      else execute handler rest stack.tail payload
    | .saveReturn => execute handler rest (none :: stack) payload
    | .resume =>
      let none :: stack := stack | .error "return did not consume the caller's address"
      execute handler rest stack payload
    | .stop => return ⟨← arguments stack stack.length,payload⟩

/-- A shared paid instruction acts on the ordinary operands using the concrete
EVM memory, account and log semantics. The omitted stack suffix has already
passed the stack-role and height checks. Each opcode is charged in independent
physical replay, outside the logical comparison. -/
structure Live where
  world : Journal
  memory : List Byte
  returnData : List Byte

/-- Compare the live journal after erasing the root bytecode that is under
validation. Other code, original storage, transient slots, warmth, logs,
and the refund counter remain in the comparison. -/
def withoutRootCode (root : Address) (world : Journal) : Journal :=
  let erase (account : Account) :=
    if account.address = root then {account with code := []} else account
  {world with accounts := world.accounts.map erase, original := world.original.map erase}

def fundedHandler (cfg : State.Config) (env : World.Environment) : Handler Live :=
  fun opcode operands state => do
    let frame : State.Frame := ⟨env,[],0,operands,state.memory,state.returnData,1000000⟩
    match ← State.instructionWithImage cfg state.world frame 1
        (BitVec.ofNat 8 opcode) (fun _ => 0) (fun _ => false) with
    | .next world frame => return (frame.stack,⟨world,frame.memory,frame.returnData⟩)
    | _ => .error "return-path effect did not complete"

def run (cfg : State.Config) (image : State.CodeChunks) (producer : Nat) (pcs : List Nat)
    (handler : Handler Payload) (state : Payload) : Except String (Nat × Cut Payload) := do
  let route ← required (physicalRoute cfg image producer pcs) "invalid physical return route"
  if !usesReturn route.actions [] false false then .error "invalid return-address stack flow"
  else return (route.returnPc, ← execute handler route.actions [] state)

/-- A verified physical return may change addresses. For shared live inputs
each route reaches its own valid continuation with the same observable payload. -/
theorem correspondence (cfg : State.Config) (before after : State.CodeChunks) (oldPush newPush : Nat)
    (oldPcs newPcs : List Nat) (oldReturn newReturn : Nat) (actions : List Action)
    (old : physicalRoute cfg before oldPush oldPcs = some ⟨oldReturn,actions⟩)
    (new : physicalRoute cfg after newPush newPcs = some ⟨newReturn,actions⟩)
    (roles : usesReturn actions [] false false = true)
    (handler : Handler Payload) (state : Payload) :
    (run cfg before oldPush oldPcs handler state).map Prod.snd =
      (run cfg after newPush newPcs handler state).map Prod.snd := by
  simp only [run,old,new,roles, Bool.not_true, Bool.false_eq_true, ite_false,
    required,Bind.bind,Except.bind]
  cases execute handler actions [] state <;> rfl

-- An immediate byte equal to JUMPDEST is not a return target.
example (cfg : State.Config) :
    physicalRoute cfg (.leaf ⟨[0x60,0x03,0x60,0x5b,0x00],5,by decide,3,[],
      by decide +kernel⟩) 0 [0,2,3,4] = none := rfl

end SolarTV.EVM.RelocatedReturn
