import SolarTV.MIR.State.Normalize
import SolarTV.EVMIR.State.BudgetedPath
import SolarTV.State.MemoryLaws

/-! A backend certificate for the external Entry.echo ABI path. -/
namespace SolarTV.AbiBytecode
open EvmRules Execution

private def literal (word : Word) : MIR.State.Operand :=
  ⟨.constant ⟨256, word⟩, .word 256⟩

private def localWord (id : Nat) (width : Nat := 256) : MIR.State.Operand :=
  ⟨.local id, .word width⟩

private def reject : MIR.State.Block := ⟨[], [], .reverted (literal 0) (literal 0)⟩

/-- The independently imported module immediately after `lower-evm-shaped`. -/
def firstLowered : MIR.State.Functions := [
  (0, ⟨[], [], [
    ⟨[], [
      ⟨some 0, .word 256, some 0x34, .context (.environment 0x34), []⟩,
      ⟨some 2, .word 1, none, .notEqual, [localWord 0, literal 0]⟩],
      .branch (localWord 2 1) 3 1⟩,
    ⟨[], [
      ⟨some 4, .word 256, some 0x35, .memory .calldataLoad, [literal 0]⟩,
      ⟨some 6, .word 256, some 0x1c, .shift .shr 256, [literal 224, localWord 4]⟩],
      .switch (localWord 6) 3 [(literal 0x8f51edf8, 2)]⟩,
    ⟨[], [], .tailCall 1 []⟩,
    reject], some "entry"⟩),
  (1, ⟨[], [], [
    ⟨[], [
      ⟨some 0, .word 256, some 0x36, .memory .calldataSize, []⟩,
      ⟨some 2, .word 1, some 0x10, .comparison .lt 256, [localWord 0, literal 68]⟩,
      ⟨some 4, .word 256, some 0x35, .memory .calldataLoad, [literal 4]⟩,
      ⟨some 6, .word 1, some 0x11, .comparison .gt 256, [localWord 4, literal 1]⟩,
      ⟨some 7, .word 1, some 0x17, .binary .or 1, [localWord 2 1, localWord 6 1]⟩],
      .branch (localWord 7 1) 3 1⟩,
    ⟨[], [
      ⟨some 9, .word 256, some 0x35, .memory .calldataLoad, [literal 36]⟩,
      ⟨some 11, .word 1, none, .notEqual, [localWord 4, literal 0]⟩],
      .branch (localWord 11 1) 2 3⟩,
    ⟨[], [⟨none, .word 0, some 0x52, .memory .store, [literal 0, localWord 9]⟩],
      .returndata (literal 0) (literal 32)⟩,
    reject], some "echo"⟩)]

def image : List Byte := [
  0x34, 0x60, 0x31, 0x57, 0x5f, 0x35, 0x60, 0xe0, 0x1c, 0x80, 0x63, 0x8f,
  0x51, 0xed, 0xf8, 0x03, 0x60, 0x31, 0x57, 0x60, 0x44, 0x36, 0x10, 0x60,
  0x01, 0x60, 0x04, 0x35, 0x11, 0x17, 0x60, 0x31, 0x57, 0x60, 0x04, 0x35,
  0x15, 0x60, 0x31, 0x57, 0x60, 0x24, 0x35, 0x5f, 0x52, 0x60, 0x20, 0x5f,
  0xf3, 0x5b, 0x5f, 0x5f, 0xfd]

private def code : EVM.State.CodeChunks :=
  .leaf ⟨image, 53, by decide, 38, [49], by decide⟩

private def selector (env : World.Environment) : Word := readWord env.input 0 >>> (224 : Nat)
private def flag (env : World.Environment) : Word := readWord env.input 4
private def size (env : World.Environment) : Word := BitVec.ofNat 256 env.input.length
private def payload (env : World.Environment) : List Byte := wordBytes (readWord env.input 36)

private def short (env : World.Environment) : Bool := (size env).ult 68#256
private def dirty (env : World.Environment) : Bool := (1#256).ult (flag env)

/-- Least frame gas at which the terminal instruction executes. -/
def needed (env : World.Environment) : Nat :=
  if env.value ≠ 0#256 then 20
  else if selector env ≠ 0x8f51edf8#256 then 53
  else if short env || dirty env then 89
  else if flag env = 0#256 then 111
  else 125

private def completion (env : World.Environment) : MIR.State.Completion :=
  if env.value ≠ 0#256 ∨ selector env ≠ 0x8f51edf8#256 ∨ (short env || dirty env) = true ∨
      flag env = 0#256 then .reverted []
  else .success (payload env)

private def mirObservation (result : MIR.State.Result) :
    MIR.State.Completion × EVM.State.Journal := (result.completion, result.world)

private theorem readPayload (word : Word) :
    readBytes (wordBytes word) 0 32 = wordBytes word := by
  simpa only [wordBytes_length] using readBytes_entire (wordBytes word)

private theorem storePayload (memory : List Byte) (word : Word) (length : memory.length = 32) :
    writeBytes memory 0 (wordBytes word) = wordBytes word := by
  have dropped : memory.drop 32 = [] := by rw [← length, List.drop_length]
  simp only [writeBytes, List.take_zero, Nat.sub_zero, List.nil_append, Nat.zero_add,
    wordBytes_length, dropped, List.append_nil, length]
  apply List.take_of_length_le
  simp only [wordBytes_length, Nat.le_refl]

private theorem expandPayload (word : Word) :
    expand (wordBytes word) 0 32 = .ok (wordBytes word) := by
  simp [expand, requiredWords, memoryWords, wordBytes_length]


private theorem expandEmpty : expand [] 0 0 = .ok [] := rfl

private theorem expandWord : expand [] 0 32 = .ok (List.replicate 32 0) := rfl

private def mirConfig (env : World.Environment) : MIR.State.Config :=
  {fork := .cancun, environment := env, context := {},
   available := [0x34,0x35,0x36,0x1c,0x10,0x11,0x17,0x52], entry := true}

private def mirFrame (journal : EVM.State.Journal) : MIR.State.Frame :=
  {world := journal, memory := [], arguments := []}

def entryId : Nat := 0

def executeMir (functions : MIR.State.Functions) (entry : Nat) (env : World.Environment)
    (journal : EVM.State.Journal) : MIR.State.Run :=
  MIR.State.execute 32 (mirConfig env) functions entry (mirFrame journal)

local macro "reduceMirStep" : conv =>
  `(conv| simp [-List.reduceReplicate, ↓MIR.State.reduceShapedRun, MIR.State.advance, MIR.State.branch,
    mirConfig, mirFrame, reject, literal, localWord, MIR.State.put, MIR.State.Scalar.ofWord, MIR.State.choose,
    MIR.State.next, MIR.State.start, ↓MIR.State.reduceFunction,
    MIR.State.required, MIR.State.read, MIR.State.readValue, MIR.State.Value.word,
    ↓MIR.State.reduceFits, MIR.State.typedWords, MIR.compare, MIR.eval, SolarTV.Word.shift,
    MIR.State.wordBits, MIR.State.widenBits, MIR.State.ValueType.width,
    expandEmpty, expandWord, storePayload, expandPayload, readPayload, wordBytes_length,
    Bind.bind, Except.bind, Pure.pure, Except.pure, *])

local macro "reduceMirGuard" : conv =>
  `(conv| simp [-List.reduceReplicate, MIR.State.transition, ↓MIR.State.reduceFunction, MIR.State.required, reject, literal, localWord,
    MIR.State.read, MIR.State.readValue, MIR.State.readBits, MIR.State.Value.word,
    ↓MIR.State.reduceFits, MIR.State.choose, MIR.State.next, MIR.State.branch,
    MIR.State.stop, MIR.State.finish, MIR.State.instruction, MIR.State.operation,
    MIR.State.memoryOperation, MIR.State.assign, MIR.State.effect,
    expandEmpty, expandWord, expandPayload, readPayload, storePayload, wordBytes_length,
    MIR.State.wordBits, MIR.State.widenBits, MIR.State.ValueType.width, MIR.eval,
    Bind.bind, Except.bind, Pure.pure, Except.pure, *])

local macro "checkMirPath" : tactic =>
  `(tactic| (
    simp (config := {failIfUnchanged := false}) only [selector, short, size, dirty, flag] at *
    conv =>
      lhs
      arg 2
      arg 3
      try reduceMirStep
      try reduceMirGuard
      try reduceMirStep
      try reduceMirGuard
      try reduceMirStep
      try reduceMirGuard
      try reduceMirStep
      try reduceMirGuard
    simp [completion, selector, short, size, dirty, flag, MIR.State.conclude, mirFrame,
      mirObservation, payload, Except.map, Bind.bind, Except.bind, Pure.pure, Except.pure, *]
  ))

set_option maxRecDepth 4096 in
set_option maxHeartbeats 2000000 in
private theorem mir_executes (env : World.Environment) (journal : EVM.State.Journal) :
    (executeMir firstLowered entryId env journal).map mirObservation = .ok
      (completion env, match completion env with
        | .success _ => EVM.State.finalize journal
        | _ => journal) := by
  change (MIR.State.conclude (mirConfig env) (mirFrame journal)
    (MIR.State.run 32 (mirConfig env) firstLowered ⟨{function := 0}, [], mirFrame journal⟩)).map
      mirObservation = _
  by_cases noValue : env.value = 0#256
  · by_cases selected : selector env = 0x8f51edf8#256
    · cases tooShort : short env <;> cases malformed : dirty env <;>
        by_cases disabled : flag env = 0#256 <;> checkMirPath
    · have wrongSelector : 0x8f51edf8#256 ≠ selector env := Ne.symm selected
      checkMirPath
  · checkMirPath

open EVM.State EVM.State.BudgetedPath

def priceRows : List (Nat × Opcode) := [
  (0x03, ⟨true, 3, 0, 2, 1⟩),
  (0x10, ⟨true, 3, 0, 2, 1⟩),
  (0x11, ⟨true, 3, 0, 2, 1⟩),
  (0x15, ⟨true, 3, 0, 1, 1⟩),
  (0x17, ⟨true, 3, 0, 2, 1⟩),
  (0x1c, ⟨true, 3, 0, 2, 1⟩),
  (0x34, ⟨true, 2, 0, 0, 1⟩),
  (0x35, ⟨true, 3, 0, 1, 1⟩),
  (0x36, ⟨true, 2, 0, 0, 1⟩),
  (0x52, ⟨true, 3, 0, 2, 0⟩),
  (0x57, ⟨true, 10, 0, 2, 0⟩),
  (0x5b, ⟨true, 1, 0, 0, 0⟩),
  (0x5f, ⟨true, 2, 0, 0, 1⟩),
  (0x60, ⟨true, 3, 0, 0, 1⟩),
  (0x63, ⟨true, 3, 0, 0, 1⟩),
  (0x80, ⟨true, 3, 0, 1, 2⟩),
  (0xf3, ⟨true, 0, 0, 2, 0⟩),
  (0xfd, ⟨true, 0, 0, 2, 0⟩)]

/-- Fork and opcode schema used by the certificate. Callers can bind these rows
    to their Cancun configuration without supplying any facts about execution. -/
def CancunPrices (cfg : Config) : Prop :=
  cfg.fork = .cancun ∧ ∀ opcode row, (opcode, row) ∈ priceRows → available cfg opcode = row

private def frame (env : World.Environment) (pc : Nat) (stack : List Word)
    (memory : List Byte) : Frame := ⟨env, image, pc, stack, memory, [], 0⟩

private theorem imageSize : code.size = 53 := rfl
private theorem revertTarget : 49 ∈ code.jumps 0 := by decide
private def selectorDifference (env : World.Environment) : Word := 0x8f51edf8#256 - selector env
private def badArgs (env : World.Environment) : Word :=
  (if dirty env then 1 else 0) ||| (if short env then 1 else 0)
private def zeroFlag (env : World.Environment) : Word := if flag env = 0#256 then 1 else 0

private theorem badArgs_zero (env : World.Environment) :
    badArgs env = 0#256 ↔ (short env || dirty env) = false := by
  cases a : short env <;> cases b : dirty env <;> simp [badArgs, a, b]

private theorem zeroFlag_zero (env : World.Environment) :
    zeroFlag env = 0#256 ↔ flag env ≠ 0#256 := by
  by_cases h : flag env = 0#256 <;> simp [zeroFlag, h]

private theorem selectorDifference_zero (env : World.Environment) :
    selectorDifference env = 0#256 ↔ selector env = 0x8f51edf8#256 := by
  constructor
  · intro zero
    have same := congrArg (fun word => word + selector env) zero
    simpa [selectorDifference, BitVec.sub_add_cancel] using same.symm
  · intro selected
    simp [selectorDifference, selected]

private def row (opcode : Nat) : Opcode :=
  ((priceRows.find? (fun pair => pair.1 == opcode)).map Prod.snd).getD
    ⟨false, 0, 0, 0, 0⟩

local macro "takeStep " op:num cost:num rest:num after:term : tactic =>
  `(tactic| (
    refine .step (after := $after) (nextWorld := ‹Journal›) (charge := $cost) (rest := $rest) ?_ ?_
    · refine CodeChunks.meteredStep _ _ _ code _ _ $op $cost rfl rfl ?_
      intro balance
      have fork := (‹CancunPrices ‹Config››).1
      have schema := (‹CancunPrices ‹Config››).2 $op (row $op) (by decide)
      simp [instructionWithImage, frame, imageSize, schema, fork, Fork.atLeast,
        pureOp, EVM.step, Evm.shr, Evm.sub, Evm.lt, Evm.gt, Evm.or, Evm.iszero,
        next, withMemory, memoryCharge, memoryCost, requiredWords, memoryWords,
        storePayload, expandPayload, wordBytes_length, readPayload,
        Bind.bind, Except.bind, Pure.pure, Except.pure,
        row, priceRows, revertTarget, *] <;>
      first | rfl | simp [selector, size, flag, payload, selectorDifference, badArgs, zeroFlag,
        short, dirty, storePayload, expandPayload, readPayload, *]
  ))

private theorem valuePrefix (env : World.Environment) (cfg : Config) (journal : Journal)
    (prices : CancunPrices cfg) :
    MeteredPrefix cfg journal (frame env 0x00 [] [])
      journal (frame env 0x03 [49, env.value] []) 2 5 := by
  takeStep 0x34 2 3 (frame env 0x01 [env.value] [])
  takeStep 0x60 3 0 (frame env 0x03 [49, env.value] [])
  exact .done _ _

private theorem selectorPrefix (env : World.Environment) (cfg : Config) (journal : Journal)
    (prices : CancunPrices cfg) (noValue : env.value = 0#256) :
    MeteredPrefix cfg journal (frame env 0x03 [49, env.value] [])
      journal (frame env 0x12 [49, selectorDifference env, selector env] []) 9 33 := by
  takeStep 0x57 10 23 (frame env 0x04 [] [])
  takeStep 0x5f 2 21 (frame env 0x05 [0] [])
  takeStep 0x35 3 18 (frame env 0x06 [readWord env.input 0] [])
  takeStep 0x60 3 15 (frame env 0x08 [224, readWord env.input 0] [])
  takeStep 0x1c 3 12 (frame env 0x09 [selector env] [])
  takeStep 0x80 3 9 (frame env 0x0a [selector env, selector env] [])
  takeStep 0x63 3 6 (frame env 0x0f [0x8f51edf8#256, selector env, selector env] [])
  takeStep 0x03 3 3 (frame env 0x10 [selectorDifference env, selector env] [])
  takeStep 0x60 3 0 (frame env 0x12 [49, selectorDifference env, selector env] [])
  exact .done _ _

private theorem argsPrefix (env : World.Environment) (cfg : Config) (journal : Journal)
    (prices : CancunPrices cfg) (selected : selector env = 0x8f51edf8#256) :
    MeteredPrefix cfg journal (frame env 0x12 [49, selectorDifference env, selector env] [])
      journal (frame env 0x20 [49, badArgs env, selector env] []) 10 36 := by
  have jump : selectorDifference env = 0#256 := (selectorDifference_zero env).2 selected
  takeStep 0x57 10 26 (frame env 0x13 [selector env] [])
  takeStep 0x60 3 23 (frame env 0x15 [68, selector env] [])
  takeStep 0x36 2 21 (frame env 0x16 [size env, 68, selector env] [])
  takeStep 0x10 3 18 (frame env 0x17 [(if short env then 1 else 0), selector env] [])
  takeStep 0x60 3 15 (frame env 0x19 [1, (if short env then 1 else 0), selector env] [])
  takeStep 0x60 3 12 (frame env 0x1b [4, 1, (if short env then 1 else 0), selector env] [])
  takeStep 0x35 3 9 (frame env 0x1c [flag env, 1, (if short env then 1 else 0), selector env] [])
  takeStep 0x11 3 6 (frame env 0x1d [(if dirty env then 1 else 0), (if short env then 1 else 0), selector env] [])
  takeStep 0x17 3 3 (frame env 0x1e [badArgs env, selector env] [])
  takeStep 0x60 3 0 (frame env 0x20 [49, badArgs env, selector env] [])
  exact .done _ _

private theorem flagPrefix (env : World.Environment) (cfg : Config) (journal : Journal)
    (prices : CancunPrices cfg) (goodArgs : (short env || dirty env) = false) :
    MeteredPrefix cfg journal (frame env 0x20 [49, badArgs env, selector env] [])
      journal (frame env 0x27 [49, zeroFlag env, selector env] []) 5 22 := by
  have jump : badArgs env = 0#256 := (badArgs_zero env).2 goodArgs
  takeStep 0x57 10 12 (frame env 0x21 [selector env] [])
  takeStep 0x60 3 9 (frame env 0x23 [4, selector env] [])
  takeStep 0x35 3 6 (frame env 0x24 [flag env, selector env] [])
  takeStep 0x15 3 3 (frame env 0x25 [zeroFlag env, selector env] [])
  takeStep 0x60 3 0 (frame env 0x27 [49, zeroFlag env, selector env] [])
  exact .done _ _

private theorem successPrefix (env : World.Environment) (cfg : Config) (journal : Journal)
    (prices : CancunPrices cfg) (enabled : flag env ≠ 0#256) :
    MeteredPrefix cfg journal (frame env 0x27 [49, zeroFlag env, selector env] [])
      journal (frame env 0x30 [0, 32, selector env] (payload env)) 7 29 := by
  have jump : zeroFlag env = 0#256 := (zeroFlag_zero env).2 enabled
  takeStep 0x57 10 19 (frame env 0x28 [selector env] [])
  takeStep 0x60 3 16 (frame env 0x2a [36, selector env] [])
  takeStep 0x35 3 13 (frame env 0x2b [readWord env.input 36, selector env] [])
  takeStep 0x5f 2 11 (frame env 0x2c [0, readWord env.input 36, selector env] [])
  takeStep 0x52 6 5 (frame env 0x2d [selector env] (payload env))
  takeStep 0x60 3 2 (frame env 0x2f [32, selector env] (payload env))
  takeStep 0x5f 2 0 (frame env 0x30 [0, 32, selector env] (payload env))
  exact .done _ _

private theorem revertBranch (env : World.Environment) (cfg : Config) (journal : Journal)
    (prices : CancunPrices cfg) (pc : Nat) (condition : Word) (stack : List Word)
    (opcode : code.get? pc = some 0x57) (inside : pc < 53) (taken : condition ≠ 0)
    (height : stack.length ≤ 1022) :
    MeteredPrefix cfg journal (frame env pc (49 :: condition :: stack) []) journal
      (frame env 0x31 stack []) 1 10 := by
  refine .step (count := 0) (rest := 0) (charge := 10) ?_ (.done _ _)
  refine CodeChunks.meteredStep _ _ _ code _ _ 0x57 10 rfl opcode ?_
  intro balance
  have schema := prices.2 0x57 ⟨true, 10, 0, 2, 0⟩ (by decide)
  have room : ¬1024 < stack.length := by omega
  have nonzero : condition ≠ 0#256 := taken
  have operands : ¬stack.length + 1 + 1 < 2 := by omega
  simp [instructionWithImage, frame, imageSize, Nat.not_le.mpr inside, schema, prices.1,
    Fork.atLeast, pureOp, revertTarget, nonzero, room, operands]

private theorem revertTail (env : World.Environment) (cfg : Config) (journal : Journal)
    (prices : CancunPrices cfg) (stack : List Word) (height : stack.length ≤ 1022) :
    MeteredPrefix cfg journal (frame env 0x31 stack []) journal
      (frame env 0x34 (0 :: 0 :: stack) []) 3 5 := by
  have room : ¬1024 < stack.length := by omega
  have roomOne : ¬1024 < stack.length + 1 := by omega
  have roomTwo : ¬1024 < stack.length + 2 := by omega
  takeStep 0x5b 1 4 (frame env 0x32 stack [])
  takeStep 0x5f 2 2 (frame env 0x33 (0 :: stack) [])
  takeStep 0x5f 2 0 (frame env 0x34 (0 :: 0 :: stack) [])
  exact .done _ _

private theorem revertExit (env : World.Environment) (cfg : Config) (journal : Journal)
    (prices : CancunPrices cfg) (stack : List Word) (height : stack.length ≤ 1022) :
    Checked cfg journal (frame env 0x34 (0 :: 0 :: stack) []) [⟨0,0⟩] ⟨.reverted,[],journal⟩ := by
  refine .halt (by decide) ?_
  intro balance
  have schema := prices.2 0xfd ⟨true, 0, 0, 2, 0⟩ (by decide)
  refine (code.instruction_eq cfg journal
    {frame env 0x34 (0 :: 0 :: stack) [] with gas := balance}).trans ?_
  simp [instructionWithImage, frame, imageSize, schema, prices.1, Fork.atLeast,
    show code.get? 0x34 = some 0xfd from rfl, pureOp, Exit.result, withMemory, memoryCharge,
    memoryCost, memoryWords, requiredWords, expand, readBytes, Bind.bind, Except.bind]
  omega

private theorem successExit (env : World.Environment) (cfg : Config) (journal : Journal)
    (prices : CancunPrices cfg) :
    Checked cfg journal (frame env 0x30 [0, 32, selector env] (payload env))
      [⟨0,0⟩] ⟨.success,payload env,journal⟩ := by
  refine .halt (by decide) ?_
  intro balance
  have schema := prices.2 0xf3 ⟨true, 0, 0, 2, 0⟩ (by decide)
  refine (code.instruction_eq cfg journal
    {frame env 0x30 [0, 32, selector env] (payload env) with gas := balance}).trans ?_
  simp [instructionWithImage, frame, imageSize, schema, prices.1, Fork.atLeast,
    show code.get? 0x30 = some 0xf3 from rfl, pureOp, Exit.result, withMemory, memoryCharge,
    memoryCost, requiredWords, memoryWords, payload, expandPayload, wordBytes_length, readPayload,
    Bind.bind, Except.bind]

def executeBytes (runtime : List Byte) (cfg : Config) (env : World.Environment) (journal : Journal)
    (fuel gas : Nat) : Run :=
  (EVM.State.execute (fuel + 40) cfg journal {frame env 0 [] [] with code := runtime, gas}).map
    (fun result => {result with work := 0})

private theorem executesPath (cfg : Config) (env : World.Environment) (journal : Journal)
    (fuel gas : Nat) {cut : Frame} {count cost : Nat} {exit : Exit}
    (trace : MeteredPrefix cfg journal (frame env 0 [] []) journal cut count cost)
    (terminal : Checked cfg journal cut [⟨0,0⟩] exit) (within : count + 1 ≤ 40) :
    executeBytes image cfg env journal fuel gas = .ok
      (if gas < cost then ⟨.outOfGas,[],journal,0,0⟩
       else settle journal (exit.result (gas - cost))) := by
  have result := terminal.executeAfterPrefix trace (fuel + (40 - (count + 1))) gas
  simp only [List.length_cons, List.length_nil, Nat.zero_add, required_exact, required_nil,
    charged, List.map_cons, List.map_nil, List.sum_cons, List.sum_nil, Nat.add_zero] at result
  rw [show fuel + (40 - (count + 1)) + (count + 1) = fuel + 40 by omega] at result
  exact result

private theorem executesRevert (cfg : Config) (env : World.Environment) (journal : Journal)
    (prices : CancunPrices cfg) (fuel gas : Nat) {count cost : Nat} (stack : List Word)
    (height : stack.length ≤ 1022) (within : count + 4 ≤ 40)
    (head : MeteredPrefix cfg journal (frame env 0 [] []) journal (frame env 0x31 stack []) count cost) :
    executeBytes image cfg env journal fuel gas = .ok
      (if gas < cost + 5 then ⟨.outOfGas,[],journal,0,0⟩
       else ⟨.reverted,[],journal,gas - (cost + 5),0⟩) := by
  simpa [settle, Exit.result] using
    executesPath cfg env journal fuel gas (head.append (revertTail env cfg journal prices stack height))
      (revertExit env cfg journal prices stack height) (by omega)

private def spend (initial : Journal) (gas cost : Nat) (result : Result) : Result :=
  if gas < cost then ⟨.outOfGas,[],initial,0,0⟩ else {result with gas := gas - cost}

private def metered (initial : Journal) (gas cost : Nat)
    (mir : MIR.State.Completion × Journal) : Run :=
  match mir with
  | (.success data, journal) => .ok (spend initial gas cost ⟨.success,data,journal,0,0⟩)
  | (.reverted data, _) => .ok (spend initial gas cost ⟨.reverted,data,initial,0,0⟩)
  | _ => .error "unexpected first-lowered MIR completion"

/-- Apply the path's bytecode debit to the unpriced MIR observation. -/
def meterMir (initial : Journal) (gas cost : Nat) (mir : MIR.State.Run) : Run :=
  ((mir.map mirObservation).bind (metered initial gas cost))

/-- For this exact runtime and first lowered MIR module, every frame budget
agrees on success, revert, or out-of-gas, including data and the journal. -/
theorem abi_to_bytecode (cfg : Config) (prices : CancunPrices cfg) (env : World.Environment)
    (journal : Journal) (fuel gas : Nat) :
    executeBytes image cfg env journal fuel gas =
      meterMir journal gas (needed env) (executeMir firstLowered entryId env journal) := by
  unfold meterMir
  rw [mir_executes]
  have valueTrace := valuePrefix env cfg journal prices
  by_cases noValue : env.value = 0#256
  · have selectorTrace := valueTrace.append (selectorPrefix env cfg journal prices noValue)
    by_cases selected : selector env = 0x8f51edf8#256
    · have argsTrace := selectorTrace.append (argsPrefix env cfg journal prices selected)
      cases goodArgs : short env || dirty env with
      | true =>
        have taken : badArgs env ≠ 0#256 := by
          intro zero
          have rejected := (badArgs_zero env).1 zero
          rw [goodArgs] at rejected
          contradiction
        have trace := argsTrace.append
          (revertBranch env cfg journal prices 0x20 _ [selector env] rfl (by decide) taken (by simp))
        simpa [needed, completion, metered, spend, noValue, selected, goodArgs, Except.bind] using
          executesRevert cfg env journal prices fuel gas [selector env] (by simp) (by decide) trace
      | false =>
        have flagTrace := argsTrace.append (flagPrefix env cfg journal prices goodArgs)
        by_cases disabled : flag env = 0#256
        · have taken : zeroFlag env ≠ 0#256 := by simp [zeroFlag, disabled]
          have trace := flagTrace.append
            (revertBranch env cfg journal prices 0x27 _ [selector env] rfl (by decide) taken (by simp))
          simpa [needed, completion, metered, spend, noValue, selected, goodArgs, disabled, Except.bind] using
            executesRevert cfg env journal prices fuel gas [selector env] (by simp) (by decide) trace
        · have trace := flagTrace.append (successPrefix env cfg journal prices disabled)
          simpa [needed, completion, metered, spend, settle, Exit.result, noValue, selected, goodArgs,
            disabled, Except.bind] using
            executesPath cfg env journal fuel gas trace (successExit env cfg journal prices) (by decide)
    · have taken : selectorDifference env ≠ 0#256 := fun zero => selected ((selectorDifference_zero env).1 zero)
      have trace := selectorTrace.append
        (revertBranch env cfg journal prices 0x12 _ [selector env] rfl (by decide) taken (by simp))
      simpa [needed, completion, metered, spend, noValue, selected, Except.bind] using
        executesRevert cfg env journal prices fuel gas [selector env] (by simp) (by decide) trace
  · have trace := valueTrace.append
      (revertBranch env cfg journal prices 0x03 _ [] rfl (by decide) noValue (by simp))
    simpa [needed, completion, metered, spend, noValue, Except.bind] using
      executesRevert cfg env journal prices fuel gas [] (by decide) (by decide) trace

end SolarTV.AbiBytecode
