import SolarTV.EVMIR.State.Selfdestruct
import SolarTV.EVMIR.Precompiles.Execute
import SolarTV.EVMIR.State.Environment
import SolarTV.EVMIR.State.Delegation
import SolarTV.EVMIR.State.Messages

/-! Gas-charged bytecode execution with real nested frames and constructors.
Every frame owns its stack, memory and return-data buffer. Accounts, original
storage and access sets are transaction-wide and journaled at call boundaries.
Unsupported opcodes and finite evaluator limits are model errors;
they never become EVM exceptions and cannot prove a pair correct.

The explicit-world mode starts after transaction validation, with zero root
value and explicit transaction/block context. The CLI admits Istanbul through
Osaka, plus Amsterdam's environment/scalar/memory fragment. Its two-resource
state/account/call gas remains unsupported.
Calls follow EIP-150/214/2929; creation follows EIP-161/170/684/1014/2681/3541/3860.
https://eips.ethereum.org/EIPS/eip-150
https://eips.ethereum.org/EIPS/eip-2929 -/

namespace SolarTV.EVM.State
open EvmRules

inductive Step where
  | next (world : Journal) (frame : Frame)
  | done (result : Result)
  | call (message : Calls.Message) (outOffset outSize requested : Nat)
      (code : ExecutionCode) (world : Journal) (frame : Frame)
  | create (salt : Option Word) (value : Word) (initcode : List Byte)
      (world : Journal) (frame : Frame)

abbrev Action := Except String Step

def stepFailed (world : Journal) : Action := .ok (.done ⟨.exception, [], world, 0, 0⟩)

def stepExhausted (world : Journal) : Action := .ok (.done ⟨.outOfGas, [], world, 0, 0⟩)

def next (world : Journal) (frame : Frame) (stack : List Word) : Action :=
  .ok (.next world {frame with pc := frame.pc + 1, stack})

def available (cfg : Config) (opcode : Nat) : Opcode :=
  cfg.opcodes[opcode]?.getD ⟨false, 0, 0, 0, 0⟩

/-- Charge expansion before allocating host memory. Natural address sums do not wrap. -/
def withMemory (world : Journal) (frame : Frame) (charge offset count : Nat)
    (continuation : Frame → Action) : Action :=
  let total := charge + memoryCharge frame.memory offset count
  if frame.gas < total then stepExhausted world else do
    let memory ← expand frame.memory offset count
    continuation {frame with memory, gas := frame.gas - total}

def instruction (cfg : Config) (world : Journal) (frame : Frame) : Action :=
  if frame.pc ≥ frame.code.length then .ok (.done ⟨.success, [], world, frame.gas, 0⟩) else
  let opcode := frame.code[frame.pc]?.getD 0 |>.toNat
  let info := available cfg opcode
  if !info.available then stepFailed world
  -- Amsterdam's environment/memory fragment is admitted independently of the
  -- two-resource gas model. Account, storage and call operations remain errors.
  else if cfg.fork.atLeast .amsterdam &&
      ([0x31,0x3b,0x3c,0x3f,0x47,0x54,0x55,0x5c,0x5d,0xf0,0xf1,0xf2,0xf4,0xf5,0xfa,0xff].contains opcode ||
        (0xa0 ≤ opcode && opcode ≤ 0xa4)) then
    .error "Amsterdam state/account/call gas is not modeled"
  else if frame.stack.length < info.inputs || frame.stack.length - info.inputs + info.outputs > 1024 then
    stepFailed world
  else if let some op := pureOp opcode then
    let dynamic := if opcode = 0x0a then
      info.dynamic * ((256 - (frame.stack[1]?.getD 0).clz.toNat + 7) / 8) else 0
    if frame.gas < info.base + dynamic then stepExhausted world else
    match EVM.step ⟨BitVec.ofNat 256 frame.environment.input.length,
        BitVec.ofNat 256 frame.returnData.length⟩ op frame.stack with
    | none => stepFailed world
    | some stack => next world {frame with gas := frame.gas - info.base - dynamic} stack
  else if 0x60 ≤ opcode ∧ opcode ≤ 0x7f then
    if frame.gas < info.base then stepExhausted world else
      .ok (.next world {frame with
        pc := frame.pc + 1 + (opcode - 0x5f)
        stack := pushValue frame.code frame.pc (opcode - 0x5f) :: frame.stack
        gas := frame.gas - info.base})
  else if 0x80 ≤ opcode ∧ opcode ≤ 0x9f then
    let op := if opcode < 0x90 then EVM.Op.dup ⟨opcode - 0x80, by omega⟩
      else EVM.Op.swap ⟨opcode - 0x90, by omega⟩
    if frame.gas < info.base then stepExhausted world else
      match EVM.step ⟨0, 0⟩ op frame.stack with
      | none => stepFailed world
      | some stack => next world {frame with gas := frame.gas - info.base} stack
  else
    let values := frame.stack.take info.inputs
    let rest := frame.stack.drop info.inputs
    let a := values[0]?.getD 0
    let b := values[1]?.getD 0
    let c := values[2]?.getD 0
    match opcode with
    | 0x00 => .ok (.done ⟨.success, [], world, frame.gas, 0⟩)
    | 0xfe => stepFailed world
    | 0x40 | 0x41 | 0x42 | 0x43 | 0x44 | 0x45 | 0x46 | 0x48 | 0x49 | 0x4a | 0x4b =>
      if frame.gas < info.base then stepExhausted world
      else next world {frame with gas := frame.gas - info.base}
        (environmentValue cfg opcode a :: rest)
    | 0x20 => withMemory world frame (info.base + info.dynamic * ((b.toNat + 31) / 32)) a.toNat b.toNat fun frame =>
          next world frame (Keccak.hash (readBytes frame.memory a.toNat b.toNat) :: rest)
    | 0x30 | 0x32 | 0x33 | 0x34 | 0x36 | 0x38 | 0x3a | 0x3d | 0x47 | 0x58 | 0x59 | 0x5a | 0x5f =>
      if frame.gas < info.base then stepExhausted world else
        let value := match opcode with
          | 0x30 => frame.environment.address.setWidth 256
          | 0x32 => frame.environment.origin.setWidth 256
          | 0x33 => frame.environment.caller.setWidth 256
          | 0x34 => frame.environment.value
          | 0x36 => BitVec.ofNat 256 frame.environment.input.length
          | 0x38 => BitVec.ofNat 256 frame.code.length
          | 0x3a => frame.environment.gasPrice
          | 0x3d => BitVec.ofNat 256 frame.returnData.length
          | 0x47 => (account world frame.environment.address).balance
          | 0x58 => BitVec.ofNat 256 frame.pc
          | 0x59 => BitVec.ofNat 256 frame.memory.length
          | 0x5a => BitVec.ofNat 256 (frame.gas - info.base)
          | _ => 0
        next world {frame with gas := frame.gas - info.base} (value :: rest)
    | 0x31 | 0x3b | 0x3f =>
      let address := a.setWidth 160
      if opcode != 0x31 && cfg.unknownCode == some address then
        .error "MIR root runtime code is unknown" else
      let charge := if cfg.fork.atLeast .berlin && world.warmAddresses.contains address then 100 else info.base
      if frame.gas < charge then stepExhausted world else
        let value := if opcode = 0x31 then (account world address).balance
          else if opcode = 0x3b then BitVec.ofNat 256 (account world address).code.length
          else if world.accounts.any (fun a => a.address == address) then Keccak.hash (account world address).code else 0
        next (warmAddress world address) {frame with gas := frame.gas - charge} (value :: rest)
    | 0x35 =>
      if frame.gas < info.base then stepExhausted world
      else next world {frame with gas := frame.gas - info.base}
        (readWord frame.environment.input a.toNat :: rest)
    | 0x3c =>
      let owner := a.setWidth 160
      if cfg.unknownCode == some owner then .error "MIR root runtime code is unknown" else
      let count := (values[3]?.getD 0).toNat
      let base := if cfg.fork.atLeast .berlin && world.warmAddresses.contains owner then 100 else info.base
      -- The EXTCODE gas class reflects account access; copying also pays the
      -- same per-word price as CALLDATACOPY (Yellow Paper W_copy).
      let copyWord := (available cfg 0x37).dynamic
      withMemory world frame (base + copyWord * ((count+31)/32)) b.toNat count fun frame =>
        let bytes := readBytes (account world owner).code c.toNat count
        next (warmAddress world owner) {frame with memory := writeBytes frame.memory b.toNat bytes} rest
    | 0x37 | 0x39 | 0x3e =>
      if opcode = 0x3e && b.toNat + c.toNat > frame.returnData.length then stepFailed world
      else withMemory world frame (info.base + info.dynamic * ((c.toNat + 31) / 32)) a.toNat c.toNat fun frame =>
        let source := if opcode = 0x37 then frame.environment.input
          else if opcode = 0x39 then frame.code else frame.returnData
        let memory := writeBytes frame.memory a.toNat (readBytes source b.toNat c.toNat)
        next world {frame with memory} rest
    | 0x50 =>
      if frame.gas < info.base then stepExhausted world
      else next world {frame with gas := frame.gas - info.base} rest
    | 0x51 | 0x52 | 0x53 => withMemory world frame info.base a.toNat (if opcode = 0x53 then 1 else 32) fun frame =>
      if opcode = 0x51 then next world frame (readWord frame.memory a.toNat :: rest)
      else
        let bytes := if opcode = 0x53 then [b.setWidth 8] else wordBytes b
        next world {frame with memory := writeBytes frame.memory a.toNat bytes} rest
    | 0x54 | 0x55 | 0x5c | 0x5d =>
      let owner := frame.environment.address
      let old := account world owner
      let transient := opcode = 0x5c || opcode = 0x5d
      let store := opcode = 0x55 || opcode = 0x5d
      let current := slotsRead (if transient then old.transient else old.storage) a
      let original := slotsRead ((world.original.find? (fun x => x.address == owner)).getD {address := owner}).storage a
      let price := if transient then Storage.Price.mk 100 0
        else if store then Storage.writePrice cfg.fork (world.warmSlots.contains (owner, a)) original current b
        else Storage.Price.mk (Storage.readPrice cfg.fork (world.warmSlots.contains (owner, a))) 0
      if store && frame.environment.isStatic then stepFailed world
      else if opcode = 0x55 && cfg.fork.atLeast .istanbul && frame.gas ≤ 2300 then stepExhausted world
      else if frame.gas < price.gas then stepExhausted world else
        let world := if transient then world else warmSlot world owner a
        let world := if !store then world else
          putAccount {world with refund := world.refund + price.refund}
            (if transient then {old with transient := slotsWrite old.transient a b}
             else {old with storage := slotsWrite old.storage a b})
        next world {frame with gas := frame.gas - price.gas} (if store then rest else current :: rest)
    | 0x56 | 0x57 =>
      if frame.gas < info.base then stepExhausted world
      else if opcode = 0x57 && b == 0 then next world {frame with gas := frame.gas - info.base} rest
      else if !validJump frame.code a then stepFailed world
      else .ok (.next world {frame with stack := rest, pc := a.toNat, gas := frame.gas - info.base})
    | 0x5b =>
      if frame.gas < info.base then stepExhausted world
      else next world {frame with gas := frame.gas - info.base} rest
    | 0x5e =>
      let charge := info.base + info.dynamic * ((c.toNat + 31) / 32)
      let sourceGrowth := memoryCost (max (requiredWords frame.memory b.toNat c.toNat)
        (requiredWords frame.memory a.toNat c.toNat)) - memoryCost (requiredWords frame.memory a.toNat c.toNat)
      withMemory world frame (charge + sourceGrowth) a.toNat c.toNat fun frame => do
        let memory ← expand frame.memory b.toNat c.toNat
        let bytes := readBytes memory b.toNat c.toNat
        next world {frame with memory := writeBytes memory a.toNat bytes} rest
    | 0xf3 | 0xfd => withMemory world frame info.base a.toNat b.toNat fun frame =>
      .ok (.done ⟨if opcode = 0xfd then .reverted else .success,
        readBytes frame.memory a.toNat b.toNat, world, frame.gas, 0⟩)
    | 0xf1 | 0xf2 | 0xf4 | 0xfa =>
      let kind := if opcode = 0xf1 then Calls.Kind.call else if opcode = 0xf2 then .callcode
        else if opcode = 0xf4 then .delegatecall else .staticcall
      let explicitValue := opcode = 0xf1 || opcode = 0xf2
      let value := if explicitValue then c else 0
      let inputOffset := (values[if explicitValue then 3 else 2]?.getD 0).toNat
      let inputSize := (values[if explicitValue then 4 else 3]?.getD 0).toNat
      let outOffset := (values[if explicitValue then 5 else 4]?.getD 0).toNat
      let outSize := (values[if explicitValue then 6 else 5]?.getD 0).toNat
      let destination := b.setWidth 160
      let base := if cfg.fork.atLeast .berlin && world.warmAddresses.contains destination then 100 else info.base
      let valueCharge := if explicitValue && value != 0 then 9000 else 0
      let newAccount := if kind == .call && dead world destination &&
          (!cfg.fork.atLeast .spuriousDragon || value != 0) then 25000 else 0
      let (code, resolution, entered) := executionCode cfg.fork world destination
      if cfg.unknownCode == some code.address then .error "MIR root runtime code is unknown" else
      if frame.environment.isStatic && kind == .call && value != 0 then stepFailed world else
        withMemory world frame (base + valueCharge + newAccount + resolution) inputOffset inputSize fun frame =>
          withMemory world frame 0 outOffset outSize fun frame =>
            .ok (.call ⟨kind, destination, value, readBytes frame.memory inputOffset inputSize⟩
              outOffset outSize a.toNat code entered {frame with stack := rest, pc := frame.pc + 1})
    | 0xf0 | 0xf5 =>
      let size := c.toNat
      if frame.environment.isStatic then stepFailed world
      else if cfg.fork.atLeast .shanghai && size > 49152 then stepExhausted world
      else
        let perWord := (if cfg.fork.atLeast .shanghai then 2 else 0) + if opcode = 0xf5 then 6 else 0
        withMemory world frame (info.base + perWord * ((size + 31) / 32)) b.toNat size fun frame =>
          .ok (.create (if opcode = 0xf5 then some (values[3]?.getD 0) else none) a
            (readBytes frame.memory b.toNat size) world {frame with stack := rest, pc := frame.pc + 1})
    | 0xff =>
      let beneficiary := a.setWidth 160
      let charge := destructionPrice cfg.fork world frame.environment.address beneficiary
      if frame.environment.isStatic then stepFailed world
      else if frame.gas < charge then stepExhausted world
      else .ok (.done ⟨.success, [], destroy cfg.fork world frame.environment.address beneficiary,
        frame.gas - charge, 0⟩)
    | opcode =>
      if 0xa0 ≤ opcode ∧ opcode ≤ 0xa4 then
        if frame.environment.isStatic then stepFailed world else
          withMemory world frame (info.base + info.dynamic * b.toNat) a.toNat b.toNat fun frame =>
            let log := World.Log.mk frame.environment.address (values.drop 2) (readBytes frame.memory a.toNat b.toNat)
            next {world with logs := world.logs ++ [log]} frame rest
      else .error s!"unmodeled world opcode {opcode}"

/-- Work fuel bounds the evaluator, not the EVM. Exhaustion is unsupported. -/
def run (fuel : Nat) (cfg : Config) (world : Journal) (frame : Frame) : Run :=
  match fuel with
  | 0 => .error "world instruction/depth evaluation limit"
  | fuel + 1 => do
    let result ← match ← instruction cfg world frame with
      | .done result => pure result
      | .next world frame => run fuel cfg world frame
      | .call message outOffset outSize requested code world frame =>
        let checkpoint := warmAddress world message.destination
        if frame.environment.depth ≥ 1024 ||
            ((message.kind == .call || message.kind == .callcode) &&
              (account world frame.environment.address).balance.ult message.value) then
          -- A rejected value-bearing call returns its unspent stipend too.
          run fuel cfg checkpoint {frame with
            stack := 0 :: frame.stack
            returnData := []
            gas := frame.gas + if Calls.hasValue message then 2300 else 0}
        else
          match Calls.forwardGas cfg.fork frame.gas requested (Calls.hasValue message) with
          | none => exhausted checkpoint
          | some budget =>
            let child ← executeCall (run fuel cfg) cfg checkpoint frame.environment message code budget.supplied
            let committed := child.world
            let data := child.data
            let returned := child.gas
            let copied := data.take outSize
            let resumed ← run (fuel - child.work) cfg committed {frame with
              stack := child.value :: frame.stack
              memory := writeBytes frame.memory outOffset copied
              returnData := data
              gas := frame.gas - budget.charged + returned}
            pure {resumed with work := resumed.work + child.work}
      | .create salt value initcode world frame =>
        let creator := account world frame.environment.address
        if frame.environment.depth ≥ 1024 || creator.balance.ult value || creator.nonce ≥ 2^64 - 1 then
          run fuel cfg world {frame with stack := 0 :: frame.stack, returnData := []}
        else
          let forwarded := if cfg.fork.atLeast .tangerineWhistle then frame.gas - frame.gas / 64 else frame.gas
          let child ← executeCreation (run fuel cfg) cfg world frame.environment salt value initcode forwarded
          let resumed ← run (fuel - child.work) cfg child.world {frame with
            stack := child.value :: frame.stack
            returnData := child.data
            gas := frame.gas - forwarded + child.gas}
          pure {resumed with work := resumed.work + child.work}
    return {result with work := result.work + 1}
termination_by fuel
decreasing_by all_goals omega

/-- Single-step equations keep symbolic execution from unfolding unknown continuations. -/
theorem run_next (fuel : Nat) {cfg world frame nextWorld nextFrame}
    (h : instruction cfg world frame = .ok (.next nextWorld nextFrame)) :
    run (fuel + 1) cfg world frame = (do
      let result ← run fuel cfg nextWorld nextFrame
      return {result with work := result.work + 1}) := by
  simp only [run, h, Except.bind, Bind.bind, Except.pure, Pure.pure]

theorem run_done (fuel : Nat) {cfg world frame result}
    (h : instruction cfg world frame = .ok (.done result)) :
    run (fuel + 1) cfg world frame = .ok {result with work := result.work + 1} := by
  simp only [run, h, Except.bind, Bind.bind, Except.pure, Pure.pure]

theorem run_error (fuel : Nat) {cfg world frame reason}
    (h : instruction cfg world frame = .error reason) :
    run (fuel + 1) cfg world frame = .error reason := by
  simp only [run, h, Except.bind, Bind.bind]

/-- Frame rollback keeps parent-side warming/nonce changes in the checkpoint. -/
def execute (fuel : Nat) (cfg : Config) (world : Journal) (frame : Frame) : Run := do
  let result ← run fuel cfg world frame
  return if result.stop = .success then {result with world := finalize result.world} else
    {result with world, gas := if result.stop = .exception || result.stop = .outOfGas then 0 else result.gas}

end SolarTV.EVM.State
