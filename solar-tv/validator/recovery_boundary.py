"""Bind the recovery gas and call proof to the final Morpho runtime."""

from dataclasses import dataclass

from . import authorization_lowering, entry_proof, native, packing_boundary, world
from .callers import require
from .check import build_model

MODULE = "SolarTV.MIR.Transforms.StaticCallCut"
STORAGE = "SolarTV.MIR.Transforms.StorageBoundary"
NAME = "MorphoRecoveryBoundary"


@dataclass
class Site:
    gas: int
    call: int
    nonce: int
    authorization: int


def find(decoded):
    points = list(decoded)
    sites = [
        (gas, call)
        for address, gas, call in zip(points, points[1:], points[2:], strict=False)
        if [decoded[pc] for pc in (address, gas, call)]
        == [("push1", 1), ("gas", None), ("staticcall", None)]
        and call == gas + 1
    ]
    require(len(sites) == 1, "expected one adjacent recovery GAS and STATICCALL")
    gas, call = sites[0]
    at = points.index(call)
    nearby = points[at - 140 : at + 90]
    stores = [pc for pc in nearby if decoded[pc] == ("sstore", None)]
    require(
        len(stores) == 2 and stores[0] < gas < stores[1], "unexpected signing stores"
    )
    nonce, authorization = stores
    nonce_at = points.index(nonce)
    authorization_at = points.index(authorization)
    require(
        [decoded[pc][0] for pc in points[nonce_at - 2 : nonce_at + 3]]
        == ["swap1", "swap2", "sstore", "dup4", "sub"]
        and [
            decoded[pc][0] for pc in points[authorization_at - 2 : authorization_at + 5]
        ]
        == ["push0", "keccak256", "sstore", "push0", "mstore", "swap1", "push2"],
        "unexpected nonce or authorization storage continuation",
    )
    return Site(gas, call, nonce, authorization)


def source(site, configuration, base, packing):
    return f"""import EntryImage
import {MODULE}
import {STORAGE}
import {packing}
{entry_proof.HEADER}
open SolarTV.EVM.MessageBoundary
open MIR.StaticCallCut (project priced)
{configuration}
def samplePC : Nat := {site.gas}
def recoveryPC : Nat := {site.call}

theorem executed (journal : Execution.Journal) (frame : EVM.State.Frame)
    (offset length : Word) (rest : List Word)
    (image : frame.code = code.bytes) (pc : frame.pc = samplePC)
    (stack : frame.stack = 1 :: offset :: length :: 0 :: 32 :: rest)
    (space : rest.length+6 ≤ 1024) :
    EVM.State.instruction tvWorldConfig journal frame =
      (if frame.gas < 2 then EVM.State.stepExhausted journal
        else .ok (.next journal (afterGas frame))) ∧
    EVM.State.instruction tvWorldConfig journal (afterGas frame) =
      staticAction tvWorldConfig journal (afterGas frame)
        ((BitVec.ofNat 256 (frame.gas-2)).toNat) 1 offset length 0 32 rest := by
  apply recovery tvWorldConfig journal frame {base} offset length rest stack
  · rw [image, pc, ← EVM.State.CodeChunks.get_eq]
    decide +kernel
  · rw [image, pc, ← EVM.State.CodeChunks.get_eq]
    decide +kernel
  · rfl
  · rfl
  · decide
  · exact space

theorem contextual (cfg : MIR.State.Config) (work : Nat)
    (mir : MIR.State.Frame) (frame : EVM.State.Frame)
    (rest : List Word) (address arg : MIR.State.Operand)
    (offset length : MIR.State.Scalar) (scratch input active : List Execution.Byte)
    (image : frame.code = code.bytes) (pc : frame.pc = samplePC)
    (stack : frame.stack = 1 :: offset.word :: length.word :: 0 :: 32 :: rest)
    (space : rest.length+6 ≤ 1024)
    (readAddress : MIR.State.read mir address = .ok 1)
    (readSlice : MIR.State.readValue mir arg = .ok (.aggregate [.scalar offset,.scalar length]))
    (available : cfg.available.contains 0xfa = true)
    (fork : cfg.fork = .cancun) (bound : cfg.externalConfig = some tvWorldConfig)
    (environment : frame.environment = cfg.environment)
    (paid : 2 ≤ frame.gas) (boundedGas : frame.gas-2 ≤ 30000000)
    (cleared : Execution.expand mir.memory 0 32 = .ok scratch)
    (memory : frame.memory = Execution.writeBytes scratch 0 (Execution.wordBytes 0))
    (inputOk : Execution.expand frame.memory offset.word.toNat length.word.toNat = .ok input)
    (outputOk : Execution.expand input 0 32 = .ok active) :
    let atCall := afterGas frame
    project {{mir with memory := frame.memory}} (do
      let action ← EVM.State.instruction tvWorldConfig mir.world atCall
      EVM.State.advance tvWorldConfig (EVM.State.run work tvWorldConfig) action) =
    MIR.LowerReturndata.invoke (priced cfg mir frame) work
      (MIR.State.messageArguments (priced cfg mir frame) mir
        (.address 0xfa false false true true true) [address,arg]) := by
  have action := (executed mir.world frame offset.word length.word rest image pc stack space).2
  have bounded : (BitVec.ofNat 256 (frame.gas-2)).toNat = (afterGas frame).gas := by
    simp [afterGas, Nat.mod_eq_of_lt (by omega : frame.gas-2 < 2^256)]
  dsimp only
  rw [action, bounded]
  exact MIR.StaticCallCut.recovery cfg tvWorldConfig work mir frame rest address arg offset length
    scratch input active readAddress readSlice available fork rfl bound environment (by decide)
    paid boundedGas cleared memory inputOk outputOk

def callConfig (frame : EVM.State.Frame) : MIR.State.Config :=
  {{fork := .cancun, environment := frame.environment, available := [0xfa],
    externalConfig := some tvWorldConfig, context := tvWorldConfig.block, entry := false}}

def typedFrame (journal : Execution.Journal) (frame : EVM.State.Frame)
    (offset length : Word) : MIR.State.Frame :=
  ⟨journal,frame.memory,[.aggregate [.scalar ⟨256,offset⟩,.scalar ⟨256,length⟩]],
    [],frame.returnData,0,0,[]⟩

def addressOperand : MIR.State.Operand := ⟨.constant ⟨160,1⟩,.word 160⟩
def sliceOperand : MIR.State.Operand := ⟨.argument 0,.slice 0⟩

def recoveryMessage (lowered : Bool) (work : Nat) (frame : EVM.State.Frame)
    (mir : MIR.State.Frame) (offset length : Word) : MIR.State.MessageStep :=
  let cfg := priced (callConfig frame) mir frame
  if lowered then MIR.LowerStaticWord.lowered cfg work mir 1 offset length
  else MIR.LowerReturndata.invoke cfg work
    (MIR.State.messageArguments cfg mir (.address 0xfa false false true true true)
      [addressOperand,sliceOperand])

theorem lowered_eq (work : Nat) (frame : EVM.State.Frame) (journal : Execution.Journal)
    (offset length : Word) (paid : 2 ≤ frame.gas) (bound : frame.gas-2 ≤ 30000000) :
    recoveryMessage true work frame (typedFrame journal frame offset length) offset length =
    recoveryMessage false work frame (typedFrame journal frame offset length) offset length := by
  symm
  unfold recoveryMessage
  apply MIR.LowerStaticWord.staticWord (pointer := ⟨256,offset⟩) (length := ⟨256,length⟩)
    (gasBefore := frame.gas) (budget := frame.gas-2) (budgets := [frame.gas-2])
  all_goals first | rfl | exact paid | exact Nat.le_refl _ | exact bound

def routeRecovery (lowered : Bool) : EVM.State.Reentry.Route
  | remainingWork+2, proceed, journal, frame => do
    if frame.code ≠ code.bytes ∨ frame.pc ≠ samplePC then none else
    let 1 :: offset :: length :: 0 :: 32 :: rest := frame.stack | none
    if rest.length+6 > 1024 then none
    else if frame.gas < 2 then some (.ok ⟨.outOfGas,[],journal,0,1⟩)
    else if frame.gas-2 > 30000000 then none
    else
      let .ok scratch := Execution.expand frame.memory 0 32 | none
      if scratch ≠ frame.memory ∨
          Execution.writeBytes scratch 0 (Execution.wordBytes 0) ≠ frame.memory then none else
      let .ok input := Execution.expand frame.memory offset.toNat length.toNat | none
      let .ok _ := Execution.expand input 0 32 | none
      let mir := typedFrame journal frame offset length
      some (MIR.StaticCallCut.resumeWith remainingWork
        (fun steps bounded => proceed steps (by omega)) frame journal rest
        (recoveryMessage lowered remainingWork frame mir offset length))
  | _, _, _, _ => none

theorem typedContinuation (remainingWork : Nat) (journal : Execution.Journal) (frame : EVM.State.Frame)
    (offset length : Word) (rest : List Word) (input active : List Execution.Byte)
    (image : frame.code = code.bytes) (pc : frame.pc = samplePC)
    (stack : frame.stack = 1 :: offset :: length :: 0 :: 32 :: rest)
    (space : rest.length+6 ≤ 1024)
    (paid : 2 ≤ frame.gas) (gasBound : frame.gas-2 ≤ 30000000)
    (expanded : Execution.expand frame.memory 0 32 = .ok frame.memory)
    (clear : Execution.writeBytes frame.memory 0 (Execution.wordBytes 0) = frame.memory)
    (inputOk : Execution.expand frame.memory offset.toNat length.toNat = .ok input)
    (outputOk : Execution.expand input 0 32 = .ok active) :
    MIR.StaticCallCut.resume tvWorldConfig remainingWork frame journal rest
      (recoveryMessage false remainingWork frame (typedFrame journal frame offset length) offset length) =
      EVM.State.run (remainingWork+2) tvWorldConfig journal frame := by
  let mir := typedFrame journal frame offset length
  have linked := contextual (callConfig frame) remainingWork mir frame rest addressOperand sliceOperand
    ⟨256,offset⟩ ⟨256,length⟩ frame.memory input active image pc stack space rfl rfl rfl rfl rfl rfl
    paid gasBound expanded clear.symm inputOk outputOk
  change project mir _ = recoveryMessage false remainingWork frame mir offset length at linked
  rw [← linked]
  obtain ⟨sample,call⟩ := executed journal frame offset length rest image pc stack space
  simp only [Nat.not_lt.mpr paid, ite_false] at sample
  have gas : (BitVec.ofNat 256 (frame.gas-2)).toNat = frame.gas-2 := by
    simp [Nat.mod_eq_of_lt (by omega : frame.gas-2 < 2^256)]
  rw [gas] at call
  exact MIR.StaticCallCut.resume_eq tvWorldConfig remainingWork frame mir rest 1 offset length rfl sample call

theorem recoverySound (lowered : Bool) : EVM.State.Reentry.Sound tvWorldConfig (routeRecovery lowered) := by
  intro work proceed agrees journal frame result chosen
  cases work with
  | zero => simp [routeRecovery] at chosen
  | succ remainingWork =>
    simp only [routeRecovery] at chosen
    split at chosen
    · contradiction
    rename_i located
    have location : frame.code = code.bytes ∧ frame.pc = samplePC := by simpa using located
    split at chosen
    · rename_i offset length rest stack
      split at chosen
      · contradiction
      rename_i fits
      have space : rest.length+6 ≤ 1024 := by omega
      split at chosen
      · rename_i low
        have gas := (executed journal frame offset length rest location.1 location.2 stack space).1
        simp only [low, ite_true] at gas
        cases chosen
        exact (EVM.State.run_done (remainingWork+1) gas).symm
      rename_i enough
      have paid : 2 ≤ frame.gas := by omega
      split at chosen
      · contradiction
      rename_i bound
      have gasBound : frame.gas-2 ≤ 30000000 := by omega
      split at chosen
      · rename_i scratch expanded
        split at chosen
        · contradiction
        rename_i memory
        have ready : scratch = frame.memory ∧
            Execution.writeBytes scratch 0 (Execution.wordBytes 0) = frame.memory :=
          by simpa using memory
        rw [ready.1] at expanded
        have clear := ready.2
        rw [ready.1] at clear
        split at chosen
        · rename_i input inputOk
          split at chosen
          · rename_i active outputOk
            cases chosen
            simp only [agrees]
            change MIR.StaticCallCut.resume tvWorldConfig remainingWork frame journal rest _ = _
            cases lowered
            · exact typedContinuation remainingWork journal frame offset length rest input active
                location.1 location.2 stack space paid gasBound expanded clear inputOk outputOk
            · rw [lowered_eq remainingWork frame journal offset length paid gasBound]
              exact typedContinuation remainingWork journal frame offset length rest input active
                location.1 location.2 stack space paid gasBound expanded clear inputOk outputOk
          · contradiction
        · contradiction
      · contradiction
    · contradiction

def noncePC : Nat := {site.nonce}
def authorizationPC : Nat := {site.authorization}

def routeStore (first second : Bool) : EVM.State.Reentry.Route
  | remainingWork+1, proceed, journal, frame => do
    if frame.code = code.bytes ∧
        ((first = true ∧ frame.pc = noncePC) ∨ (second = true ∧ frame.pc = authorizationPC)) then
      let slot :: word :: rest := frame.stack | none
      if rest.length > 1024 then none else
      some (MIR.StorageBoundary.resumeWith tvWorldConfig journal frame slot word rest
        (proceed remainingWork (Nat.lt_succ_self _)))
    else none
  | _, _, _, _ => none

theorem storeSound (first second : Bool) : EVM.State.Reentry.Sound tvWorldConfig (routeStore first second) := by
  intro work proceed agrees journal frame result chosen
  simp only [routeStore] at chosen
  split at chosen
  · rename_i ready
    split at chosen
    · rename_i slot word rest stack
      split at chosen
      · contradiction
      rename_i fits
      have opcode : frame.code[frame.pc]? = some 0x55 := by
        rw [ready.1, ← EVM.State.CodeChunks.get_eq]
        rcases ready.2 with ⟨_,pc⟩ | ⟨_,pc⟩ <;> rw [pc] <;> decide +kernel
      cases chosen
      simp only [agrees]
      exact MIR.StorageBoundary.resume_eq tvWorldConfig journal frame slot word rest work
        opcode rfl (by decide) stack (by omega)
    · contradiction
  · contradiction

def route (lowered : Bool) : EVM.State.Reentry.Route :=
  EVM.State.Reentry.choose (routeStore true true) <|
    EVM.State.Reentry.choose (MorphoPacking.packedRoute lowered) (routeRecovery lowered)

theorem routeSound (lowered : Bool) : EVM.State.Reentry.Sound tvWorldConfig (route lowered) :=
  EVM.State.Reentry.choose_sound _ _ _ (storeSound true true)
    (EVM.State.Reentry.choose_sound _ _ _ (MorphoPacking.packedSound lowered) (recoverySound lowered))

theorem nested (work : Nat) (journal : Execution.Journal) (frame : EVM.State.Frame) :
    EVM.State.Reentry.executeWith tvWorldConfig (route false) work journal frame =
    EVM.State.Reentry.executeWith tvWorldConfig (route true) work journal frame := by
  rw [EVM.State.Reentry.execute_correspondence _ _ (routeSound false),
    EVM.State.Reentry.execute_correspondence _ _ (routeSound true)]

#print axioms executed
#print axioms contextual
#print axioms nested
"""


def certify(
    directory, site, layout, fixture, solar, executable, timeout, *, tables=None
):
    if tables is not None:
        authorization_lowering.source(
            *(table["setAuthorizationWithSig"] for table in tables)
        )
    for module in (MODULE, STORAGE):
        built = build_model(timeout, module)
        entry_proof.success(built, module)
    configuration = world.configuration(solar, "cancun", fixture, timeout, executable)
    packing = packing_boundary.certify(directory, layout, configuration, timeout)
    base = next(
        row["forks"]["cancun"]["base_gas"]
        for row in native.catalog(executable, timeout)["evmir"]
        if row["opcode"] == 0xFA
    )
    checked = entry_proof.lean(
        directory,
        NAME,
        source(site, configuration, base, packing),
        timeout,
        module=True,
    )
    output = entry_proof.success(checked, NAME)
    entry_proof.check_axioms(output.splitlines(), 3)
    return {
        "proof": f"kernel/{NAME}.lean",
        "gas_pc": site.gas,
        "call_pc": site.call,
        "store_pcs": [site.nonce, site.authorization],
        "packing_pc": layout.steps[0].pc,
        "packing_gas": layout.gas,
        "scope": "nested MIR signing envelope, recovery, and both metered stores in the emitted authorization body",
    }


def replay_source(state):
    root = state["address"]
    seed = {
        **state,
        "accounts": [
            {**account, "code": []} if account["address"] == root else account
            for account in state["accounts"]
        ],
    }
    warm = ", ".join(world.warm_addresses(state, "cancun"))
    slots = ", ".join(f"({owner},{key})" for owner, key in state["warm_slots"])
    return f"""import {NAME}
open SolarTV EvmRules EVM.State.Reentry
def accounts : List Execution.Account :=
  ({world.accounts_expression(seed)} : List Execution.Account).map fun account =>
    if account.address = {root} then {{account with code := code.bytes}} else account
def checkpoint : Execution.Journal := ⟨accounts,accounts,[{warm}],[{slots}],[],0⟩
def start : EVM.State.Frame :=
  ⟨{world.environment_expression(state)},code.bytes,0,[],[],[],{state["gas"]}⟩
def replay (pick : Route) : String :=
  EVM.State.describe tvWorldConfig .exact (executeWith tvWorldConfig pick 2048 checkpoint start)
def broken : Route := fun work execute journal frame =>
  (route true work execute journal frame).map (Except.map fun result =>
    {{result with stop := .reverted, data := [], «world» := journal}})
def brokenStore : Route
  | work+1, proceed, journal, frame => do
      if frame.pc != authorizationPC || frame.code != code.bytes then
        route true (work+1) proceed journal frame
      else
        let slot :: word :: rest := frame.stack | none
        some (MIR.StorageBoundary.resumeWith tvWorldConfig journal frame slot (word ^^^ 1) rest
          (proceed work (Nat.lt_succ_self _)))
  | _, _, _, _ => none
def brokenPacked : Route :=
  choose MorphoPacking.changedDomain (route true)
#eval IO.println ("[" ++ replay (route false) ++ "," ++ replay (route true) ++ "," ++
  replay broken ++ "," ++ replay brokenStore ++ "," ++ replay brokenPacked ++ "]")
"""
