"""Compare materialized MIR encoders with the ordered ABI operations."""

from . import abi_effects, abi_zero_copy, mir_cfg, mir_effects, native, prune
from .common import Unsupported

ENCODERS = {"abi_encode", "abi_encode_tree"}


def word(argument):
    value, bits = argument
    if bits != 256:
        raise Unsupported("ABI encoder requires a word carrier")
    return value


def cleanup(rule):
    match rule["kind"]:
        case "any":
            return ".word"
        case "uint" if 0 < rule["bits"] <= 256:
            return f"(.uint {rule['bits']})"
        case "boolean":
            return ".boolean"
        case _:
            raise Unsupported("ABI encoder cleanup has an unmodeled scalar rule")


def fields_for(shape, argument):
    value = word(argument)
    if shape["kind"] == "word":
        return [f".word {value}"]
    if shape["kind"] != "tuple" or any(
        field["kind"] != "word" for field in shape["fields"]
    ):
        raise Unsupported("ABI encoder needs static scalar tuple fields")
    result = []
    for index, field in enumerate(shape["fields"]):
        pointer = f"({value} + {32 * index}#256)"
        result.append(f".memory {pointer} {cleanup(field['rule'])}")
    return result


def encoder(op, args, bits, work):
    cursor = 0
    base, selector = "none", "none"
    if op["base"]:
        base = f"(some {word(args[cursor])})"
        cursor += 1
    selector_value = None
    if op["selector"]:
        selector_value = word(args[cursor])
        selector = f"(some {selector_value})"
        cursor += 1
    values = args[cursor:]
    mode = op["mode"]
    if (mode, bits) not in {(1, 256), (2, 512)} or mode == 1 and op["base"]:
        raise Unsupported("ABI encoder needs a scratch slice or an allocated object")
    if op["kind"] == "abi_encode":
        if any(op["functions"]) or len(op["functions"]) != len(values):
            raise Unsupported("ABI encoder needs scalar word leaves")
        fields = [f".word {word(value)}" for value in values]
    else:
        if len(op["shapes"]) != len(values) or any(op["memory_slices"]):
            raise Unsupported("ABI encoder has invalid layout operands")
        if (
            op["shapes"]
            == [
                {"kind": "word", "rule": {"bits": 0, "function": False, "kind": "any"}},
                {"kind": "bytes", "location": 1},
            ]
            and mode == 2
            and not op["base"]
            and selector_value is not None
        ):
            data, width = values[1]
            if width != 512 or op["args"][-1]["type"].get("location") != "calldata":
                raise Unsupported("ABI encoder needs a typed calldata byte range")
            return (
                (
                    f"MIR.LowerAbiEncode.bytesScratch {selector_value} {word(values[0])} "
                    f"({data}.extractLsb' 256 256) ({data}.extractLsb' 0 256) "
                    f"(fun encoded{work} length{work} =>"
                ),
                f"(encoded{work} ++ length{work})",
            )
        fields = [
            field
            for shape, value in zip(op["shapes"], values, strict=True)
            for field in fields_for(shape, value)
        ]
    plan = "[" + ",".join(fields) + "]"
    if mode == 1:
        return (
            f"MIR.LowerAbiEncode.object {selector} {plan} (fun encoded{work} =>",
            f"encoded{work}",
        )
    return (
        f"MIR.LowerAbiEncode.scratch {base} {selector} {plan} (fun encoded{work} length{work} =>",
        f"(encoded{work} ++ length{work})",
    )


def encodes(body):
    return any(
        inst["operation"]["kind"] in ENCODERS
        for block in body["blocks"]
        for inst in block["instructions"]
    )


def function_code(models, body, definitions, fork, cuts):
    params = [mir_effects.scalar_width(ty) for ty in body["params"]]
    binders = " ".join(f"(arg{i} : BitVec {bits})" for i, bits in enumerate(params))
    args = " ".join(f"arg{i}" for i in range(len(params)))
    trees = [
        mir_effects.render(
            abi_effects.product_projections(model),
            definitions,
            abi_fork=fork,
            call_cuts=cuts,
            abi_encoder=encoder if side == 0 else None,
        )
        for side, model in enumerate(models)
    ]
    if any(
        encodes(other) for other in models[1]["functions"] if other["id"] not in cuts
    ):
        raise Unsupported(
            "ABI encoder lowering left an encoder outside a checked callee"
        )
    return f"""
set_option maxRecDepth 16384
set_option maxHeartbeats 5000000
def before (environment : World.Environment) {binders} : MIR.Effects.Tree :=
{trees[0]}
def after (environment : World.Environment) {binders} : MIR.Effects.Tree :=
{trees[1]}
theorem same (environment : World.Environment) {binders} :
    before environment {args} = after environment {args} := by
  simp [before, after, MIR.LowerAbiEncode.scratch, MIR.LowerAbiEncode.object,
    MIR.LowerAbiEncode.bytesScratch, MIR.LowerAbiEncode.load, MIR.LowerAbiEncode.store,
    MIR.LowerAbiEncode.copyCalldata, MIR.LowerAbiEncode.reserve, MIR.LowerAbiEncode.writeStatic,
    MIR.LowerAbiEncode.writeFields, MIR.LowerAbiEncode.Cleanup.apply, MIR.LowerAbiEncode.selectorSize,
    MIR.Effects.abiRevertData, MIR.LowerAlloc.freePointerRequest, MIR.compare, MIR.eval,
    MIR.cast, MIR.shift, Word.shift, MIR.select, MIR.wordBinary, MIR.unary, Word.unary,
    Word.binary, MIR.ternary, BitVec.extractLsb'_append_eq_left, BitVec.extractLsb'_append_eq_right,
    BitVec.setWidth_append_eq_right, BitVec.add_assoc, Nat.mod_mod, BitVec.toNat_ofNat,
    MIR.LowerAbiEncode.cancelBase, MIR.LowerAbiEncode.sliceStart, MIR.LowerAbiEncode.sliceSize, MIR.Effects.highHalfWord, MIR.Effects.highHalfNat] <;>
    simp only [MIR.LowerAbiEncode.cancelBase, BitVec.sub_add_cancel, BitVec.reduceSub]
theorem preserved {{State Halt : Type}} (environment : World.Environment) {binders}
    (handle : MIR.Effects.Request → State → Except Halt (Word × State))
    (finish : MIR.Effects.Boundary → State → Halt) (state : State) :
    MIR.Effects.interpret handle finish (before environment {args}) state =
      MIR.Effects.interpret handle finish (after environment {args}) state :=
  MIR.Effects.contextual _ _ (same environment {args}) handle finish state
#print axioms same
#print axioms preserved
"""


def code(modules, function, definitions, fork, prelude):
    (old, body), (new, target) = [
        abi_effects.selected_body(module, function) for module in modules
    ]
    if not encodes(body):
        raise Unsupported("selected MIR source has no ABI encoder")
    for model in (old, new):
        graph = {
            body["id"]: {
                inst["operation"]["function"]
                for block in body["blocks"]
                for inst in block["instructions"]
                if inst["operation"]["kind"] == "call"
            }
            for body in model["functions"]
        }
        if mir_cfg.recursive(graph):
            raise Unsupported("ABI encoder proof requires acyclic changed calls")
    by_name = {callee["return_buffer"]: callee for callee in old["functions"]}
    remaining = [
        name for name, callee in by_name.items() if encodes(callee) and name != function
    ]
    # Emit each relevant body once. Every semantic encoder in the reachable
    # call graph has its own proof, with a checked body at both bindings.
    retained, tables = abi_effects.unchanged_callees(old, new)
    proof = abi_effects.retained_callee_code(tables, definitions) if retained else ""
    cuts = sorted({*retained, *(by_name[name]["id"] for name in remaining)})
    for name in [*remaining, function]:
        (left, source), (right, target) = [
            abi_effects.selected_body(module, name) for module in modules
        ]
        if (source["id"], source["params"], source["returns"]) != (
            target["id"],
            target["params"],
            target["returns"],
        ):
            raise Unsupported(
                "ABI encoder lowering changed the declared function binding"
            )
        local_cuts = [index for index in cuts if index != source["id"]]
        part = function_code((left, right), source, definitions, fork, local_cuts)
        proof += f"namespace Encoder{source['id']}\n{part}\nend Encoder{source['id']}\n"
    return prelude + proof, cuts


def check(paths, solar, replay, version, timeout, function):
    from .check import PRELUDE

    raw = [prune.declarations(path.read_text()) for path in paths]
    sections = [functions.get(function, {}).get("text", []) for _, functions in raw]
    counts = [
        sum(" = abi_encode [" in line for line in section) for section in sections
    ]
    if counts[0] <= counts[1]:
        raise Unsupported("selected ABI encoder lowering did not remove an encoder")

    texts, declarations, modules = abi_effects.typed_pair(
        paths, solar, replay, version, timeout
    )
    tables = [functions for _, functions in declarations]
    if tables[0][function]["text"][0] != tables[1][function]["text"][0]:
        raise Unsupported("ABI encode lowering changed the function header")
    pair = [abi_effects.selected_body(module, function)[0] for module in modules]
    definitions = {row["name"]: row for row in native.catalog(replay, timeout)["evmir"]}
    try:
        proof, evidence = abi_zero_copy.proof(pair, definitions, version, PRELUDE)
        kind = "kernel-checked-abi-encode-zero-copy"
    except Unsupported:
        proof, cuts = code(modules, function, definitions, version, PRELUDE)
        evidence = {"checked_callees": cuts}
        kind = "kernel-checked-abi-encode-effects"
    return abi_effects.certificate(
        proof,
        texts,
        function,
        version,
        timeout,
        {"kind": kind, **evidence},
        scope="abi-encode",
    )
