"""Check the real typed zero-callback path to the priced collateral helper."""

from string import Template

from . import collateral_mir, decoder_proof, mir_state
from .callers import require
from .collateral_proof import PRECONDITIONS


def plan(tables, catalog):
    definitions = {row["name"]: row for row in catalog.values()}
    sides = []
    for table in tables:
        body = table["supplyCollateral"]
        callee = table["safeTransferFrom"]["id"]
        writes = [
            index
            for index, block in enumerate(body["blocks"])
            if len(block["instructions"]) == 20
            and block["instructions"][13]["operation"].get("name") == "sstore"
        ]
        require(len(writes) == 1, "expected one collateral update block")
        update = writes[0]
        block = body["blocks"][update]
        term = block["terminator"]
        require(term["kind"] == "branch", "expected a callback branch")
        exit_block = term["else"]
        exit_ = body["blocks"][exit_block]
        ops = exit_["instructions"]
        require(
            len(ops) == 5
            and ops[4]["operation"]["kind"] == "call"
            and ops[4]["operation"]["function"] == callee,
            "expected transferFrom after the empty callback branch",
        )
        values = dict(
            zip(
                (
                    "caller_id",
                    "widened_id",
                    "length_id",
                    "nonempty_id",
                    "pointer_id",
                    "token_id",
                    "clean_id",
                    "address_id",
                ),
                [block["instructions"][i]["result"] for i in (14, 16, 18, 19)]
                + [op["result"] for op in ops[:4]],
                strict=True,
            )
        )
        sides.append(
            {
                "update": update,
                "exit_block": exit_block,
                "callee": callee,
                "block": mir_state.block(exit_, definitions),
                **values,
            }
        )
    return sides


def fragments(decoder, tail, sides):
    yield SHARED
    for side, plan_ in zip(("before", "after"), sides, strict=True):
        yield f"""
theorem {side}CollateralExitBlock :
    {side}MarketBody.blocks[{plan_["exit_block"]}]? = some {plan_["block"]} := rfl
"""
        for part in PARTS:
            yield Template(part).substitute(side=side, **plan_)
        yield composed(side, plan_["callee"], decoder, tail)


def composed(side, callee, decoder, tail):
    binders = decoder_proof.input_binders(decoder)
    return f"""
def {side}EmptyCollateralMIR (env : World.Environment) (journal : Execution.Journal) :
    Except String (MirCut ⊕ MirCursor) := do
  let frame ← decode selector caller {{mirCfg with environment := env}} journal
  let .inr machine ← mirStepsFrom {{mirCfg with environment := env}} {side}Functions
      ⟨{{function := {side}Entry}},[],frame⟩ 52 | .error "stopped before transferFrom"
  mirStep {callee} [] 972 {{mirCfg with environment := env}} {side}Functions
    ⟨machine,[],false,none⟩ Keccak.hash

theorem {side}_typed_collateral_helper {binders} {collateral_mir.MIR_ASSUMPTIONS}
    (selected : MIR.TransferFromDispatch.selector env.input = selector)
    (empty : Execution.readWord env.input 260 = 0) :
    {side}EmptyCollateralMIR env journal = .ok (.inl ({side}TypedCut env journal)) := by
  have tokenFits : (tokenWord env).toNat < 2^160 := addresses 36 (by decide)
  unfold {side}EmptyCollateralMIR
  rw [collateral_decoded env journal surplus room small offsetWord slice addresses selected]
  change (do
    let .inr machine ← mirStepsFrom {{mirCfg with environment := env}} {side}Functions
        (MIR.TransferFromMarket.mirStart {side}Entry journal (argumentWords env) (collateralArguments env))
        (40+12) 1024 | .error "stopped before transferFrom"
    mirStep {callee} [] 972 {{mirCfg with environment := env}} {side}Functions
      ⟨machine,[],false,none⟩ Keccak.hash) = _
  have stored := {side}_mir_collateral {collateral_mir.MIR_ARGS}
  change mirStepsFrom _ _ _ 40 1024 = _ at stored
  rw [mirStepsContinue 40 12 1024 stored,
    mirStepsContinue 4 8 984 ({side}LogPath env journal writable),
    mirStepsContinue 4 4 980 ({side}EmptyPath env journal empty),
    {side}HelperPath env journal tokenFits]
  exact {side}AtTransferCall env journal tokenFits

theorem {side}_abi_collateral_helper {binders}
    (empty : Execution.readWord env.input 260 = 0)
    {PRECONDITIONS.replace("surplus + (collateralWritePrice", f"surplus + {tail.gas} + (collateralWritePrice")}
    (created : ¬ marketMissing (marketValue env journal))
    (noValue : env.value = 0)
    (selected : MIR.TransferFromDispatch.selector env.input = selector) :
    ∃ (physical : ByteCut) (typed : MirCut),
      runBytes {tail.helper} 2048 tvWorldConfig
        ⟨dispatchFrame env (collateralEntryGas env journal surplus) 0 [],journal,[],[],[]⟩ = .ok physical ∧
      {side}EmptyCollateralMIR env journal = .ok (.inl typed) ∧
      physical.frame.gas = surplus ∧ typed.args = (physical.frame.stack.drop 1).take 4 ∧
      typed.frame.memory = physical.frame.memory ∧
      typed.frame.world = {{physical.world with refund := journal.refund}} := by
  have ownerFits : (collateralOwner env).toNat < 2^160 := addresses 196 (by decide)
  refine ⟨collateralHelperCut env journal surplus,{side}TypedCut env journal,?_,?_,rfl,?_⟩
  · exact abi_to_collateral_helper env journal surplus room small offsetWord slice addresses empty
      assetsNonzero ownerNonzero assetsFits sumFits writable sentry created noValue selected
  · exact {side}_typed_collateral_helper env journal surplus room small offsetWord slice addresses
      created assetsNonzero ownerNonzero ownerFits assetsFits sumFits writable selected empty
  · exact {side}TypedCut_frame env journal surplus

#print axioms {side}_abi_collateral_helper
"""


SHARED = """
def tokenWord (env : World.Environment) : Word := Execution.readWord env.input 36

def tailWords (env : World.Environment) : List Word :=
  [collateralAssets env,positionMapping env,288,0] ++ argumentWords env

def loggedFrame (env : World.Environment) (journal : Execution.Journal) (frame : Frame) : Frame :=
  {frame with memory := wordArena (tailWords env), «world» :=
    {(collateralLogged env journal) with refund := journal.refund}}

theorem callerFits (env : World.Environment) : (env.caller.setWidth 256).toNat < 2^160 := by
  rw [BitVec.toNat_setWidth,Nat.mod_eq_of_lt (by have := env.caller.isLt; omega)]
  exact env.caller.isLt

theorem addressFits (env : World.Environment) : (env.address.setWidth 256).toNat < 2^160 := by
  rw [BitVec.toNat_setWidth,Nat.mod_eq_of_lt (by have := env.address.isLt; omega)]
  exact env.address.isLt

theorem segment1 (cfg : Config) (functions : Functions) (ticks : Nat)
    (machine following : Machine)
    (checked : transition ticks cfg functions machine = .ok (.next 0 following)) :
    mirStepsFrom cfg functions machine 1 (ticks+1) = .ok (.inr following) := by
  simp only [mirStepsFrom,Execution.Trace.segment,Nat.add_sub_cancel,checked,
    Bind.bind,Except.bind,Pure.pure,Except.pure,Nat.succ_ne_zero,ite_false]

def transferArgs (env : World.Environment) : List Word :=
  [tokenWord env,env.caller.setWidth 256,env.address.setWidth 256,collateralAssets env]

theorem foundBody {α : Type} (candidate : Option α) (body : α) (reason : String)
    (eq : required candidate reason = .ok body) : candidate = some body := by
  cases candidate <;> simp_all [required]
"""

PARTS = (
    """
def ${side}TailLocals (env : World.Environment) (frame : Frame) : List (Nat × Value) :=
  [(${widened_id},scalar 256 (env.caller.setWidth 256)),(${caller_id},scalar 160 (env.caller.setWidth 256))] ++ frame.locals

def ${side}EmptyLocals (locals : List (Nat × Value)) : List (Nat × Value) :=
  (${nonempty_id},scalar 1 0) :: (${length_id},scalar 256 0) :: locals

def ${side}HelperLocals (env : World.Environment) (locals : List (Nat × Value)) : List (Nat × Value) :=
  [(${address_id},scalar 160 (env.address.setWidth 256)),(${clean_id},scalar 160 (tokenWord env)),
    (${token_id},scalar 256 (tokenWord env)),(${pointer_id},scalar 256 160)] ++ locals

def ${side}Logged (env : World.Environment) (journal : Execution.Journal) : Machine :=
  let start := ${side}CollateralWritten env journal
  ⟨{start.cursor with next := 18},[],{loggedFrame env journal start.frame with
    locals := ${side}TailLocals env start.frame}⟩

def ${side}Branched (env : World.Environment) (journal : Execution.Journal) : Machine :=
  let start := ${side}Logged env journal
  ⟨{function := ${side}Entry, block := $exit_block, predecessor := some $update, entered := true},[],
    {start.frame with locals := ${side}EmptyLocals start.frame.locals}⟩

def ${side}HelperState (env : World.Environment) (journal : Execution.Journal) : Machine :=
  let start := ${side}Branched env journal
  ⟨{start.cursor with next := 4},[],{start.frame with locals := ${side}HelperLocals env start.frame.locals}⟩



def ${side}AfterCaller (env : World.Environment) (journal : Execution.Journal) : Machine :=
  let start := ${side}CollateralWritten env journal
  ⟨{start.cursor with next := 15},[],{start.frame with locals :=
    (${caller_id},scalar 160 (env.caller.setWidth 256)) :: start.frame.locals}⟩

def ${side}AfterMstore (env : World.Environment) (journal : Execution.Journal) : Machine :=
  let start := ${side}AfterCaller env journal
  ⟨{start.cursor with next := 16},[],{start.frame with memory := wordArena (tailWords env)}⟩

def ${side}AfterWiden (env : World.Environment) (journal : Execution.Journal) : Machine :=
  let start := ${side}AfterMstore env journal
  ⟨{start.cursor with next := 17},[],{start.frame with locals :=
    (${widened_id},scalar 256 (env.caller.setWidth 256)) :: start.frame.locals}⟩


theorem ${side}CallerStep (env : World.Environment) (journal : Execution.Journal) (ticks : Nat) :
    transition ticks {mirCfg with environment := env} ${side}Functions (${side}CollateralWritten env journal) =
    .ok (.next 0 (${side}AfterCaller env journal)) := by
  simp only [${side}CollateralWritten, ${side}AfterCaller, transition, ${side}MarketLookup, ${side}MarketBlock${update},
    required, Bind.bind, Except.bind, Pure.pure, Except.pure]
  rfl

theorem ${side}StoreStep (env : World.Environment) (journal : Execution.Journal) (ticks : Nat) :
    transition ticks {mirCfg with environment := env} ${side}Functions (${side}AfterCaller env journal) =
    .ok (.next 0 (${side}AfterMstore env journal)) := by
  let before := [collateralOwner env,positionMapping env,288,0] ++ argumentWords env
  have within : 0+1 ≤ before.length := by simp [before]
  have memory : Execution.expand (collateralArena env) 0 32 =
      .ok (collateralArena env) :=
    expandArenaRange before 0 1 within (by simp [before, argumentWords])
  have write : Execution.writeBytes (collateralArena env) 0 (Execution.wordBytes (collateralAssets env)) =
      wordArena (tailWords env) :=
    writeActiveArena before 0 (collateralAssets env) (by omega)
  simp only [${side}AfterMstore, ${side}AfterCaller, ${side}CollateralWritten, transition, ${side}MarketLookup,
    ${side}MarketBlock${update}, required, Bind.bind, Except.bind, Pure.pure, Except.pure]
  -- Simplify frame updates before exposing the interpreter operation.
  simp [instruction, assign, operation, memoryOperation, next, advance, MIR.State.read, readValue,
    Value.fits, Value.word, ValueType.same, ValueType.width, scalar, ${side}MarketMachine,
    collateralArguments, memory, write, Execution.wordBytes_length, effect, mirCfg,
    required, Bind.bind, Except.bind, Pure.pure, Except.pure, Scalar.ofWord] <;> rfl


theorem ${side}WidenStep (env : World.Environment) (journal : Execution.Journal) (ticks : Nat) :
    transition ticks {mirCfg with environment := env} ${side}Functions (${side}AfterMstore env journal) =
    .ok (.next 0 (${side}AfterWiden env journal)) := by
  have callerWide : (Scalar.ofWord 160 (env.caller.setWidth 256)).word = env.caller.setWidth 256 :=
    scalarFits 160 _ (callerFits env)
  unfold Scalar.ofWord at callerWide
  simp only [${side}AfterWiden, ${side}AfterMstore, ${side}AfterCaller, ${side}CollateralWritten, transition,
    ${side}MarketLookup, ${side}MarketBlock${update}, required, Bind.bind, Except.bind, Pure.pure, Except.pure]
  simp [instruction, assign, assignValue, operation, next, advance, MIR.State.read, readValue,
    Value.fits, Value.word, ValueType.same, ValueType.width, scalar, ${side}MarketMachine,
    required, Bind.bind, Except.bind, Pure.pure, Except.pure, Scalar.ofWord, put, effect,
    List.filter_cons, callerWide, mirCfg] <;> rfl
""",
    """
theorem ${side}LogPaid (env : World.Environment) (journal : Execution.Journal) (ticks : Nat)
    (writable : env.isStatic = false) :
    transition ticks {mirCfg with environment := env} ${side}Functions (${side}AfterWiden env journal) =
    .ok (.next 0 (${side}Logged env journal)) := by
  have within : 0+1 ≤ (tailWords env).length := by simp [tailWords]
  have active : Execution.expand (wordArena (tailWords env)) 0 32 =
      .ok (wordArena (tailWords env)) :=
    expandArenaRange (tailWords env) 0 1 within (by simp [tailWords, argumentWords])
  have payload : Execution.readBytes (wordArena (tailWords env)) 0 32 =
      Execution.wordBytes (collateralAssets env) := by
    rw [show 0 = 32*0 by rfl, show 32=32*1 by rfl, readArena _ _ _ within]
    simp [wordArena,tailWords]
  simp only [${side}AfterWiden, ${side}AfterMstore, ${side}AfterCaller, ${side}Logged, ${side}CollateralWritten, transition,
    ${side}MarketLookup, ${side}MarketBlock${update}, required, Bind.bind, Except.bind, Pure.pure, Except.pure]
  simp [instruction, assign, operation, next, advance, MIR.State.read, readValue,
    Value.fits, Value.word, ValueType.same, ValueType.width, scalar, ${side}MarketMachine,
    required, Bind.bind, Except.bind, Pure.pure, Except.pure, Scalar.ofWord, effect, mirCfg,
    active, payload, writable, loggedFrame, ${side}TailLocals, collateralLogged, mirCollateralWritten] <;> rfl

theorem ${side}LogPath (env : World.Environment) (journal : Execution.Journal)
    (writable : env.isStatic = false) :
    mirStepsFrom {mirCfg with environment := env} ${side}Functions
      (${side}CollateralWritten env journal) 4 984 = .ok (.inr (${side}Logged env journal)) := by
  rw [mirStepsContinue 1 3 984 (segment1 _ _ 983 _ _ (${side}CallerStep env journal 983)),
    mirStepsContinue 1 2 983 (segment1 _ _ 982 _ _ (${side}StoreStep env journal 982)),
    mirStepsContinue 1 1 982 (segment1 _ _ 981 _ _ (${side}WidenStep env journal 981))]
  exact segment1 _ _ 980 _ _ (${side}LogPaid env journal 980 writable)
""",
    """
def ${side}AfterLength (env : World.Environment) (journal : Execution.Journal) : Machine :=
  let start := ${side}Logged env journal
  ⟨{start.cursor with next := 19},[],{start.frame with locals :=
    (${length_id},scalar 256 0) :: start.frame.locals}⟩

def ${side}AfterEmpty (env : World.Environment) (journal : Execution.Journal) : Machine :=
  let start := ${side}AfterLength env journal
  ⟨{start.cursor with next := 20},[],{start.frame with locals :=
    (${nonempty_id},scalar 1 0) :: start.frame.locals}⟩

def ${side}AtHelperBlock (env : World.Environment) (journal : Execution.Journal) : Machine :=
  let start := ${side}AfterEmpty env journal
  ⟨{function := ${side}Entry, block := $exit_block, predecessor := some $update},[],start.frame⟩

theorem ${side}LengthStep (env : World.Environment) (journal : Execution.Journal) (ticks : Nat)
    (empty : Execution.readWord env.input 260 = 0) :
    transition ticks {mirCfg with environment := env} ${side}Functions (${side}Logged env journal) =
    .ok (.next 0 (${side}AfterLength env journal)) := by
  simp only [${side}AfterLength, ${side}Logged, ${side}CollateralWritten, transition,
    ${side}MarketLookup, ${side}MarketBlock${update}, required, Bind.bind, Except.bind, Pure.pure, Except.pure]
  simp [instruction, extractField, assignValue, next, advance, readValue, Value.fits, ValueType.fields,
    ValueType.same, ValueType.width, scalar, ${side}MarketMachine, collateralArguments, loggedFrame,
    required, Bind.bind, Except.bind, Pure.pure, Except.pure, put, effect, empty,
    List.filter_cons] <;> rfl

theorem ${side}EmptyStep (env : World.Environment) (journal : Execution.Journal) (ticks : Nat) :
    transition ticks {mirCfg with environment := env} ${side}Functions (${side}AfterLength env journal) =
    .ok (.next 0 (${side}AfterEmpty env journal)) := by
  simp only [${side}AfterEmpty, ${side}AfterLength, ${side}Logged, ${side}CollateralWritten, transition,
    ${side}MarketLookup, ${side}MarketBlock${update}, required, Bind.bind, Except.bind, Pure.pure, Except.pure]
  rfl

theorem ${side}BranchStep (env : World.Environment) (journal : Execution.Journal) (ticks : Nat) :
    transition ticks {mirCfg with environment := env} ${side}Functions (${side}AfterEmpty env journal) =
    .ok (.next 0 (${side}AtHelperBlock env journal)) := by
  simp only [${side}AtHelperBlock, ${side}AfterEmpty, ${side}AfterLength, ${side}Logged, ${side}CollateralWritten,
    transition, ${side}MarketLookup, ${side}MarketBlock${update}, required, Bind.bind, Except.bind, Pure.pure, Except.pure]
  rfl

theorem ${side}EnterHelper (env : World.Environment) (journal : Execution.Journal) (ticks : Nat) :
    transition ticks {mirCfg with environment := env} ${side}Functions (${side}AtHelperBlock env journal) =
    .ok (.next 0 (${side}Branched env journal)) := by
  simp only [${side}AtHelperBlock, ${side}AfterEmpty, ${side}AfterLength, ${side}Logged, ${side}Branched, ${side}CollateralWritten,
    transition, ${side}MarketLookup, ${side}CollateralExitBlock, required, Bind.bind, Except.bind, Pure.pure, Except.pure]
  rfl

theorem ${side}EmptyPath (env : World.Environment) (journal : Execution.Journal)
    (empty : Execution.readWord env.input 260 = 0) :
    mirStepsFrom {mirCfg with environment := env} ${side}Functions
      (${side}Logged env journal) 4 980 = .ok (.inr (${side}Branched env journal)) := by
  rw [mirStepsContinue 1 3 980 (segment1 _ _ 979 _ _ (${side}LengthStep env journal 979 empty)),
    mirStepsContinue 1 2 979 (segment1 _ _ 978 _ _ (${side}EmptyStep env journal 978)),
    mirStepsContinue 1 1 978 (segment1 _ _ 977 _ _ (${side}BranchStep env journal 977))]
  exact segment1 _ _ 976 _ _ (${side}EnterHelper env journal 976)
""",
    """
def ${side}AfterPointer (env : World.Environment) (journal : Execution.Journal) : Machine :=
  let start := ${side}Branched env journal
  ⟨{start.cursor with next := 1},[],{start.frame with locals :=
    (${pointer_id},scalar 256 160) :: start.frame.locals}⟩

def ${side}AfterToken (env : World.Environment) (journal : Execution.Journal) : Machine :=
  let start := ${side}AfterPointer env journal
  ⟨{start.cursor with next := 2},[],{start.frame with locals :=
    (${token_id},scalar 256 (tokenWord env)) :: start.frame.locals}⟩

def ${side}AfterClean (env : World.Environment) (journal : Execution.Journal) : Machine :=
  let start := ${side}AfterToken env journal
  ⟨{start.cursor with next := 3},[],{start.frame with locals :=
    (${clean_id},scalar 160 (tokenWord env)) :: start.frame.locals}⟩

theorem ${side}PointerStep (env : World.Environment) (journal : Execution.Journal) (ticks : Nat) :
    transition ticks {mirCfg with environment := env} ${side}Functions (${side}Branched env journal) =
    .ok (.next 0 (${side}AfterPointer env journal)) := by
  simp only [${side}AfterPointer, ${side}Branched, ${side}Logged, ${side}CollateralWritten, transition,
    ${side}MarketLookup, ${side}CollateralExitBlock, required, Bind.bind, Except.bind, Pure.pure, Except.pure]
  rfl
""",
    """
theorem ${side}TokenStep (env : World.Environment) (journal : Execution.Journal) (ticks : Nat) :
    transition ticks {mirCfg with environment := env} ${side}Functions (${side}AfterPointer env journal) =
    .ok (.next 0 (${side}AfterToken env journal)) := by
  have enough : 5+1 ≤ (tailWords env).length := by simp [tailWords,argumentWords]
  have active : Execution.expand (wordArena (tailWords env)) 160 32 =
      .ok (wordArena (tailWords env)) :=
    expandArenaRange (tailWords env) 5 1 enough (by simp [tailWords,argumentWords])
  have token : Execution.readWord (wordArena (tailWords env)) 160 = tokenWord env :=
    readArenaWord (tailWords env) 5 (tokenWord env) (by rfl)
  simp only [${side}AfterToken, ${side}AfterPointer, ${side}Branched, ${side}Logged, ${side}CollateralWritten, transition,
    ${side}MarketLookup, ${side}CollateralExitBlock, required, Bind.bind, Except.bind, Pure.pure, Except.pure]
  simp [instruction, assign, assignValue, operation, memoryOperation, next, advance, MIR.State.read, readValue,
    Value.fits, Value.word, ValueType.same, ValueType.width, scalar, ${side}MarketMachine,
    required, Bind.bind, Except.bind, Pure.pure, Except.pure, Scalar.ofWord, effect, mirCfg,
    active, token, put, loggedFrame, List.filter_cons] <;> rfl

theorem ${side}CleanStep (env : World.Environment) (journal : Execution.Journal) (ticks : Nat)
    (tokenFits : (tokenWord env).toNat < 2^160) :
    transition ticks {mirCfg with environment := env} ${side}Functions (${side}AfterToken env journal) =
    .ok (.next 0 (${side}AfterClean env journal)) := by
  have cleaned : narrow 160 (tokenWord env) = tokenWord env :=
    scalarFits 160 _ tokenFits
  simp only [${side}AfterClean, ${side}AfterToken, ${side}AfterPointer, ${side}Branched, ${side}Logged, ${side}CollateralWritten,
    transition, ${side}MarketLookup, ${side}CollateralExitBlock, required, Bind.bind, Except.bind, Pure.pure, Except.pure]
  simp [instruction, assign, assignValue, operation, next, advance, MIR.State.read, readValue,
    Value.fits, Value.word, ValueType.same, ValueType.width, scalar, ${side}MarketMachine,
    required, Bind.bind, Except.bind, Pure.pure, Except.pure, Scalar.ofWord, effect, mirCfg,
    cleaned, put, List.filter_cons] <;> rfl
""",
    """
theorem ${side}AddressStep (env : World.Environment) (journal : Execution.Journal) (ticks : Nat) :
    transition ticks {mirCfg with environment := env} ${side}Functions (${side}AfterClean env journal) =
    .ok (.next 0 (${side}HelperState env journal)) := by
  simp only [${side}AfterClean, ${side}AfterToken, ${side}AfterPointer, ${side}Branched, ${side}Logged, ${side}CollateralWritten,
    ${side}HelperState, transition, ${side}MarketLookup, ${side}CollateralExitBlock, required, Bind.bind,
    Except.bind, Pure.pure, Except.pure]
  rfl

theorem ${side}HelperPath (env : World.Environment) (journal : Execution.Journal)
    (tokenFits : (tokenWord env).toNat < 2^160) :
    mirStepsFrom {mirCfg with environment := env} ${side}Functions
      (${side}Branched env journal) 4 976 = .ok (.inr (${side}HelperState env journal)) := by
  rw [mirStepsContinue 1 3 976 (segment1 _ _ 975 _ _ (${side}PointerStep env journal 975)),
    mirStepsContinue 1 2 975 (segment1 _ _ 974 _ _ (${side}TokenStep env journal 974)),
    mirStepsContinue 1 1 974 (segment1 _ _ 973 _ _ (${side}CleanStep env journal 973 tokenFits))]
  exact segment1 _ _ 972 _ _ (${side}AddressStep env journal 972)
""",
    """
def ${side}TypedCut (env : World.Environment) (journal : Execution.Journal) : MirCut :=
  let machine := ${side}HelperState env journal
  ⟨transferArgs env,[],machine.frame,machine.cursor,[]⟩


theorem ${side}AtTransferCall (env : World.Environment) (journal : Execution.Journal)
    (tokenFits : (tokenWord env).toNat < 2^160) :
    mirStep $callee [] 972 {mirCfg with environment := env} ${side}Functions
      ⟨${side}HelperState env journal,[],false,none⟩ Keccak.hash = .ok (.inl (${side}TypedCut env journal)) := by
  let m := ${side}HelperState env journal
  let token : Operand := ⟨.local ${clean_id},.word 160⟩
  let sender : Operand := ⟨.local ${caller_id},.word 160⟩
  let receiver : Operand := ⟨.local ${address_id},.word 160⟩
  let amount : Operand := ⟨.argument 1,.word 256⟩
  have call : instAt ${side}Functions m.cursor =
      some (⟨none,.word 0,none,.call $callee,[token,sender,receiver,amount]⟩ : Instruction) := by
    have lookup : (${side}Functions.find? (·.1 == ${side}Entry)).map Prod.snd = some ${side}MarketBody :=
      foundBody _ _ _ ${side}MarketLookup
    change (do
      let body ← (${side}Functions.find? (fun f : Nat × Function => f.1 == ${side}Entry)).map Prod.snd
      let block ← body.blocks[$exit_block]?
      block.instructions[4]? : Option Instruction) = _
    rw [lookup]
    change (do let block ← ${side}MarketBody.blocks[$exit_block]?; block.instructions[4]? : Option Instruction) = _
    rw [${side}CollateralExitBlock]
    rfl
  have readsToken : MIR.State.read m.frame token = .ok (tokenWord env) := by
    change (Except.ok ((Scalar.ofWord 160 (tokenWord env)).word) : Except String Word) = _
    rw [scalarFits 160 _ tokenFits]
  have readsSender : MIR.State.read m.frame sender = .ok (env.caller.setWidth 256) := by
    change (Except.ok ((Scalar.ofWord 160 (env.caller.setWidth 256)).word) : Except String Word) = _
    rw [scalarFits 160 _ (callerFits env)]
  have readsReceiver : MIR.State.read m.frame receiver = .ok (env.address.setWidth 256) := by
    change (Except.ok ((Scalar.ofWord 160 (env.address.setWidth 256)).word) : Except String Word) = _
    rw [scalarFits 160 _ (addressFits env)]
  have readsAmount : MIR.State.read m.frame amount = .ok (collateralAssets env) := rfl
  change mirStep $callee [] 972 {mirCfg with environment := env} ${side}Functions
    ⟨m,[],false,none⟩ Keccak.hash = _
  unfold mirStep
  simp only [call, ite_true]
  change (do
    let args ← [token,sender,receiver,amount].mapM (MIR.State.read m.frame)
    return Sum.inl (MirCut.mk args [] m.frame m.cursor []) : Except String (MirCut ⊕ MirCursor)) = _
  simp only [List.mapM_cons,List.mapM_nil, readsToken, readsSender, readsReceiver, readsAmount,
    Bind.bind,Except.bind,Pure.pure,Except.pure]
  rfl

theorem ${side}TypedCut_frame (env : World.Environment) (journal : Execution.Journal) (gas : Nat) :
    let typed := ${side}TypedCut env journal
    let physical := collateralHelperCut env journal gas
    typed.args = (physical.frame.stack.drop 1).take 4 ∧
    typed.frame.memory = physical.frame.memory ∧
    typed.frame.world = {physical.world with refund := journal.refund} := by
  exact ⟨rfl,rfl,rfl⟩
""",
)
