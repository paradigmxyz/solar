"""Check the emitted authorization deadline guard at both successors."""

from . import decoder_proof, metered_prefix, mir_state
from .callers import require


def source(runtime, decoded, catalog, layout):
    points = list(decoded)
    start = points.index(layout.pc)
    steps = points[start : start + 6]
    ops = [decoded[pc] for pc in steps]
    require(
        [op for op, _ in ops[:5]] == ["timestamp", "dup2", "lt", "push2", "jumpi"]
        and len(ops) == 6,
        "unexpected authorization deadline guard",
    )
    failure = ops[3][1]
    success = steps[-1]
    gas = [catalog[runtime[pc]]["forks"]["cancun"]["base_gas"] for pc in steps[:5]]
    fees = [sum(gas[i:]) for i in range(6)]
    start_stack = decoder_proof.words(layout.stack)
    memory = decoder_proof.words(layout.memory)
    stacks = [
        start_stack,
        "[timestamp] ++ " + start_stack,
        "[deadline env,timestamp] ++ " + start_stack,
        "[expiry env] ++ " + start_stack,
        f"[{failure},expiry env] ++ " + start_stack,
        start_stack,
    ]
    out = f"""
open SolarTV.Execution

def timestamp : Word := EVM.State.environmentValue tvWorldConfig 0x42 0

def deadline (env : World.Environment) : Word := readWord env.input 132
def expiry (env : World.Environment) : Word := Evm.lt (deadline env) timestamp

def beforeDeadline (env : World.Environment) (surplus : Nat) : EVM.State.Frame :=
  decoderFrame env (surplus+{fees[0]}) {layout.pc} {start_stack} {memory}

def afterDeadline (env : World.Environment) (surplus : Nat) : EVM.State.Frame :=
  decoderFrame env surplus (if expiry env = 0 then {success} else {failure}) {start_stack} {memory}
"""
    args = "(env : World.Environment) (journal : Journal) (surplus : Nat)"
    debits = []
    for i, pc in enumerate(steps[:-1]):
        opcode = runtime[pc]
        spec = catalog[opcode]
        fields = spec["forks"]["cancun"]
        schema = ", ".join(
            str(x).lower()
            for x in (
                fields["available"],
                fields["base_gas"],
                fields["dynamic_gas"],
                *spec["stack"],
            )
        )
        before = f"decoderFrame env 0 {pc} ({stacks[i]}) {memory}"
        after = f"decoderFrame env 0 {steps[i + 1]} ({stacks[i + 1]}) {memory}"
        if i == 4:
            after = "afterDeadline env 0"
        name = f"deadlineStep{i}"
        out += metered_prefix.debit(name, args, before, after, opcode, gas[i])
        out += f"""
  have schema : EVM.State.available tvWorldConfig {opcode} = ⟨{schema}⟩ := rfl
"""
        facts = [
            "schema",
            "unpaid",
            "dispatch_size",
            "EVM.State.instructionWithImage",
            "decoderFrame",
        ]
        if i == 0:
            facts.append("timestamp")
        if i == 1:
            facts.append("deadline")
        if i == 2:
            facts += ["expiry", "deadline"]
        if i == 3:
            out += f"  have value : code.immediate {pc} 2 = {failure} := by decide +kernel\n"
            facts.append("value")
        if i == 4:
            out += f"""  have jumps : {failure} ∈ code.jumps 0 := by decide +kernel
  by_cases unpaid : balance < {gas[i]} <;> by_cases expired : expiry env = 0#256 <;>
    simp [EVM.step,EVM.State.pureOp,EVM.State.next,afterDeadline,decoderFrame,
    jumps,expired,{",".join(facts)}]
"""
        else:
            out += f"""  by_cases unpaid : balance < {gas[i]} <;>
    simp [← EVM.State.CodeChunks.immediate_eq,EVM.State.pureOp,EVM.State.next,EVM.step,
    {",".join(facts)}]
"""
        if i == 2:
            out += "  all_goals rfl\n"
        debits.append(metered_prefix.Debit(name, pc, opcode, gas[i], after))
    out += metered_prefix.certify(
        "metered_deadline",
        args.replace(" (surplus : Nat)", ""),
        "env journal 0",
        fees[0],
        f"decoderFrame env 0 {layout.pc} {start_stack} {memory}",
        "afterDeadline env 0",
        debits,
    )
    out += f"""
theorem deadlineGuard {args} (work : Nat) :
    EVM.State.run (work+5) tvWorldConfig journal (beforeDeadline env surplus) = (do
      let result ← EVM.State.run work tvWorldConfig journal (afterDeadline env surplus)
      return {{result with work := result.work+5}}) := by
  have exactPrice := (metered_deadline env journal).paid work (surplus+{fees[0]}) (by omega)
  simpa only [beforeDeadline,afterDeadline,decoderFrame,Nat.add_sub_cancel] using exactPrice

#print axioms deadlineGuard
"""
    return out, success, failure, fees[0]


MIR_DEFINITIONS = """
open SolarTV.Execution MIR.State MIR.TransferFromMarket
def authWords (env : World.Environment) : List Word :=
  [0,0,288,0,readWord env.input 4,readWord env.input 36,readWord env.input 68,
    readWord env.input 100,readWord env.input 132]
def mirAvailable : List Nat := [0x42,0x51,0x10,0x14,0x54,0x55]
theorem one (cfg : MIR.State.Config) (fs : Functions) (ticks : Nat) (initial reached : Machine)
    (step : transition ticks cfg fs initial = .ok (.next 0 reached)) :
    mirStepsFrom cfg fs initial 1 (ticks+1) = .ok (.inr reached) := by
  simp only [mirStepsFrom,Execution.Trace.segment,Nat.add_sub_cancel,step,
    Bind.bind,Except.bind,Pure.pure,Except.pure,Nat.succ_ne_zero,ite_false]
def guardCfg (env : World.Environment) : MIR.State.Config :=
  {fork := .cancun, environment := env, context := tvWorldConfig.block, available := mirAvailable, entry := false}
"""


def mir_fragments(before, after, catalog, *, success, failure):
    for side, func in zip(("before", "after"), (before, after), strict=True):
        body = func["blocks"][0]
        z = body["terminator"]
        require(
            len(body["instructions"]) == 5 and z["kind"] == "branch",
            "unexpected MIR authorization deadline guard",
        )
        timestamp, pointer, deadline, expired, accepted = [
            instruction["result"] for instruction in body["instructions"]
        ]
        entry = func["id"]
        block = mir_state.block(body, catalog)
        yield f"""
open SolarTV.Execution MIR.State MIR.TransferFromMarket MIR.TransferFromABI

def {side}AuthBody : Function :=
  (MIR.State.function {side}AuthFunctions {entry}).toOption.getD ⟨[],[],[],none⟩
theorem {side}AuthLookup : MIR.State.function {side}AuthFunctions {entry} = .ok {side}AuthBody := rfl
theorem {side}AuthGuard : {side}AuthBody.blocks[0]? = some {block} := rfl

def {side}AfterRead (env : World.Environment) (journal : Journal) : Machine :=
  ⟨{{function := {entry},entered := true,next := 3}},[],
    {{(authorizationFrame env journal) with locals :=
      [({deadline},scalar 256 (deadline env)),({pointer},scalar 256 256),({timestamp},scalar 256 timestamp)]}}⟩

def {side}AfterGuard (env : World.Environment) (journal : Journal) : Machine :=
  let machine := {side}AfterRead env journal
  let expired := if expiry env = 0#256 then 0 else 1
  let allowed := if expiry env = 0#256 then 1 else 0
  ⟨{{function := {entry},block := if expiry env = 0#256 then {z["then"]} else {z["else"]},
     predecessor := some 0}},[],{{machine.frame with locals :=
      ({accepted},scalar 1 allowed) :: ({expired},scalar 1 expired) :: machine.frame.locals}}⟩

def {side}Opened (env : World.Environment) (journal : Journal) : Machine :=
  ⟨{{function := {entry},entered := true}},[],authorizationFrame env journal⟩

def {side}Sampled (env : World.Environment) (journal : Journal) : Machine :=
  ⟨{{function := {entry},entered := true,next := 1}},[],
    {{(authorizationFrame env journal) with locals := [({timestamp},scalar 256 timestamp)]}}⟩

def {side}Pointer (env : World.Environment) (journal : Journal) : Machine :=
  ⟨{{function := {entry},entered := true,next := 2}},[],
    {{(authorizationFrame env journal) with locals := [({pointer},scalar 256 256),({timestamp},scalar 256 timestamp)]}}⟩

theorem {side}Open (env : World.Environment) (journal : Journal) (ticks : Nat) :
    transition ticks (guardCfg env) {side}AuthFunctions
      ⟨{{function := {entry}}},[],authorizationFrame env journal⟩ =
    .ok (.next 0 ({side}Opened env journal)) := by
  simp only [transition,{side}Opened,{side}AuthLookup,{side}AuthGuard,
    required,Bind.bind,Except.bind,Pure.pure,Except.pure]
  rfl

theorem {side}Sample (env : World.Environment) (journal : Journal) (ticks : Nat) :
    transition ticks (guardCfg env) {side}AuthFunctions ({side}Opened env journal) =
    .ok (.next 0 ({side}Sampled env journal)) := by
  simp only [transition,{side}Opened,{side}Sampled,{side}AuthLookup,{side}AuthGuard,
    required,Bind.bind,Except.bind,Pure.pure,Except.pure]
  rfl

theorem {side}Point (env : World.Environment) (journal : Journal) (ticks : Nat) :
    transition ticks (guardCfg env) {side}AuthFunctions ({side}Sampled env journal) =
    .ok (.next 0 ({side}Pointer env journal)) := by
  simp only [transition,{side}Sampled,{side}Pointer,{side}AuthLookup,{side}AuthGuard,
    required,Bind.bind,Except.bind,Pure.pure,Except.pure]
  rfl

theorem {side}Load (env : World.Environment) (journal : Journal) (ticks : Nat) :
    transition ticks (guardCfg env) {side}AuthFunctions ({side}Pointer env journal) =
    .ok (.next 0 ({side}AfterRead env journal)) := by
  have enough : 8+1 ≤ (authWords env).length := by simp [authWords]
  have ready : expand (authArena env) 256 32 = .ok (authArena env) :=
    expandArenaRange (authWords env) 8 1 enough (by simp [authWords])
  have loaded : readWord (authArena env) 256 = deadline env :=
    readArenaWord (authWords env) 8 (deadline env) (by rfl)
  have merged (f : Frame) (mem : f.memory = authArena env) :
      operation (guardCfg env) f (.memory .load) [256#256] = value f (deadline env) := by
    simp only [operation,memoryOperation,mem,show (256#256).toNat = 256 from rfl,ready,loaded,
      Bind.bind,Except.bind,Pure.pure,Except.pure]
    cases f
    simp_all
  simp only [guardCfg,mirAvailable] at merged
  simp only [transition,{side}Pointer,{side}AuthLookup,{side}AuthGuard,
    required,Bind.bind,Except.bind,Pure.pure,Except.pure]
  simp [instruction,merged,guardCfg,mirAvailable,authorizationFrame,
    signatureArgs,{side}AfterRead,assign,assignValue,MIR.State.read,readValue,readBits,
    Value.fits,ValueType.same,ValueType.width,Value.word,scalar,Scalar.ofWord,put,effect,value,
    Bind.bind,Except.bind,Pure.pure,Except.pure,required,memoryOperation,ready,loaded,next,advance,
    List.filter_cons] <;> rfl

theorem {side}DeadlineRead (env : World.Environment) (journal : Journal) (surplus : Nat) :
    mirStepsFrom (guardCfg env) {side}AuthFunctions
      ⟨{{function := {entry}}},[],authorizationFrame env journal⟩ 4 (surplus+7) =
    .ok (.inr ({side}AfterRead env journal)) := by
  rw [mirStepsContinue 1 3 (surplus+7) (one _ _ (surplus+6) _ _ ({side}Open env journal _))]
  have t : surplus+7-1 = surplus+6 := by omega
  rw [t,mirStepsContinue 1 2 (surplus+6) (one _ _ (surplus+5) _ _ ({side}Sample env journal _))]
  have t : surplus+6-1 = surplus+5 := by omega
  rw [t,mirStepsContinue 1 1 (surplus+5) (one _ _ (surplus+4) _ _ ({side}Point env journal _))]
  have t : surplus+5-1 = surplus+4 := by omega
  rw [t]
  exact one _ _ (surplus+3) _ _ ({side}Load env journal _)

theorem {side}MIRDeadline (env : World.Environment) (journal : Journal) (surplus : Nat) :
    mirStepsFrom (guardCfg env) {side}AuthFunctions ({side}AfterRead env journal) 3 (surplus+3) =
    .ok (.inr ({side}AfterGuard env journal)) := by
  have wordLT : Evm.lt (deadline env) timestamp =
      if (deadline env).ult timestamp then 1#256 else 0#256 := by
    cases h : (deadline env).ult timestamp <;> simp [Evm.lt,h]
  cases cmp : (deadline env).ult timestamp <;>
    have flag : expiry env = if (deadline env).ult timestamp then 1#256 else 0#256 := wordLT
  all_goals
    simp [cmp] at flag
    simp (config := {{maxSteps := 50000}}) [mirStepsFrom,Execution.Trace.segment,transition,
      {side}AuthLookup,{side}AuthGuard,required,next,advance,enter,branch,instruction,operation,
      guardCfg,mirAvailable,{side}AfterRead,{side}AfterGuard,authorizationFrame,signatureArgs,
      assign,assignValue,MIR.State.read,readValue,readBits,Value.fits,ValueType.same,ValueType.width,
      Value.word,scalar,Scalar.ofWord,put,effect,value,Bind.bind,Except.bind,Pure.pure,Except.pure,
      List.filter_cons,MIR.compare,cmp,flag,wordBits] <;> first | rfl | decide

theorem {side}_deadline_guard (env : World.Environment) (journal : Journal) (surplus : Nat) :
    mirStepsFrom (guardCfg env) {side}AuthFunctions
      ⟨{{function := {entry}}},[],authorizationFrame env journal⟩ 7 (surplus+7) =
    .ok (.inr ({side}AfterGuard env journal)) ∧
    (afterDeadline env surplus).pc =
      (if ({side}AfterGuard env journal).cursor.block = {z["then"]} then {success} else {failure}) := by
  constructor
  · change mirStepsFrom _ _ _ (4+3) _ = _
    rw [mirStepsContinue 4 3 (surplus+7) ({side}DeadlineRead env journal surplus)]
    have remaining : surplus+7-4 = surplus+3 := by omega
    rw [remaining]
    exact {side}MIRDeadline env journal surplus
  · by_cases expired : expiry env = 0#256 <;>
      simp [afterDeadline,decoderFrame,{side}AfterGuard,expired]

#print axioms {side}_deadline_guard
"""
