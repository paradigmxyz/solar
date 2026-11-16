"""Compositional effect trees from independently parsed, scalar acyclic MIR.

Every non-scalar opcode remains an ordered request, with universally quantified
results. No memory/storage implementation is unfolded or assumed to succeed.
This is a separate certificate, not a full-state interpreter proof. Rejected
operations, undefined operands and excessive expansion never become tree nodes.
"""

import hashlib
import re
import tempfile
from dataclasses import replace
from pathlib import Path

from . import audit, mir_state, native
from . import operations as ops
from .common import Unsupported, literal


def scalar_width(ty):
    if ty.get("kind", "word") != "word":
        raise Unsupported("effect trees require scalar integer MIR")
    return int(mir_state.width(ty["bits"]))


def render(model, definitions):
    if len(model["functions"]) != 1:
        raise Unsupported("effect trees require a single call-free function")
    function = model["functions"][0]
    for ty in [*function["params"], *function["returns"]]:
        scalar_width(ty)
    blocks = function["blocks"]
    work = 0

    def operand(value, locals):
        bits = scalar_width(value["type"])
        match value["kind"]:
            case "constant":
                return literal(value["value"], bits), bits
            case "arg":
                index = value["index"]
                if not 0 <= index < len(function["params"]):
                    raise Unsupported("unknown effect-tree argument")
                return f"arg{index}", bits
            case "local":
                result = locals.get(value["index"])
                if result is None or result[1] != bits:
                    raise Unsupported("unavailable effect-tree SSA operand")
                return result
            case _:
                raise Unsupported("undefined effect-tree operand")

    def pure(op, args, bits):
        kind = op["kind"]
        values = [value for value, _ in args]
        if kind == "low_bits" and op["bits"] != bits:
            raise Unsupported("effect-tree low-bits representation mismatch")
        if kind in {"word", "low_bits"}:
            return f"({values[0]}).setWidth {bits}"
        if kind == "sign_extend":
            if op["from"] != args[0][1] or op["to"] != bits:
                raise Unsupported("effect-tree sign extension representation mismatch")
            return f"({values[0]}).signExtend {bits}"
        if kind == "ne":
            return f"MIR.compare .ne {values[0]} {values[1]}"
        if kind == "select":
            return f"MIR.select {values[0]} {values[1]} {values[2]}"
        if kind != "opcode" or not definitions[op["name"]]["pure"]:
            return None
        name = op["name"]
        if name in ops.COMPARISONS:
            return f"MIR.compare .{name} {' '.join(values)}"
        if name in ops.SHIFTS:
            # Schema order is amount,value; MIR.shift takes value,amount.
            return f"MIR.shift .{name} {values[1]} {values[0]}"
        if name in ops.WORD_BINARY:
            return f"MIR.wordBinary .{name} {' '.join(values)}"
        if name in ops.BINARY:
            return f"MIR.eval .{name} {' '.join(values)}"
        if name in {"not", "clz"}:
            return f"MIR.unary .{name} {values[0]}"
        if name in ops.TERNARY:
            return f"MIR.ternary .{name} {' '.join(values)}"
        raise Unsupported(f"unmodeled scalar effect-tree opcode `{name}`")

    # Check every operation, including unreachable blocks and unused values.
    # Unsupported semantics cannot disappear through lazy Lean let reduction.
    for block in blocks:
        if block["terminator"]["kind"] not in {
            "jump",
            "branch",
            "return",
            "returndata",
            "revert",
            "stop",
            "invalid",
        }:
            raise Unsupported("unmodeled effect-tree terminator")
        for phi in block["phis"]:
            scalar_width(phi["type"])
            for _, value in phi["incoming"]:
                scalar_width(value["type"])
                if value["kind"] == "undefined":
                    raise Unsupported("undefined effect-tree operand")
        for inst in block["instructions"]:
            op = inst["operation"]
            if inst["type"] is not None:
                scalar_width(inst["type"])
            if op["kind"] == "opcode":
                row = definitions[op["name"]]
                if (
                    row["observes_code"]
                    or row["observes_gas"]
                    and row["gas"] != "SStore"
                    or row["sets_returndata"]
                ):
                    raise Unsupported(
                        "effect trees exclude code/gas observers and calls"
                    )
                if row["pure"]:
                    pure(op, [("0#256", 256)] * len(op["args"]), 256)
            elif op["kind"] not in {"word", "low_bits", "sign_extend", "ne", "select"}:
                raise Unsupported(f"unmodeled effect-tree operation `{op['kind']}`")
            for value in op["args"]:
                scalar_width(value["type"])
                if value["kind"] == "undefined":
                    raise Unsupported("undefined effect-tree operand")
        for value in block["terminator"].get("args", []):
            scalar_width(value["type"])
            if value["kind"] == "undefined":
                raise Unsupported("undefined effect-tree operand")

    def enter(index, predecessor, locals, path):
        nonlocal work
        if index in path:
            raise Unsupported("effect-tree cycles require an invariant")
        path = (*path, index)
        block = blocks[index]
        current = dict(locals)
        # Simultaneous phis read the incoming environment, never sibling phis.
        for phi in block["phis"]:
            selected = [value for pred, value in phi["incoming"] if pred == predecessor]
            if len(selected) != 1:
                raise Unsupported("effect-tree phi predecessor mismatch")
            current[phi["result"]] = operand(selected[0], locals)
        prefix, close = [], []
        for inst in block["instructions"]:
            work += 1
            if work > 4096:
                raise Unsupported("effect-tree expansion exceeds 4096 instructions")
            op = inst["operation"]
            args = [operand(value, current) for value in op["args"]]
            bits = scalar_width(inst["type"]) if inst["type"] is not None else 256
            expression = pure(op, args, bits)
            if expression is None:
                if any(width != 256 for _, width in args):
                    raise Unsupported("effect-tree raw operands require i256")
                row = definitions[op["name"]]
                returns = inst["result"] is not None
                if row["stack"] != [len(args), int(returns)]:
                    raise Unsupported("effect-tree opcode signature mismatch")
                values = ",".join(value for value, _ in args)
                prefix.append(
                    f"MIR.Effects.Tree.request ⟨{row['opcode']},[{values}],{'true' if returns else 'false'}⟩ (fun reply{work} =>"
                )
                close.append(")")
                expression = f"reply{work}.setWidth {bits}"
            if inst["result"] is not None:
                name = f"local{work}"
                prefix.append(f"let {name} : BitVec {bits} := {expression}\n")
                current[inst["result"]] = name, bits
            elif expression is not None and op["kind"] != "opcode":
                raise Unsupported("effect-tree scalar instruction lacks a result")
        term = block["terminator"]
        args = [operand(value, current) for value in term.get("args", [])]

        def edge(target):
            return "(\n" + enter(target, index, current, path) + "\n)"

        match term["kind"]:
            case "jump":
                result = edge(term["target"])
            case "branch":
                if len(args) != 1 or args[0][1] != 1:
                    raise Unsupported("effect-tree branch requires i1")
                result = f"if {args[0][0]} = 1 then {edge(term['then'])} else {edge(term['else'])}"
            case "return" | "returndata" | "revert" | "stop" | "invalid":
                values = ",".join(
                    f"({bits},({value}).setWidth 256)" for value, bits in args
                )
                result = f'MIR.Effects.Tree.done ⟨"{term["kind"]}",[{values}]⟩'
            case _:
                raise Unsupported("unmodeled effect-tree terminator")
        return "\n".join([*prefix, result, *reversed(close)])

    result = enter(0, None, {}, ())
    if len(result) > 262144:
        raise Unsupported("effect-tree text exceeds 256 KiB")
    return result


def check(a, b, timeout, executable):
    from .check import PRELUDE, ROOT, build_model, run

    definitions = {
        row["name"]: row for row in native.catalog(executable, timeout)["evmir"]
    }
    trees = [render(program.symbolic_mir["model"], definitions) for program in (a, b)]
    build = build_model(timeout)
    if build.returncode:
        raise Unsupported(build.stdout + build.stderr)
    binders = " ".join(
        f"({arg} : BitVec {bits})"
        for arg, bits in zip(a.inputs, a.input_widths, strict=True)
    )
    args = " ".join(a.inputs)
    code = PRELUDE + "set_option maxHeartbeats 1000000\n"
    for name, tree in zip(("before", "after"), trees, strict=True):
        code += f"def {name} {binders} : MIR.Effects.Tree :=\n{tree}\n"
    code += f"""
theorem pair_correct {binders} : before {args} = after {args} := by
  simp [before, after, MIR.compare, MIR.eval, MIR.cast, MIR.shift, Word.shift,
    Word.binary, MIR.unary, Word.unary, MIR.select, MIR.wordBinary, MIR.ternary,
    BitVec.ofNatLT_eq_ofNat, BitVec.ult]
#print axioms pair_correct
"""
    with tempfile.TemporaryDirectory(prefix="solar-tv-mir-effects-") as directory:
        path = Path(directory) / "Pair.lean"
        path.write_text(code)
        result = run(["lake", "env", "lean", path], ROOT, timeout)
    # Reuse the same strict axiom policy as direct SSA proofs.
    axioms = re.search(r"'pair_correct' depends on axioms: \[(.*?)\]", result.stdout)
    if result.returncode or not (
        "'pair_correct' does not depend on any axioms" in result.stdout
        or axioms
        and not set(axioms[1].split(", "))
        - {"propext", "Quot.sound", "Classical.choice"}
    ):
        raise Unsupported(
            "compositional MIR effect proof did not close\n"
            + result.stdout
            + result.stderr
        )
    pair = tuple(
        replace(
            program,
            scope="effect-tree",
            import_evidence={
                **program.import_evidence,
                "kind": "independent-scalar-effect-tree",
                "effect_tree_sha256": hashlib.sha256(tree.encode()).hexdigest(),
                "boundary": "trusted compiler parser/schema export and scalar effect-tree renderer; contextual theorem covers arbitrary stateful request handlers, not full-state interpreter correspondence",
            },
        )
        for program, tree in zip((a, b), trees, strict=True)
    )
    return audit.Result("verified", stdout="verified\n", program=pair[0], pair=pair)
