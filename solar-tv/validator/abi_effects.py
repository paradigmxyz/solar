"""Check a static MIR ABI adapter and its continuation as an effect tree."""

import hashlib
import tempfile
from copy import deepcopy
from pathlib import Path

from . import (
    audit,
    entry_proof,
    lower_abi,
    mir_effects,
    mir_state,
    native,
    prune,
    type_prune,
)
from .common import Program, Unsupported


def selected_body(module, name):
    functions = {body["id"]: body for body in module["functions"]}
    entries = [body for body in functions.values() if body["return_buffer"] == name]
    if len(entries) != 1:
        raise Unsupported("ABI effect proof requires an unambiguous entry")
    entry = entries[0]
    pending = [entry["id"]]
    reachable = {}
    while pending:
        id_ = pending.pop()
        if id_ in reachable:
            continue
        body = functions.get(id_)
        if body is None:
            raise Unsupported("ABI effect proof has an unknown callee")
        reachable[id_] = body
        for block in body["blocks"]:
            pending.extend(
                inst["operation"]["function"]
                for inst in block["instructions"]
                if inst["operation"]["kind"] == "call"
            )
    return {"entry_id": entry["id"], "functions": list(reachable.values())}, entry


def returndata_projections(model):
    """Resolve constant projections of locally constructed returndata slices."""
    model = deepcopy(model)
    for function in model["functions"]:
        for block in function["blocks"]:
            slices = {}
            instructions = []
            for inst in block["instructions"]:
                op = inst["operation"]
                if op["kind"] == "make_slice" and op["location"] == "returndata":
                    pointer, length = op["args"]
                    if (
                        inst["type"].get("kind") != "slice"
                        or inst["type"].get("location") != "returndata"
                        or pointer["type"].get("location") != "returndata"
                        or length["type"].get("bits") != 256
                        or "undefined" in {pointer["kind"], length["kind"]}
                    ):
                        raise Unsupported("invalid returndata slice construction")
                    slices[inst["result"]] = op["args"]
                    continue
                if op["kind"] == "extract_value":
                    aggregate = op["args"][0]
                    values = slices.get(aggregate.get("index"))
                    if (
                        aggregate["kind"] == "local"
                        and values is not None
                        and op["index"] in (0, 1)
                    ):
                        value = values[op["index"]]
                        if value["type"] != inst["type"]:
                            raise Unsupported("returndata projection changed its type")
                        inst = {**inst, "operation": {"kind": "word", "args": [value]}}
                instructions.append(inst)
            block["instructions"] = instructions
    return model


def guard_source(params, local_guard):
    """Compare the explicit decoder with calls into the typed source body."""
    words = [
        f"(Execution.readWord environment.input {4 + 32 * i})"
        for i in range(len(params))
    ]
    cases = []
    if local_guard:
        cases.append(("nonzero", "environment.value ≠ 0#256"))
    if params:
        short = f"environment.input.length < {4 + 32 * len(params)}"
        cases.append(("short", short))
    canonical = []
    for i, (word, ty) in enumerate(zip(words, params, strict=True)):
        bits = lower_abi.width(ty)
        if bits == 256:
            continue
        bad = f"{(1 << bits) - 1} < {word}.toNat"
        cases.append((f"bad{i}", bad))
        canonical.extend(
            [
                f"  have fit{i} : {word}.toNat < {1 << bits} := by omega",
                f"  have canonical{i} := cleanWidth {word} {bits} fit{i}",
                (
                    f"  have mask{i} : {word} &&& {(1 << bits) - 1}#256 = {word} :=\n"
                    f"    cleanMask {word} {bits} (by decide) fit{i}"
                ),
            ]
        )
    reject = " ∨ ".join(term for _, term in cases) or "False"
    splits = [
        f"  by_cases {id_} : {term}\n  · normalize_abi_tree bounded"
        for id_, term in cases
    ]
    return words, reject, "\n".join(splits + canonical)


def unchanged_callees(old, new):
    left, right = [{f["id"]: f for f in side["functions"]} for side in (old, new)]
    common = {
        id_
        for id_, f in left.items()
        if id_ != old["entry_id"] and id_ != new["entry_id"] and right.get(id_) == f
    }
    while True:
        closed = {
            id_
            for id_ in common
            if all(
                block["terminator"]["kind"] != "tail_call"
                and all(
                    inst["operation"]["kind"] != "call"
                    or inst["operation"]["function"] in common
                    for inst in block["instructions"]
                )
                for block in left[id_]["blocks"]
            )
        }
        if closed == common:
            ids = sorted(closed)
            return ids, [[table[id_] for id_ in ids] for table in (left, right)]
        common = closed


def internal_code(old, new, body, wrapper, definitions, version, prelude):
    if (body["params"], body["returns"]) != (wrapper["params"], wrapper["returns"]):
        raise Unsupported("decoder proof needs an unchanged internal interface")
    cuts, tables = unchanged_callees(old, new)
    decoder = any(
        inst["operation"]["kind"] == "abi_decode"
        for function in old["functions"]
        for block in function["blocks"]
        for inst in block["instructions"]
    )
    revert_data = any(
        block["terminator"]["kind"] == "revert_return_data"
        for function in old["functions"]
        for block in function["blocks"]
    )
    if not decoder and not revert_data and not cuts:
        raise Unsupported(
            "internal ABI proof needs a source decoder or retained callees"
        )
    trees = [
        mir_effects.render(
            returndata_projections(model), definitions, abi_fork=version, call_cuts=cuts
        )
        for model in (old, new)
    ]
    params = [
        (f"arg{i}", mir_effects.scalar_width(ty)) for i, ty in enumerate(body["params"])
    ]
    binders = " ".join(f"({name} : BitVec {bits})" for name, bits in params)
    args = " ".join(name for name, _ in params)
    callee_proof = ""
    if cuts:
        for name, table in zip(("leftCallees", "rightCallees"), tables, strict=True):
            entries = ",".join(mir_state.function_body(f, definitions) for f in table)
            callee_proof += f"def {name} : MIR.State.Functions := [{entries}]\n"
        callee_proof += """
theorem sameCallees : leftCallees = rightCallees := rfl
#print axioms sameCallees
"""
    code = (
        prelude
        + callee_proof
        + f"""
set_option maxRecDepth 16384
set_option maxHeartbeats 4000000
private theorem highHalf (word : Word) :
    ((word >>> 128).setWidth 128).setWidth 256 = word >>> 128 := by
  have fits : word.toNat >>> 128 < 2^128 := by
    rw [Nat.shiftRight_eq_div_pow]
    exact (Nat.div_lt_iff_lt_mul (Nat.two_pow_pos _)).mpr word.isLt
  have wide : word.toNat >>> 128 < 2^256 := Nat.lt_of_lt_of_le fits (by decide)
  apply BitVec.eq_of_toNat_eq
  simp only [BitVec.toNat_setWidth, BitVec.toNat_ushiftRight]
  rw [Nat.mod_eq_of_lt fits, Nat.mod_eq_of_lt wide]
def before (environment : World.Environment) {binders} : MIR.Effects.Tree :=
{trees[0]}
def after (environment : World.Environment) {binders} : MIR.Effects.Tree :=
{trees[1]}
theorem wrapped (environment : World.Environment) {binders} :
    before environment {args} = after environment {args} := by
  simp [before, after, MIR.Effects.decodeMemoryBool, MIR.Effects.abiDecodeFailure,
    MIR.Effects.abiRevertData,
    MIR.Effects.signedBoolHead, Decidable.imp_iff_not_or, MIR.eval, MIR.compare,
    MIR.shift, Word.shift, BitVec.ult_iff_toNat_lt, MIR.cast, MIR.select,
    MIR.wordBinary, MIR.unary, Word.unary, Word.binary, highHalf]
theorem contextual {{State Halt : Type}} (environment : World.Environment) {binders}
    (handler : MIR.Effects.Request → State → Except Halt (Word × State))
    (finish : MIR.Effects.Boundary → State → Halt) (state : State) :
    MIR.Effects.interpret handler finish (before environment {args}) state =
      MIR.Effects.interpret handler finish (after environment {args}) state :=
  MIR.Effects.contextual _ _ (wrapped environment {args}) handler finish state
#print axioms wrapped
#print axioms contextual
"""
    )
    return code, {
        "kind": "kernel-checked-abi-memory-decoder-effects"
        if decoder
        else "kernel-checked-abi-revert-data-effects"
        if revert_data
        else "kernel-checked-abi-internal-effects",
        "retained_callees": [f["return_buffer"] for f in tables[0]],
    }


def certificate(code, texts, function, version, timeout, evidence):
    from .check import build_model

    entry_proof.success(build_model(timeout), "ABI effect proof model")
    with tempfile.TemporaryDirectory(prefix="solar-tv-abi-effects-") as directory:
        output = entry_proof.success(
            entry_proof.lean(Path(directory), "Abi", code, timeout),
            "ABI contextual effect certificate",
        )
    entry_proof.check_axioms(
        output.splitlines(), 2 + bool(evidence.get("retained_callees"))
    )
    evidence = {**evidence, "proof_sha256": hashlib.sha256(code.encode()).hexdigest()}
    pair = tuple(
        Program(
            ("abi-effect-tree", function),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            scope="abi-effect-tree",
            selector=function,
            evm_version=version,
            import_evidence=evidence,
        )
        for text in texts
    )
    return audit.Result("verified", stdout="verified\n", program=pair[0], pair=pair)


def check(paths, solar, replay, version, timeout, function):
    from .check import PRELUDE, ROOT, run

    texts = []
    for path in paths:
        if path.suffix != ".mir" or path.stat().st_size > 1024 * 1024:
            raise Unsupported("ABI effect proof requires MIR files of at most 1 MiB")
        parsed = run(
            [
                solar,
                path.resolve(),
                "-Zmir-pipeline=none",
                "-Zvalidate-ir=true",
                f"--evm-version={version}",
            ],
            ROOT,
            timeout,
        )
        if parsed.returncode:
            raise Unsupported(parsed.stderr)
        texts.append(parsed.stdout)
    modules = [type_prune.export(text, replay, timeout) for text in texts]
    if any(
        module["library"] or module["dispatch_entry"] is not None for module in modules
    ):
        raise Unsupported("ABI effect proof needs an undispatched contract")
    (old, body), (new, wrapper) = [
        selected_body(module, function) for module in modules
    ]
    routes = [
        next((r for r in module["selectors"] if r["id"] == model["entry_id"]), None)
        for module, model in zip(modules, (old, new), strict=True)
    ]
    left, right = routes
    definitions = {row["name"]: row for row in native.catalog(replay, timeout)["evmir"]}
    if (
        not left
        and not right
        and all(
            model["entry_id"]
            not in {
                *module["runtime_entries"],
                module["constructor"],
                module["receive"],
                module["fallback"],
            }
            for module, model in zip(modules, (old, new), strict=True)
        )
    ):
        code, evidence = internal_code(
            old, new, body, wrapper, definitions, version, PRELUDE
        )
        return certificate(
            code,
            texts,
            function,
            version,
            timeout,
            evidence,
        )
    if (
        not left
        or not right
        or left["wrapper"]
        or right != {**left, "wrapper": True, "id": right["id"]}
        or wrapper["params"]
        or wrapper["returns"]
    ):
        raise Unsupported("ABI effect proof needs the same selected wrapper route")
    source = modules[0]
    runtime = source["selectors"] + [
        source[key] for key in ("receive_route", "fallback_route") if source[key]
    ]
    local_guard = not left["payable"] and any(row["payable"] for row in runtime)
    header = prune.declarations(texts[0])[1][function]["text"][0]
    params = lower_abi.layout(
        header, "abi_params", body["params"], arguments=True, limit=8
    )
    returns = lower_abi.layout(
        header, "abi_return_params", body["returns"], arguments=False
    )
    words, rejected, splits = guard_source(params, local_guard)
    inputs = [
        f"({word}.setWidth {ty['bits']})"
        for word, ty in zip(words, body["params"], strict=True)
    ]
    left_tree = mir_effects.render(
        old, definitions, abi_return=True, abi_arguments=inputs, abi_fork=version
    )
    right_tree = mir_effects.render(new, definitions, abi_fork=version)
    code = (
        PRELUDE
        + f"""
set_option maxHeartbeats 4000000
set_option maxRecDepth 16384
private theorem cleanWidth (word : Word) (width : Nat) (fits : word.toNat < 2^width) :
    (word.setWidth width).setWidth 256 = word := by
  apply BitVec.eq_of_toNat_eq
  simp [BitVec.toNat_setWidth, Nat.mod_eq_of_lt fits, Nat.mod_eq_of_lt word.isLt]
private theorem cleanMask (word : Word) (width : Nat) (within : width ≤ 256)
    (fits : word.toNat < 2^width) :
    word &&& BitVec.ofNat 256 (2^width - 1) = word := by
  have upper : 2^width ≤ 2^256 := Nat.pow_le_pow_right (by decide) within
  have maskFits : 2^width - 1 < 2^256 := by
    have positive := Nat.two_pow_pos width
    omega
  apply BitVec.eq_of_toNat_eq
  rw [BitVec.toNat_and, BitVec.toNat_ofNat, Nat.mod_eq_of_lt maskFits,
    Nat.and_two_pow_sub_one_eq_mod, Nat.mod_eq_of_lt fits]
def before (environment : World.Environment) : MIR.Effects.Tree :=
  if {rejected} then .done ⟨"revert",[(256,0),(256,0)]⟩ else
  (
{left_tree}
  )
def after (environment : World.Environment) : MIR.Effects.Tree :=
{right_tree}
macro "normalize_abi_tree" bound:term : tactic => `(tactic|
  simp [*, before, after, MIR.compare, MIR.eval, MIR.cast, MIR.shift, Word.shift,
    Word.binary, MIR.unary, Word.unary, MIR.select, MIR.wordBinary, MIR.ternary,
    BitVec.ofNatLT_eq_ofNat, BitVec.ult_iff_toNat_lt, BitVec.toNat_ofNat,
    Nat.mod_eq_of_lt $bound, MIR.Effects.abiReturn, MIR.Effects.writeAbiWords])
theorem wrapped (environment : World.Environment) (bounded : environment.input.length < 2^256) :
    before environment = after environment := by
{splits}
  normalize_abi_tree bounded
theorem contextual {{State Halt : Type}} (environment : World.Environment)
    (bounded : environment.input.length < 2^256)
    (handler : MIR.Effects.Request → State → Except Halt (Word × State))
    (finish : MIR.Effects.Boundary → State → Halt) (state : State) :
    MIR.Effects.interpret handler finish (before environment) state =
      MIR.Effects.interpret handler finish (after environment) state :=
  MIR.Effects.contextual _ _ (wrapped environment bounded) handler finish state
#print axioms wrapped
#print axioms contextual
"""
    )
    evidence = {
        "kind": "kernel-checked-abi-contextual-effects",
        "selector": left["selector"],
        "abi_params": params,
        "abi_returns": returns,
        "rejects_value_in_wrapper": local_guard,
    }
    return certificate(code, texts, function, version, timeout, evidence)
