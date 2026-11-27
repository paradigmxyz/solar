"""Compare typed narrow integer functions with their widened word carriers."""

import re
import subprocess
import sys

from . import abi_effects, mir_cfg, mir_effects, native, prune
from .common import Unsupported


def lowered(ty):
    if ty["kind"] == "word" and 1 < ty["bits"] < 256:
        return {**ty, "bits": 256, "name": "i256"}
    return ty


def interface(source, target):
    return all(
        [lowered(ty) for ty in source[key]] == target[key]
        for key in ("params", "returns")
    )


def binds(old, new):
    left, right = [{f["id"]: f for f in model["functions"]} for model in (old, new)]
    if left.keys() != right.keys() or old["entry_id"] != new["entry_id"]:
        raise Unsupported("integer lowering changed the direct call closure")
    for index, body in left.items():
        target = right[index]
        if target["return_buffer"] != body["return_buffer"] or not interface(
            body, target
        ):
            raise Unsupported(
                "integer lowering requires the corresponding word interface"
            )
    return left[old["entry_id"]]


def masks(models, cuts):
    used = set()
    for model in models:
        for body in model["functions"]:
            if body["id"] in cuts:
                continue
            for block in body["blocks"]:
                for inst in block["instructions"]:
                    op = inst["operation"]
                    if op["kind"] != "opcode" or op["name"] != "and":
                        continue
                    for value in op["args"]:
                        if (
                            value["kind"] != "constant"
                            or value["type"].get("bits") != 256
                        ):
                            continue
                        number = int(value["value"])
                        bits = number.bit_length()
                        if 0 < bits < 256 and number == 2**bits - 1:
                            used.add(bits)
    return sorted(used)


def proof(old, new, definitions, fork, prelude, cuts=(), call_widths=None):
    body = binds(old, new)
    widths = [mir_effects.scalar_width(ty) for ty in body["params"]]
    return_widths = [mir_effects.scalar_width(ty) for ty in body["returns"]]
    variables = [f"arg{i}" for i in range(len(widths))]
    target_arguments = [
        f"({word}.setWidth 256)" if 1 < bits < 256 else word
        for word, bits in zip(variables, widths, strict=True)
    ]
    old_tree = mir_effects.render(
        abi_effects.product_projections(old),
        definitions,
        abi_fork=fork,
        abi_arguments=variables,
        return_widths=return_widths,
        call_cuts=cuts,
        call_result_widths=call_widths,
    )
    new_tree = mir_effects.render(
        abi_effects.product_projections(new),
        definitions,
        abi_fork=fork,
        abi_arguments=target_arguments,
        return_widths=return_widths,
        call_cuts=cuts,
        call_result_widths=call_widths,
    )
    used = masks((old, new), cuts)
    mask_lemmas = "".join(
        f"""
theorem mask{width} (value : Word) :
    value &&& {2**width - 1}#256 = (value.setWidth {width}).setWidth 256 :=
  MIR.LowerIntegers.widenLow value {width} (by decide)
"""
        for width in used
    )
    names = ", ".join(f"mask{width}" for width in used)
    if names:
        names = ", " + names
    binders = " ".join(
        f"({word} : BitVec {bits})"
        for word, bits in zip(variables, widths, strict=True)
    )
    arguments = " ".join(variables)
    return (
        prelude
        + mask_lemmas
        + f"""
set_option maxRecDepth 16384
set_option maxHeartbeats 5000000
def before (environment : World.Environment) (heapFloor : Word) {binders} : MIR.Effects.Tree :=
{old_tree}
def after (environment : World.Environment) (heapFloor : Word) {binders} : MIR.Effects.Tree :=
{new_tree}
theorem same (environment : World.Environment) (heapFloor : Word) {binders} :
    before environment heapFloor {arguments} = after environment heapFloor {arguments} := by
  simp [before, after, MIR.Effects.abiRevertData, MIR.LowerAlloc.freePointerRequest,
    MIR.compare, MIR.eval, MIR.cast, MIR.shift, Word.shift, MIR.select, MIR.wordBinary,
    MIR.unary, Word.unary, Word.binary, MIR.ternary, MIR.LowerIntegers.eqWiden,
    MIR.LowerIntegers.eqWidenNat, MIR.LowerIntegers.oneBit,
    MIR.LowerIntegers.ultWiden, MIR.Effects.highHalfWord, MIR.Effects.highHalfNat,
    beq_iff_eq, BitVec.setWidth_setWidth_of_le, BitVec.setWidth_setWidth,
    BitVec.setWidth_and, BitVec.setWidth_add, BitVec.setWidth_mul, BitVec.add_assoc,
    Nat.mod_mod, BitVec.toNat_ofNat{names}] <;>
    simp only [MIR.LowerIntegers.oneBit]
theorem contextual {{State Halt : Type}} (environment : World.Environment) (heapFloor : Word) {binders}
    (handle : MIR.Effects.Request → State → Except Halt (Word × State))
    (finish : MIR.Effects.Boundary → State → Halt) (state : State) :
    MIR.Effects.interpret handle finish (before environment heapFloor {arguments}) state =
      MIR.Effects.interpret handle finish (after environment heapFloor {arguments}) state :=
  MIR.Effects.contextual _ _ (same environment heapFloor {arguments}) handle finish state
#print axioms same
#print axioms contextual
"""
    )


def cut_proof(old, new, definitions, fork, prelude):
    binds(old, new)

    def calls(model):
        return {
            body["id"]: {
                inst["operation"]["function"]
                for block in body["blocks"]
                for inst in block["instructions"]
                if inst["operation"]["kind"] == "call"
            }
            for body in model["functions"]
        }

    graph = calls(old)
    if mir_cfg.recursive(graph) or mir_cfg.recursive(calls(new)):
        raise Unsupported("integer lowering needs acyclic direct calls")
    ids = set(graph)
    widths = {
        body["id"]: mir_effects.scalar_width(body["returns"][0])
        for body in old["functions"]
        if len(body["returns"]) == 1
    }
    certificates = []
    for index in graph:
        parts = [{**model, "entry_id": index} for model in (old, new)]
        body = proof(*parts, definitions, fork, "", sorted(ids - {index}), widths)
        certificates.append(
            f"namespace IntegerBody{index}\n{body}\nend IntegerBody{index}\n"
        )
    return prelude + "".join(certificates)


NARROW = re.compile(
    r"\b(?:" + "|".join(f"i{bits}" for bits in range(8, 256, 8)) + r")\b"
)


def candidate(paths, function):
    if any(
        path.suffix != ".mir" or path.stat().st_size > 1024 * 1024 for path in paths
    ):
        return False
    tables = [prune.declarations(path.read_text())[1] for path in paths]
    if any(function not in table for table in tables):
        return False
    before, after = ("\n".join(table[function]["text"]) for table in tables)
    return bool(NARROW.search(before)) and not NARROW.search(after)


def check(paths, solar, replay, version, timeout, function, *, verbose=False):
    from .check import PRELUDE

    try:
        if not candidate(paths, function):
            return None
        texts, _, modules = abi_effects.typed_pair(
            paths, solar, replay, version, timeout
        )
        left, right = (
            {key: value for key, value in module.items() if key != "functions"}
            for module in modules
        )
        if left != right:
            raise Unsupported("integer lowering changed the entry bindings")
        old, new = [
            abi_effects.selected_body(module, function)[0] for module in modules
        ]
        definitions = {
            row["name"]: row for row in native.catalog(replay, timeout)["evmir"]
        }
        try:
            code = proof(old, new, definitions, version, PRELUDE)
            callees = 0
        except Unsupported:
            code = cut_proof(old, new, definitions, version, PRELUDE)
            callees = len(old["functions"]) - 1
        return abi_effects.certificate(
            code,
            texts,
            function,
            version,
            timeout,
            {
                "kind": "kernel-checked-integer-width-effects",
                "checked_callees": callees,
            },
            scope="integer-width",
        )
    except (Unsupported, OSError, subprocess.TimeoutExpired) as error:
        if verbose:
            print(f"Integer width proof: {error}", file=sys.stderr)
        return None
