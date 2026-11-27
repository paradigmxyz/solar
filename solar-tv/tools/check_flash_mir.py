"""Link both frozen FlashLoan MIR captures to the pinned runtime."""

import argparse
import json
from dataclasses import replace
from functools import partial
from pathlib import Path

from check_flash_complete import (
    CALLBACK_CALL,
    CALLER,
    REPAYMENT_CALL,
    SCENARIOS,
    digest,
    fixture_for,
    observe,
)
from validator import (
    audit,
    entry_proof,
    heap_entry,
    mir_state,
    native,
    type_prune,
    world,
)
from validator.common import Program

CALLERS = ("safeTransfer", "flashLoan", "safeTransferFrom")
RETURN_PCS = (3600, 6478, 3112)
VERSION = "cancun"
FUEL = 512
TIMEOUT = 120
# The callback stores keccak256(msg.data) in slot zero.
HASH_CALLBACK = "365f5f37365f205f5500"


def bodies(model):
    return {fn["return_buffer"]: fn for fn in model["functions"]}


def closure(model):
    table = {fn["id"]: fn for fn in model["functions"]}
    pending = [bodies(model)["flashLoan"]["id"]]
    seen = set()
    while pending:
        index = pending.pop()
        if index in seen:
            continue
        seen.add(index)
        for block in table[index]["blocks"]:
            ops = [row["operation"] for row in block["instructions"]] + [
                block["terminator"]
            ]
            pending.extend(op["function"] for op in ops if "function" in op)
    # The checked runtime dispatcher selects the retained FlashLoan closure.
    seen.add(model["dispatch_entry"])
    return [table[index] for index in sorted(seen)]


def gas_sites(model):
    table = bodies(model)
    sites = []
    for name in CALLERS:
        ((block, instruction),) = (
            (block, index)
            for block, body in enumerate(table[name]["blocks"])
            for index, row in enumerate(body["instructions"])
            if row["operation"].get("name") == "gas"
        )
        sites.append({"function": name, "block": block, "instruction": instruction})
    return sites


def replay_pair(texts, image, fixture, scenario, sites, executable):
    result, callback, repayment = observe(image, fixture, executable, scenario)
    count = 2 if repayment is None else 3
    cuts = [
        heap_entry.observe(image, fixture, VERSION, executable, 30, pc=pc)
        for pc in RETURN_PCS[:count]
    ]
    gas_samples = [
        {**site, "gas": gas}
        for site, gas in zip(sites[:count], cuts[-1]["samples"], strict=True)
    ]
    budgets = cuts[-1]["budgets"]
    remaining = [cut["gas"] for cut in cuts]
    state = {
        **world.with_runtime(fixture, image),
        "gas": None,
        "gas_policy": None,
        "message_gas": budgets,
        "memory": {"size": 96, "data": [0] * 95 + [128]},
    }
    programs = [
        Program(
            ("world-entry", "mir"),
            [],
            "",
            "",
            replay_ir=text,
            selector="entry",
            evm_version=VERSION,
            world=state,
        )
        for text in texts
    ]
    replayed = native.replay(
        *programs, [], executable=executable, timeout=30, mir_gas_samples=gas_samples
    )
    observations = [json.loads(observation) for observation in replayed["observations"]]
    assert observations[0] == observations[1]
    record = observations[0]
    (root,) = (
        account for account in record["accounts"] if account[0] == fixture["address"]
    )
    assert bytes(root[3]) == image
    # Raw-runtime replay omits the installed root code from its projection.
    accounts = [
        [
            *account[:3],
            [] if account[0] == fixture["address"] else account[3],
            account[4],
        ]
        for account in record["accounts"]
    ]
    assert [record["outcome"], record["data"]] == result[:2]
    assert accounts == result[4] and record["logs"] == result[5]
    assert record["message_gas"] == remaining
    return {
        "evm": result,
        "mir": observations,
        "callback": callback,
        "repayment": repayment,
        "samples": gas_samples,
        "budgets": budgets,
        "returned_gas": remaining,
    }


def check_mir(directory, scenario, models, catalog, remaining):
    namespace = "FlashLoanComplete" + scenario.name
    physical = namespace + "MemobyteTailReached"
    imported = namespace + "Mir"
    prove = partial(entry_proof.lean, directory, namespace=namespace)
    available = [
        row["opcode"] for row in catalog.values() if row["forks"][VERSION]["available"]
    ]
    header = entry_proof.HEADER + "open SolarTV.Execution EVM.State EvmRules\n"
    source = f"""import {physical}
{header}
def flashMirConfig : MIR.State.Config :=
  {{fork := .cancun, environment := env, context := tvWorldConfig.block, available := {available},
    entry := true, externalConfig := some tvWorldConfig}}
def flashMirInitial : MIR.State.Frame :=
  ⟨checkpoint,MIR.TransferFromABI.wordArena [0,0,128],[],[],[],0,0,[]⟩
"""
    labels = ("flashBefore", "flashAfter")
    for label, model in zip(labels, models, strict=True):
        functions = ",\n".join(
            mir_state.function_body(fn, catalog) for fn in closure(model)
        )
        source += f"""
def {label}Functions : MIR.State.Functions := [{functions}]
def {label}Start := pricedMirStart {label}Functions {model["dispatch_entry"]} flashMirInitial byteTailCut
def {label}Step := pricedMirStep flashMirConfig {label}Functions byteTailCut Keccak.hash
"""
    entry_proof.success(prove(imported, source, TIMEOUT, module=True), imported)
    modules, counts = [], []
    for label in labels:
        name = imported + label + "Proposal"
        recipe = f"""import {imported}
{header}export_prefix {label}Step from {label}Start shared (code.bytes) world env
  width {entry_proof.WIDTH} fuel {FUEL} skip 0 limit 128
"""
        proposal = json.loads(entry_proof.success(prove(name, recipe, TIMEOUT), name))
        assert proposal[1] is not None
        module, segments = entry_proof.segments(
            prove,
            imported,
            label,
            "Except String PricedMirCursor",
            "MIR.State.Result",
            FUEL,
            proposal,
            TIMEOUT,
        )
        modules.append(module)
        counts.append(segments)
    source = (
        entry_proof.import_modules(modules)
        + "\n"
        + header
        + """
def publicMIR (mir : MIR.State.Result) : Except String EVM.State.Observation := do
  let final ← MIR.State.conclude flashMirConfig flashMirInitial (.ok mir)
  let (stop,data) ← match final.completion with
    | .success data => .ok (Stop.success,data)
    | .reverted data => .ok (Stop.reverted,data)
    | .invalid => .ok (Stop.exception,[])
    | .outOfGas => .ok (Stop.outOfGas,[])
    | .returned _ => .error "unexpected MIR return at public exit"
  return EVM.State.observe tvWorldConfig .effects ⟨stop,data,final.world,0,0⟩

def publicImage : EVM.State.Observation :=
  EVM.State.observe tvWorldConfig .effects (finishByte checkpoint byteTailCut.result)
"""
    )
    source += f"""
theorem imageMatches :
    (runByteExit tvWorldConfig {FUEL} start).toOption.map
      (EVM.State.observe tvWorldConfig .effects ∘ finishByte checkpoint ∘ ByteExit.result) =
        some publicImage := by
  unfold runByteExit
  change (Trace.run (byteExitStep tvWorldConfig code.bytes) {FUEL} byteTailStart).toOption.map _ = _
  rw [← chunkedByteExitStep_eq tvWorldConfig code, ← {namespace}Memo.hashChecked_eq]
  change (Trace.run byteTailStep {FUEL} byteTailStart).toOption.map _ = _
  rw [byteTailReached]
  rfl
"""
    for label in labels:
        source += f"""
theorem {label}Matches :
    (do
      let result ← Trace.run {label}Step {FUEL} {label}Start
      publicMIR result).toOption =
    (runByteExit tvWorldConfig {FUEL} start).toOption.map
      (EVM.State.observe tvWorldConfig .effects ∘ finishByte checkpoint ∘ ByteExit.result) := by
  rw [imageMatches, {label}Reached]
  change (publicMIR {label}Cut).toOption = some publicImage
  decide +kernel

#print axioms {label}Matches
"""
    source += f"""
theorem returnedGas :
    flashBeforeCut.messageGas = {remaining} ∧ flashAfterCut.messageGas = {remaining} := by
  decide +kernel

#print axioms returnedGas
"""
    name = namespace + "MirLink"
    result = prove(name, source, TIMEOUT)
    (directory / f"{name}.log").write_text(result.stdout + result.stderr)
    entry_proof.check_axioms(entry_proof.success(result, name).splitlines(), 3)
    return {
        "segments": counts,
        "proof_sha256": digest((directory / f"{name}.lean").read_bytes()),
    }


def handoff_source(name, pc, function, block, index, instruction, catalog, values):
    expr = mir_state.instruction(instruction, catalog)
    reads = [
        f"(read{field} : MIR.State.read mir {mir_state.operand(operand)} = .ok request.{field})"
        for operand, field in zip(instruction["operation"]["args"], values, strict=True)
        if field is not None
    ]
    zeros = [
        f"show MIR.State.read mir {mir_state.operand(operand)} = .ok 0 by rfl"
        for operand, field in zip(instruction["operation"]["args"], values, strict=True)
        if field is None
    ]
    assumptions = "\n    ".join(reads)
    arguments = ["read" + field for field in values if field is not None]
    equations = ", ".join([*arguments, *dict.fromkeys(zeros)])
    return f"""
def {name}Site : MIR.State.Cursor :=
  {{function := {function}, block := {block}, next := {index}, entered := true}}
def {name}Row : MIR.State.Instruction := {expr}

private theorem {name}Operands (cfg : MIR.State.Config) (mir : MIR.State.Frame)
    (request : CallHandoff.Request)
    {assumptions} :
    MIR.State.messageArguments cfg mir (.raw 241) {name}Row.operands =
      .ok (241,request.args,mir,some request.gas) := by
  simp [{name}Row, MIR.State.messageArguments, MIR.State.required, CallHandoff.Request.args, List.mapM_cons,
    Bind.bind, Except.bind, Pure.pure, Except.pure, {equations}]

theorem {name} (steps : Nat) (cfg : MIR.State.Config) (evm : EVM.State.Frame)
    (mir : MIR.State.Frame) (request : CallHandoff.Request) (rest : List Word) (budgets : List Nat)
    (fork : cfg.fork = .cancun) (externalCfg : cfg.externalConfig = some tvWorldConfig)
    (env : cfg.environment = evm.environment) (depth : evm.environment.depth < 1024)
    (available : cfg.available.contains 241 = true)
    (pc : evm.pc = {pc}) (image : evm.code = code.bytes)
    (shape : evm.stack = request.gas :: (request.args ++ rest)) (height : rest.length ≤ 1017)
    (valid : request.gas.toNat ≤ 30000000)
    (input : expand evm.memory request.offset.toNat request.length.toNat = .ok evm.memory)
    (mirInput : expand mir.memory request.offset.toNat request.length.toNat = .ok mir.memory)
    (payload : request.payload mir.memory = request.payload evm.memory)
    (ledger : cfg.messageGas = some budgets) (selected : budgets[mir.messageGas.length]? = some evm.gas)
    {assumptions} :
    CallHandoff.physical steps tvWorldConfig evm mir.world =
      CallHandoff.loweredAt steps cfg mir Functions {name}Site := by
  have found : code.get? evm.pc = some 241 := by rw [pc]; decide +kernel
  have bounded : evm.pc < code.size := by rw [pc, dispatch_size]; decide
  have row : instAt Functions {name}Site = some {name}Row := rfl
  unfold CallHandoff.loweredAt
  rw [row]
  simp only [MIR.State.required, Bind.bind, Except.bind, {name}Row, Option.any_some,
    available, Bool.not_true, Bool.false_eq_true, ite_false]
  exact CallHandoff.agrees steps cfg tvWorldConfig code evm mir request rest budgets (.raw 241) _
    fork rfl externalCfg env depth rfl (by decide +kernel) bounded found image shape height valid
    input mirInput payload ledger selected ({name}Operands cfg mir request {" ".join(arguments)})

#print axioms {name}
"""


def check_handoffs(directory, models, catalog):
    module = "FlashLoanCompleteSuccessMir"
    header = entry_proof.HEADER + "open SolarTV.Execution SolarTV.MIR EvmRules\n"
    proofs = []
    for label, model in zip(("flashBefore", "flashAfter"), models, strict=True):
        source = f"""import {module}
import SolarTV.MIR.Transforms.CallHandoff
import SolarTV.MIR.Transforms.FlashCallback
{header}
abbrev Functions := FlashLoanCompleteSuccess.{label}Functions
"""
        for name, pc, body, block, index in (
            ("callback", CALLBACK_CALL, "flashLoan", 5, 0),
            ("repayment", REPAYMENT_CALL, "safeTransferFrom", 2, 18),
        ):
            function = bodies(model)[body]
            row = function["blocks"][block]["instructions"][index]
            # The theorem checks the actual operand reads in order. The value and
            # both output operands must reduce to literal zero.
            values = ("gas", "destination", None, "offset", "length", None, None)
            source += handoff_source(
                name, pc, function["id"], block, index, row, catalog, values
            )
        source += "\n#print axioms FlashCallback.stagedBytes\n"
        name = module + label + "Handoffs"
        result = entry_proof.lean(directory, name, source, TIMEOUT)
        (directory / f"{name}.log").write_text(result.stdout + result.stderr)
        entry_proof.check_axioms(entry_proof.success(result, name).splitlines(), 3)
        proofs.append(digest((directory / f"{name}.lean").read_bytes()))
    return proofs


def check_image(directory, image):
    name = "FlashLoanEmittedImage"
    source = (
        "import CheckedFlashLoanCall\n"
        + entry_proof.HEADER
        + "namespace Emitted\n"
        + entry_proof.image_definitions(image)
        + """end Emitted

theorem emittedImage : code.bytes = Emitted.code.bytes := rfl
#print axioms emittedImage
"""
    )
    result = entry_proof.lean(directory, name, source, TIMEOUT)
    entry_proof.check_axioms(entry_proof.success(result, name).splitlines(), 1)
    return digest((directory / f"{name}.lean").read_bytes())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for flag in (
        "before",
        "after",
        "baseline",
        "world",
        "compiler",
        "replay",
        "output",
    ):
        parser.add_argument("--" + flag, type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    receipt = json.loads((output / "flash-complete-replay.json").read_text())
    image = bytes.fromhex(
        json.loads(args.baseline.read_text())["contracts"]["src/Morpho.sol"]["Morpho"][
            "evm"
        ]["deployedBytecode"]["object"]
    )
    replay = str(args.replay.resolve())
    assert receipt["status"] == "verified" and receipt["runtime_sha256"] == digest(
        image
    )
    assert receipt["replay_sha256"] == digest(args.replay.read_bytes())
    captures = [path.read_bytes() for path in (args.before, args.after)]
    texts = [data.decode() for data in captures]
    models = [type_prune.export(text, replay, 30) for text in texts]
    sites = gas_sites(models[0])
    assert gas_sites(models[1]) == sites
    catalog = {row["name"]: row for row in native.catalog(replay, 30)["evmir"]}
    fixture = world.read(args.world, str(args.compiler.resolve()), "exact")
    image_proof = check_image(output, image)
    scenarios = {}
    for scenario in SCENARIOS:
        name = "FlashLoanComplete" + scenario.name + "Proof"
        source = (output / f"{name}.lean").read_text()
        old = receipt["scenarios"][scenario.name]
        assert old["proof_sha256"] == digest(source.encode())
        proof = entry_proof.lean(output, name, source, TIMEOUT)
        entry_proof.check_axioms(entry_proof.success(proof, name).splitlines(), 1)
        record = replay_pair(
            texts, image, fixture_for(scenario, fixture), scenario, sites, replay
        )
        assert record["evm"] == old["exit"]
        record.update(
            check_mir(output, scenario, models, catalog, record["returned_gas"])
        )
        scenarios[scenario.name] = record
        print(f"{scenario.name}: emitted bytes and both MIR captures agree", flush=True)
    handoffs = check_handoffs(output, models, catalog)
    padding = {}
    for length in (0, 1, 31, 32, 33, 65):
        scenario = replace(
            SCENARIOS[0],
            callback=HASH_CALLBACK,
            payload=bytes(range(1, length + 1)),
            caller_slot=None,
        )
        record = replay_pair(
            texts, image, fixture_for(scenario, fixture), scenario, sites, replay
        )
        padding[length] = {
            key: record[key] for key in ("callback", "repayment", "returned_gas")
        }
        caller = next(
            account for account in record["evm"][4] if int(account[0]) == CALLER
        )
        padding[length]["callback_hash"] = dict(caller[4])["0"]
    assert len({data["callback_hash"] for data in padding.values()}) == len(padding)
    print(
        "Checked both CALL boundaries in Lean and replayed six payload lengths",
        flush=True,
    )
    (output / "flash-mir-correspondence.json").write_text(
        json.dumps(
            {
                "status": "verified",
                "semantics": audit.semantics(),
                "runtime_sha256": digest(image),
                "replay_sha256": receipt["replay_sha256"],
                "compiler_sha256": digest(args.compiler.read_bytes()),
                "world_sha256": digest(args.world.read_bytes()),
                "captures": [digest(data) for data in captures],
                "image_proof_sha256": image_proof,
                "handoffs_sha256": handoffs,
                "padding": padding,
                "scenarios": scenarios,
            },
            indent=2,
        )
        + "\n"
    )


if __name__ == "__main__":
    main()
