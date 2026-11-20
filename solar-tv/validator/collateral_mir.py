"""Check both captured MIR paths through SupplyCollateral's packed write."""

from . import decoder_proof, market_proof, mir_state
from .callers import require
from .collateral_proof import (
    ASSETS,
    CURRENT,
    OWNER,
    PACKED,
    POSITION,
    POSITION_MAP,
    SUM,
)


def plan(tables, catalog, market):
    """Identify the actual declared guard, narrowing callee and SSTORE block."""
    definitions = {row["name"]: row for row in catalog.values()}
    pair = []
    for table, shape in zip(tables, market.mir, strict=True):
        function, narrow = table["supplyCollateral"], table["toUint128"]
        blocks = function["blocks"]
        assets = shape.present
        owner = blocks[assets]["terminator"]["then"]
        update = blocks[owner]["terminator"]["then"]
        guard_block, asset_block, owner_block, update_block = (
            blocks[index] for index in (shape.block, assets, owner, update)
        )
        updates = update_block["instructions"]
        actual = [
            item["operation"].get("name", item["operation"]["kind"])
            for item in updates[:14]
        ]
        require(
            actual
            == [
                "call",
                "mapping_slot",
                "word",
                "mapping_slot",
                "add",
                "sload",
                "shr",
                "low_bits",
                "checked",
                "and",
                "word",
                "shl",
                "or",
                "sstore",
            ]
            and updates[0]["operation"]["function"] == narrow["id"],
            "unexpected MIR collateral update",
        )
        narrow_exit = narrow["blocks"][0]["terminator"]["then"]
        pair.append(
            (
                narrow["blocks"][0]["instructions"][0]["result"],
                narrow["id"],
                [
                    (i, mir_state.block(b, definitions))
                    for i, b in (
                        (assets, asset_block),
                        (owner, owner_block),
                        (update, update_block),
                    )
                ],
                [
                    (i, mir_state.block(narrow["blocks"][i], definitions))
                    for i in (0, narrow_exit)
                ],
                [i["result"] for i in guard_block["instructions"][3:]]
                + [
                    asset_block["instructions"][0]["result"],
                    owner_block["instructions"][0]["result"],
                ]
                + [i["result"] for i in updates[:13]],
            )
        )
    return pair


MIR_DEFINITIONS = """
open MIR.State MIR.TransferFromCollateral

def collateralArguments (env : World.Environment) : List Value :=
  [MIR.TransferFromEntry.scalar 256 (collateralAssets env),
   MIR.TransferFromEntry.scalar 160 (collateralOwner env),
   .aggregate [MIR.TransferFromEntry.scalar 256 292,
     MIR.TransferFromEntry.scalar 256 (Execution.readWord env.input 260)]]

def collateralArena (env : World.Environment) : List Execution.Byte :=
  wordArena ([collateralOwner env,positionMapping env,288,0] ++ argumentWords env)

def mirCollateralWritten (env : World.Environment) (journal : Execution.Journal) : Execution.Journal :=
  {(collateralWritten env journal) with refund := journal.refund}
"""

MIR_ASSUMPTIONS = """(created : ¬ marketMissing (marketValue env journal))
    (assetsNonzero : collateralAssets env ≠ 0) (ownerNonzero : collateralOwner env ≠ 0)
    (ownerFits : (collateralOwner env).toNat < 2^160)
    (assetsFits : (collateralAssets env).toNat < 2^128)
    (sumFits : (collateralSum env journal).toNat < 2^128)
    (writable : env.isStatic = false)"""
MIR_ARGS = "env journal created assetsNonzero ownerNonzero ownerFits assetsFits sumFits writable"


def fragments(decoder, pair):
    yield (
        MIR_DEFINITIONS
        + f"""
theorem collateral_decoded {decoder_proof.input_binders(decoder)}
    (selected : MIR.TransferFromDispatch.selector env.input = selector) :
    decode selector caller {{mirCfg with environment := env}} journal =
      .ok (MIR.TransferFromMarket.mirStart 0 journal (argumentWords env) (collateralArguments env)).frame := by
  rw [typed_abi_frame env journal surplus room small offsetWord slice addresses mirCfg selected]
  rw [marketEntryFrame caller journal {" ".join(map(market_proof.word, decoder.memory[4:]))} _ _ _ (by decide)]
  have lengthWord : BitVec.ofNat 256 (Execution.readWord env.input {decoder.payload - 32}).toNat =
      Execution.readWord env.input {decoder.payload - 32} := by
    apply BitVec.eq_of_toNat_eq
    rw [BitVec.toNat_ofNat,Nat.mod_eq_of_lt (Execution.readWord env.input {decoder.payload - 32}).isLt]
  simp only [lengthWord] <;> rfl
"""
    )
    for label, (comparison, narrow, blocks, narrow_blocks, locals_) in zip(
        ("before", "after"), pair, strict=True
    ):
        assets, owner, update = [i for i, _ in blocks]
        lookup = [f"{label}MarketLookup"]
        out = f"""
def {label}NarrowBody : Function :=
  (MIR.State.function {label}Functions {narrow}).toOption.getD ⟨[],[],[],none⟩
theorem {label}NarrowLookup :
    MIR.State.function {label}Functions {narrow} = .ok {label}NarrowBody := rfl
theorem {label}NarrowParams : {label}NarrowBody.params = [.word 256] := rfl
theorem {label}NarrowReturns : {label}NarrowBody.returns = [.word 128] := rfl
"""
        lookup.append(f"{label}NarrowLookup")
        for suffix, body_blocks in (("Market", blocks), ("Narrow", narrow_blocks)):
            for index, code in body_blocks:
                name = f"{label}{suffix}Block{index}"
                lookup.append(name)
                out += f"""
theorem {name} : {label}{suffix}Body.blocks[{index}]? = some {code} := rfl
"""
        yield out

        names = [f"{label}MarketBlock2", *lookup]
        simp_rules = f"""
    mirStepsFrom,Execution.Trace.segment,transition,{",".join(names)},
    required,next,branch,advance,enter,MIR.State.read,readValue,readBits,assign,
    assignValue,instruction,put,effect,Pure.pure,Except.pure,Except.bind,Bind.bind,
    Value.fits,Value.word,ValueType.width,ValueType.same,MIR.TransferFromEntry.scalar,
    Scalar.ofWord,List.filter_cons"""
        typed = [
            (128, f"{market_proof.VALUE}.setWidth 128 |>.setWidth 256"),
            (1, "1"),
            (1, "1"),
            (1, "1"),
            (128, ASSETS),
            (256, "(positionMapping env)"),
            (256, OWNER),
            (256, POSITION_MAP),
            (256, POSITION),
            (256, CURRENT),
            (256, f"(Evm.shr 128 {CURRENT})"),
            (128, f"(Evm.shr 128 {CURRENT})"),
            (128, SUM),
            (256, f"(Evm.and {2**128 - 1} {CURRENT})"),
            (256, SUM),
            (256, f"(Evm.shl 128 {SUM})"),
            (256, PACKED),
        ]
        entries = [
            f"({local},MIR.TransferFromEntry.scalar {width} ({value}))"
            for local, (width, value) in zip(locals_, typed, strict=True)
        ]
        # Keep the call continuation and each reached frame available to the next proof.
        stages = [
            ("MarketPassed", assets, 2, False, 0, 2, 3),
            ("AssetPassed", owner, assets, False, 0, 3, 3),
            ("OwnerPassed", update, owner, False, 0, 4, 3),
            ("Called", update, owner, True, 1, 4, 2),
            ("ChildChecked", update, owner, True, 1, 4, 3),
            ("Narrowed", update, owner, True, 1, 5, 3),
            ("Loaded", update, owner, True, 6, 10, 5),
            ("Packed", update, owner, True, 13, 17, 7),
            ("Written", update, owner, True, 14, 17, 1),
        ]
        previous = f"{label}MarketMachine env journal (collateralArguments env)"
        completed = 10
        for title, block, predecessor, entered, index, count, width in stages:
            name = f"{label}Collateral{title}"
            positional = ",".join(reversed(entries[:count]))
            initial = f"{label}MarketMachine env journal (collateralArguments env)"
            if title in ("Loaded", "Packed", "Written"):
                memory = "collateralArena env"
                journal = (
                    "collateralWarmWorld env journal"
                    if title != "Written"
                    else "mirCollateralWritten env journal"
                )
            else:
                memory = "marketMemory (argumentWords env)"
                journal = "collateralMarketWorld env journal"
            extra, lemmas = mir_facts(title, label)
            stack = f"([{positional}] ++ ({initial}).frame.locals)"
            cursor = (
                f"{{function := {label}Entry, block := {block}, predecessor := some {predecessor}, "
                f"entered := {str(entered).lower()}, next := {index}}}"
            )
            frame = f"""{{({initial}).frame with locals := {stack}, memory := {memory}, «world» := {journal}}}"""
            if title in ("Called", "ChildChecked"):
                child = (
                    f"{{function := {narrow}}}"
                    if title == "Called"
                    else f"{{function := {narrow}, block := {narrow_blocks[1][0]}, predecessor := some 0}}"
                )
                child_locals = (
                    "[]"
                    if title == "Called"
                    else (f"[({comparison},MIR.TransferFromEntry.scalar 1 1)]")
                )
                machine = f"""let parent : Frame := {frame}
  ⟨{child},[⟨{cursor},parent,some {locals_[4]},.word 128⟩],
    {{parent with arguments := [MIR.TransferFromEntry.scalar 256 (collateralAssets env)], locals := {child_locals}}}⟩"""
            else:
                machine = f"⟨{cursor},[],{frame}⟩"
            definitions = f"""
def {name} (env : World.Environment) (journal : Execution.Journal) : Machine :=
  {machine}

theorem {name}Step (env : World.Environment) (journal : Execution.Journal) {MIR_ASSUMPTIONS} :
    mirStepsFrom {{mirCfg with environment := env}} {label}Functions
      ({previous}) {width} {1024 - completed} =
      .ok (.inr ({name} env journal)) := by
{extra}  simp (config := {{maxSteps := 150000}}) [{simp_rules},
    {label}MarketMachine,{previous.split()[0]},{name},collateralArguments,mirCfg,
    {",".join(lemmas)}] <;> rfl
"""
            yield definitions
            previous = name + " env journal"
            completed += width
        out = f"""
theorem {label}_mir_collateral (env : World.Environment) (journal : Execution.Journal)
    {MIR_ASSUMPTIONS} :
    mirSteps {{mirCfg with environment := env}} {label}Functions {label}Entry
      journal (argumentWords env) (collateralArguments env) {completed} =
      .ok (.inr ({previous})) := by
  unfold mirSteps
"""
        remaining = completed - 10
        out += f"""  exact (mirStepsContinue 10 {remaining} 1024
    ({label}_mir_market env journal (collateralArguments env))).trans <|
"""
        completed = 10
        for title, _, _, _, _, _, width in stages:
            name = f"{label}Collateral{title}Step"
            remaining -= width
            if remaining:
                out += f"    (mirStepsContinue {width} {remaining} {1024 - completed} ({name} {MIR_ARGS})).trans <|\n"
            else:
                out += f"    {name} {MIR_ARGS}\n"
            completed += width
        yield out
        yield f"""
theorem {label}_abi_collateral {decoder_proof.input_binders(decoder)} {MIR_ASSUMPTIONS}
    (selected : MIR.TransferFromDispatch.selector env.input = selector) :
    (do
      let frame ← decode selector caller {{mirCfg with environment := env}} journal
      mirStepsFrom {{mirCfg with environment := env}} {label}Functions ⟨{{function := {label}Entry}},[],frame⟩ {completed}) =
      .ok (.inr ({previous})) := by
  exact decodedSegment
    (collateral_decoded env journal surplus room small offsetWord slice addresses selected)
    ({label}_mir_collateral {MIR_ARGS})

#print axioms {label}_abi_collateral
"""
        yield f"""
theorem {label}_collateral_frame (env : World.Environment) (journal : Execution.Journal)
    (surplus : Nat) :
    let mir := ({previous}).frame
    let physical := suppliedCollateral env journal surplus
    mir.memory = physical.frame.memory ∧ mir.world = {{physical.world with refund := journal.refund}} :=
  by
    simp only [{label}CollateralWritten,suppliedCollateral,decoderFrame,collateralArena,
      argumentWords,mirCollateralWritten]
    constructor <;> first | rfl | trivial

#print axioms {label}_collateral_frame
"""


def mir_facts(stage, label):
    if stage == "MarketPassed":
        return (
            """
  have truncate (f : Frame) : operation {mirCfg with environment := env} f (.lowBits 128)
      [marketValue env journal] = value f ((marketValue env journal).setWidth 128 |>.setWidth 256) := rfl
  have kept : (Scalar.ofWord 128 ((marketValue env journal).setWidth 128 |>.setWidth 256)).word =
      ((marketValue env journal).setWidth 128 |>.setWidth 256) := marketScalar _
  have nonzero : ((marketValue env journal).setWidth 128 |>.setWidth 256) ≠ 0#256 :=
    fun zero => created ((marketWidenedZero _).mp zero)
  have zero : (Scalar.mk 128 (0#128)).word = 0#256 := rfl
  simp [mirCfg,value] at truncate
  unfold Scalar.ofWord at kept
""",
            ["truncate", "kept", "zero", "nonzero", "effect", "collateralMarketWorld"],
        )
    if stage == "AssetPassed":
        return (
            """
  have nonzero : collateralAssets env ≠ 0#256 := assetsNonzero
""",
            ["nonzero"],
        )
    if stage == "OwnerPassed":
        return (
            """
  have widened : (Scalar.ofWord 160 (collateralOwner env)).word = collateralOwner env :=
    scalarFits 160 _ ownerFits
  have nonzero : collateralOwner env ≠ 0#256 := ownerNonzero
  have zero : (Scalar.mk 160 (0#160)).word = 0#256 := rfl
  unfold Scalar.ofWord at widened
""",
            ["widened", "zero", "nonzero"],
        )
    if stage == "Called":
        return "", [
            "MIR.State.start",
            "typedWords",
            "callSignature",
            f"{label}NarrowParams",
            f"{label}NarrowReturns",
        ]
    if stage == "ChildChecked":
        return (
            """
  have compare (f : Frame) (activation : Nat) :
      operation {mirCfg with environment := env} f (.comparison .lt 256)
        [collateralAssets env,340282366920938463463374607431768211456] activation = value f 1 := by
    have less : (collateralAssets env).ult 340282366920938463463374607431768211456#256 = true := by
      simpa [BitVec.ult_eq_decide] using assetsFits
    change value f (widenBits (if (collateralAssets env).ult 340282366920938463463374607431768211456#256
      then 1#1 else 0#1)) = _
    rw [less]
    rfl
  simp [mirCfg,value] at compare
""",
            ["compare"],
        )
    if stage == "Narrowed":
        return (
            f"""
  have castBack (f : Frame) (activation : Nat) :
      operation {{mirCfg with environment := env}} f (.lowBits 128)
        [collateralAssets env] activation = value f (collateralAssets env) :=
    congrArg (value f) (scalarFits 128 _ assetsFits)
  have publishes (f : Frame) (result : Value) :
      publishReturns {{mirCfg with environment := env}} {label}NarrowBody [result] f = .ok f := rfl
  simp [mirCfg,value] at castBack
  simp [mirCfg] at publishes
""",
            [
                "castBack",
                "typedWords",
                "publishes",
                f"{label}NarrowReturns",
            ],
        )
    if stage == "Loaded":
        return (
            """
  have mapped (f : Frame) (mem : f.memory = marketMemory (argumentWords env)) :
      operation {mirCfg with environment := env} f (.slot .mapping) [marketId env,2] =
      value {f with memory := wordArena ([marketId env,2,288,0] ++ argumentWords env)}
        (positionMapping env) :=
    mapArena _ _ (marketId env) 3 (marketId env) 2 (288 :: 0 :: argumentWords env)
      (by simp [argumentWords]) mem
  have mappedAgain (f : Frame)
      (mem : f.memory = wordArena ([marketId env,2,288,0] ++ argumentWords env)) :
      operation {mirCfg with environment := env} f (.slot .mapping) [collateralOwner env,positionMapping env] =
      value {f with memory := collateralArena env} (collateralMapping env) :=
    mapArena _ _ (marketId env) 2 (collateralOwner env) (positionMapping env) (288 :: 0 :: argumentWords env)
      (by simp [argumentWords]) mem
  have ownerWide : (Scalar.ofWord 160 (collateralOwner env)).word = collateralOwner env :=
    scalarFits 160 _ ownerFits
  have extendOwner (f : Frame) : operation {mirCfg with environment := env} f .word [collateralOwner env] =
      value f (collateralOwner env) := rfl
  have addOne (f : Frame) : operation {mirCfg with environment := env} f (.binary .add 256)
      [collateralMapping env,1] = value f (collateralSlot env) := rfl
  simp [mirCfg,value] at mapped mappedAgain extendOwner addOne
  unfold Scalar.ofWord at ownerWide
""",
            [
                "mapped",
                "mappedAgain",
                "ownerWide",
                "extendOwner",
                "addOne",
                "readMarketStorage",
                "value",
                "collateralCurrent",
                "collateralWarmWorld",
            ],
        )
    if stage == "Packed":
        return (
            """
  let old := collateralCurrent env journal
  let hi := Evm.shr 128 old
  let total := collateralSum env journal
  have narrowHi : (Scalar.ofWord 128 hi).word = hi := scalarFits 128 hi (highFits old)
  have narrowAssets : (Scalar.ofWord 128 (collateralAssets env)).word = collateralAssets env :=
    scalarFits 128 _ assetsFits
  have narrowSum : (Scalar.ofWord 128 total).word = total := scalarFits 128 _ sumFits
  have shifted : MIR.shift .shr old 128#256 = hi := rfl
  have trunc (f : Frame) : operation {mirCfg with environment := env} f (.lowBits 128) [hi] =
      value f hi := congrArg (value f) narrowHi
  have sum (f : Frame) : operation {mirCfg with environment := env} f (.checked .add 128 false)
      [hi,collateralAssets env] = value f total := by
    rw [checkedAdd128 _ _ _ _ (highFits old) assetsFits (by simpa [hi,old,total,collateralSum,BitVec.add_comm] using sumFits)]
    simp [hi,old,total,collateralSum,BitVec.add_comm]
  have mask (f : Frame) : operation {mirCfg with environment := env} f (.binary .and 256)
      [old,340282366920938463463374607431768211455] =
      value f (Evm.and 340282366920938463463374607431768211455 old) := by
    simp [operation,MIR.eval,Word.binary,Evm.and,BitVec.and_comm] <;> rfl
  have extend (f : Frame) : operation {mirCfg with environment := env} f .word [total] = value f total := rfl
  have upper : MIR.shift .shl total 128#256 = Evm.shl 128 total := rfl
  have pack (f : Frame) : operation {mirCfg with environment := env} f (.binary .or 256)
      [Evm.and 340282366920938463463374607431768211455 old,Evm.shl 128 total] =
      value f (collateralWord env journal) := by
    simp [old,total,collateralWord,operation,MIR.eval,Word.binary,Evm.or,BitVec.or_comm] <;> rfl
  simp [mirCfg,value,old,hi,total] at trunc sum mask extend pack
  dsimp [hi,old,total,Scalar.ofWord] at shifted upper narrowHi narrowAssets narrowSum
""",
            [
                "shifted",
                "trunc",
                "sum",
                "mask",
                "extend",
                "upper",
                "pack",
                "narrowHi",
                "narrowAssets",
                "narrowSum",
            ],
        )
    assert stage == "Written"
    return (
        """
  have stored (f : Frame) (same : f.world = collateralWarmWorld env journal) :
      operation {mirCfg with environment := env} f (.storage false true)
        [collateralSlot env,collateralWord env journal] =
      effect {f with «world» := mirCollateralWritten env journal} := by
    simp [operation,storageOperation,writable,effect,same,mirCollateralWritten,
      collateralWritten,collateralWarmWorld,warmAgain,Execution.putAccount] <;> rfl
  simp [mirCfg,effect] at stored
""",
        ["stored"],
    )
