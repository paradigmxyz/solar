"""Compositional effect trees from independently parsed, acyclic MIR.

Every non-scalar opcode remains an ordered request, with universally quantified
results. No memory/storage implementation is unfolded or assumed to succeed.
This is a separate certificate, not a full-state interpreter proof. Rejected
operations, undefined operands and excessive expansion never become tree nodes.
Supplied internal bodies expand with private SSA environments and explicit
return continuations. Memory pointers retain their word carrier, not an assumed
validity or freshness proof. Allocation remains a fallible stateful request.
FMP reads use the kernel-proved MLOAD request constructor. Constructor-range
queries keep their base/end identity and arbitrary reply; other layout queries
remain unsupported, rather than receiving a default placement.
"""

import hashlib
import json
import re
import tempfile
from collections import Counter
from dataclasses import replace
from pathlib import Path

from . import audit, mir_state, native
from . import operations as ops
from .common import Unsupported, literal


def scalar_width(ty):
    if ty.get("kind") == "pointer" and ty.get("location") == "memory":
        if ty.get("bits") != 256:
            raise Unsupported("effect-tree memory pointers require a word carrier")
        return 256
    if ty.get("kind", "word") != "word":
        raise Unsupported("effect trees require integers or memory pointers")
    return int(mir_state.width(ty["bits"]))


def low_masks(model):
    counts = Counter()
    for function in model["functions"]:
        for block in function["blocks"]:
            for inst in block["instructions"]:
                op = inst["operation"]
                if op["kind"] != "opcode" or op["name"] != "and":
                    continue
                for value in op["args"]:
                    if value["kind"] != "constant" or value["type"]["kind"] != "word":
                        continue
                    mask = int(value["value"])
                    bits = mask.bit_length()
                    width = int(mir_state.width(value["type"]["bits"]))
                    if 0 < bits < width and mask == (1 << bits) - 1:
                        counts[width, bits] += 1
    return counts


def pure(op, args, bits, definitions):
    """Render the shared domain of total scalar operations, or retain an effect."""
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
    if kind == "pointer_add":
        if bits != 256 or any(width != 256 for _, width in args):
            raise Unsupported("effect-tree pointer offsets require word carriers")
        return f"MIR.eval .add {' '.join(values)}"
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


ABI_MESSAGE_REQUESTS = {"call", "callcode", "delegatecall", "staticcall"}
CHECK_REQUESTS = {
    "check",
    "checked",
    "require_short",
    "require_encode",
    "require_literal",
}
TYPED_REQUESTS = CHECK_REQUESTS | {"extcall", "returndata_bytes"}


def abi_context_read(name, args, bits):
    match name:
        case "address" | "caller" | "origin":
            return f"environment.{name}.setWidth {bits}"
        case "callvalue":
            return "environment.value"
        case "calldatasize":
            return "BitVec.ofNat 256 environment.input.length"
        case "calldataload":
            return f"Execution.readWord environment.input ({args[0][0]}).toNat"
    return None


def encoder_layout(op):
    return json.dumps(
        json.dumps({k: v for k, v in op.items() if k != "args"}, sort_keys=True)
    )


def word_pairs(args):
    for value, bits in args:
        if bits == 512:
            yield f"({value}).extractLsb' 256 256", 256
            yield f"({value}).extractLsb' 0 256", 256
        else:
            yield value, bits


def word_pair(ty):
    return (
        ty.get("kind") == "product"
        and len(ty.get("fields", ())) == 2
        and all(t.get("kind") == "word" and t.get("bits") == 256 for t in ty["fields"])
    )


def check_pair_initialization(function):
    from .dead_ssa import reads

    blocks = function["blocks"]
    all_reads = reads(blocks, reject_undefined=False)
    for block in blocks:
        insts = block["instructions"]
        for i, inst in enumerate(insts):
            op = inst["operation"]
            if op["kind"] != "insert_value" or op["args"][0]["kind"] != "undefined":
                continue
            if i + 1 == len(insts) or all_reads.count(inst["result"]) != 1:
                raise Unsupported("first product field escapes before initialization")
            other = insts[i + 1]["operation"]
            if (
                other["kind"] != "insert_value"
                or other["index"] == op["index"]
                or other["args"][0]["kind"] != "local"
                or other["args"][0].get("index") != inst["result"]
            ):
                raise Unsupported("first product field escapes before initialization")


def render(
    model,
    definitions,
    *,
    abi_return=False,
    abi_arguments=None,
    abi_fork=None,
    call_cuts=(),
    bytes_return=False,
    abi_encoder=None,
    return_widths=None,
    call_result_widths=None,
    inline_effects=False,
):
    def width(ty):
        if inline_effects and word_pair(ty):
            return 512
        if (
            abi_fork
            and ty.get("kind") == "slice"
            and ty.get("location") in {"calldata", "memory", "returndata"}
        ):
            return 512
        if (
            abi_fork
            and ty.get("kind") == "pointer"
            and ty.get("location") in {"calldata", "returndata"}
            and ty.get("bits") == 256
        ):
            return 256
        return scalar_width(ty)

    functions = {function["id"]: function for function in model["functions"]}
    if len(functions) != len(model["functions"]) or model["entry_id"] not in functions:
        raise Unsupported("invalid effect-tree function table")
    work = 0
    depth = 0

    def bounded(render):
        def visit(*args):
            nonlocal depth
            if depth >= 128:
                raise Unsupported("effect-tree expansion exceeds nesting 128")
            depth += 1
            try:
                return render(*args)
            finally:
                depth -= 1

        return visit

    def operand(value, locals, arguments):
        bits = width(value["type"])
        match value["kind"]:
            case "constant":
                return literal(value["value"], bits), bits
            case "arg":
                index = value["index"]
                if not 0 <= index < len(arguments) or arguments[index][1] != bits:
                    raise Unsupported("unknown effect-tree argument")
                return arguments[index]
            case "local":
                result = locals.get(value["index"])
                if result is None or result[1] != bits:
                    raise Unsupported("unavailable effect-tree SSA operand")
                return result
            case _:
                raise Unsupported("undefined effect-tree operand")

    @bounded
    def enter(function, index, predecessor, locals, arguments, calls, path, returned):
        nonlocal work
        if index in path:
            raise Unsupported("effect-tree cycles require an invariant")
        path = (*path, index)
        block = function["blocks"][index]
        current = dict(locals)
        # Simultaneous phis read the incoming environment, never sibling phis.
        for phi in block["phis"]:
            selected = [value for pred, value in phi["incoming"] if pred == predecessor]
            if len(selected) != 1:
                raise Unsupported("effect-tree phi predecessor mismatch")
            current[phi["result"]] = operand(selected[0], locals, arguments)
        work += 1 + len(block["phis"])

        @bounded
        def walk(start, current):
            nonlocal work
            prefix, close = [], []

            def wrap(result):
                return "\n".join([*prefix, result, *reversed(close)])

            for position in range(start, len(block["instructions"])):
                inst = block["instructions"][position]
                work += 1
                if work > 4096:
                    raise Unsupported("effect-tree expansion exceeds 4096 instructions")
                op = inst["operation"]
                args = [
                    ("0#512", 512)
                    if inline_effects
                    and op["kind"] == "insert_value"
                    and i == 0
                    and value["kind"] == "undefined"
                    else operand(value, current, arguments)
                    for i, value in enumerate(op["args"])
                ]
                bits = width(inst["type"]) if inst["type"] is not None else 256
                if op["kind"] == "call" and op["function"] not in call_cuts:
                    callee = functions[op["function"]]
                    if callee["id"] in calls:
                        raise Unsupported(
                            "recursive effect-tree calls require an invariant"
                        )
                    if len(calls) >= 32:
                        raise Unsupported("effect-tree call expansion exceeds depth 32")

                    def resume(
                        values,
                        *,
                        result_id=inst["result"],
                        result_bits=bits,
                        next_position=position + 1,
                    ):
                        restored = dict(current)
                        if result_id is not None:
                            if len(values) != 1 or values[0][1] != result_bits:
                                raise Unsupported("effect-tree call result mismatch")
                            restored[result_id] = values[0]
                        return walk(next_position, restored)

                    return wrap(
                        enter(
                            callee,
                            0,
                            None,
                            {},
                            args,
                            (*calls, callee["id"]),
                            (),
                            resume,
                        )
                    )
                if abi_encoder and op["kind"] in {"abi_encode", "abi_encode_tree"}:
                    opening, value = abi_encoder(op, args, bits, work)
                    prefix.append(opening)
                    close.append(")")
                    current[inst["result"]] = value, bits
                    continue
                if abi_fork and op["kind"] == "abi_decode":
                    prefix.append(
                        f"MIR.Effects.decodeMemoryBool {args[0][0]} (fun decoded{work} =>"
                    )
                    close.append(")")
                    current[inst["result"]] = f"decoded{work}", 1
                    continue
                if op["kind"] == "call":
                    values = ",".join(f"({value}).setWidth 256" for value, _ in args)
                    returns = str(inst["result"] is not None).lower()
                    prefix.append(
                        "MIR.Effects.Tree.request "
                        f"⟨.internalCall {op['function']},[{values}],{returns}⟩ "
                        f"(fun reply{work} =>"
                    )
                    close.append(")")
                    if inst["result"] is not None:
                        canonical = (
                            call_result_widths[op["function"]]
                            if call_result_widths is not None
                            else bits
                        )
                        current[inst["result"]] = (
                            f"((reply{work}.setWidth {canonical}).setWidth {bits})",
                            bits,
                        )
                    continue
                expression = pure(op, args, bits, definitions)
                if abi_fork and op["kind"] == "make_slice":
                    expression = f"({args[0][0]} ++ {args[1][0]})"
                if inline_effects and op["kind"] == "insert_value":
                    base, value = args[0][0], args[1][0]
                    expression = (
                        f"(({value}) ++ ({base}).extractLsb' 0 256)"
                        if op["index"] == 0
                        else f"(({base}).extractLsb' 256 256 ++ ({value}))"
                    )
                if abi_fork and op["kind"] == "extract_value":
                    start = 256 if op["index"] == 0 else 0
                    expression = f"({args[0][0]}).extractLsb' {start} 256"
                if abi_fork and op["kind"] == "abi_encode_tree" and bits == 512:
                    words = []
                    for value, width_ in args:
                        if width_ == 512:
                            words.extend(
                                [
                                    f"({value}).extractLsb' 256 256",
                                    f"({value}).extractLsb' 0 256",
                                ]
                            )
                        else:
                            words.append(value)
                    values = ",".join(words)
                    layout = encoder_layout(op)
                    prefix.extend(
                        [
                            f"MIR.Effects.Tree.request ⟨.abiTreeScratch {layout},[{values}],true⟩ (fun ptr{work} =>",
                            f"MIR.Effects.Tree.request ⟨.abiTreeScratchSize {layout},[{values}],true⟩ (fun len{work} =>",
                        ]
                    )
                    close.extend([")", ")"])
                    expression = f"ptr{work} ++ len{work}"
                if (
                    return_widths is not None
                    and op["kind"] == "layout"
                    and op["query"] == "heap_floor"
                ):
                    expression = "heapFloor"
                if abi_fork and op["kind"] == "opcode":
                    expression = abi_context_read(op["name"], args, bits) or expression
                if expression is None:
                    typed_request = inline_effects and op["kind"] in TYPED_REQUESTS
                    if any(width != 256 for _, width in args) and not typed_request:
                        raise Unsupported(
                            "effect-tree raw operands require word carriers"
                        )
                    returns = inst["result"] is not None
                    if op["kind"] == "free_pointer":
                        request = "MIR.LowerAlloc.freePointerRequest"
                    elif op["kind"] == "layout" and op["query"] == "constructor":
                        request = f"(MIR.LowerAlloc.constructorRequest {'true' if op['end'] else 'false'})"
                    elif op["kind"] == "allocate":
                        operation = f".allocate {'true' if op['zeroed'] else 'false'} {'true' if op['checked'] else 'false'}"
                    elif typed_request:
                        signature = {
                            "operand_types": [opnd["type"] for opnd in op["args"]],
                            "result_type": inst["type"],
                        }
                        widths = ",".join(str(width) for _, width in args)
                        operation = (
                            f".typedRequest {encoder_layout(op | signature)} [{widths}]"
                        )
                        args = [
                            (f"({value}).setWidth 256", 256)
                            for value, _ in word_pairs(args)
                        ]
                    elif op["kind"] == "mapping_slot":
                        operation = ".mappingSlot"
                    elif op["kind"] == "abi_encode_tree":
                        operation = f".abiTreeScratch {encoder_layout(op)}"
                    elif op["kind"] == "abi_encode":
                        selector = str(op["selector"]).lower()
                        operation = (
                            f".abiWordObject {selector}"
                            if op["mode"] == 1
                            else f".abiWordScratch {str(op['base']).lower()} {selector}"
                        )
                    elif op["kind"] == "layout":
                        access = (
                            "immutableRead"
                            if op["query"] == "immutable_read"
                            else "immutableStore"
                        )
                        operation = f".{access} {op['id']}"
                    else:
                        row = definitions[op["name"]]
                        if row["stack"] != [len(args), int(returns)]:
                            raise Unsupported("effect-tree opcode signature mismatch")
                        operation = f".opcode {row['opcode']}"
                    if (
                        op["kind"] != "free_pointer"
                        and op.get("query") != "constructor"
                    ):
                        values = ",".join(value for value, _ in args)
                        request = (
                            f"⟨{operation},[{values}],{'true' if returns else 'false'}⟩"
                        )
                    prefix.append(
                        f"MIR.Effects.Tree.request {request} (fun reply{work} =>"
                    )
                    close.append(")")
                    expression = f"reply{work}.setWidth {bits}"
                if inst["result"] is not None:
                    name = f"local{work}"
                    prefix.append(f"let {name} : BitVec {bits} := {expression}\n")
                    current[inst["result"]] = name, bits
                elif (
                    expression is not None
                    and op["kind"] not in {"opcode", "layout"}
                    and not (inline_effects and op["kind"] in TYPED_REQUESTS)
                ):
                    raise Unsupported("effect-tree scalar instruction lacks a result")
            term = block["terminator"]
            args = [
                operand(value, current, arguments) for value in term.get("args", [])
            ]

            def edge(target):
                return (
                    "(\n"
                    + enter(
                        function,
                        target,
                        index,
                        current,
                        arguments,
                        calls,
                        path,
                        returned,
                    )
                    + "\n)"
                )

            match term["kind"]:
                case "jump":
                    result = edge(term["target"])
                case "branch":
                    if len(args) != 1 or args[0][1] != 1:
                        raise Unsupported("effect-tree branch requires i1")
                    result = f"if {args[0][0]} = 1 then {edge(term['then'])} else {edge(term['else'])}"
                case "return":
                    if len(args) != len(function["returns"]) or any(
                        bits != width(ty)
                        for (_, bits), ty in zip(args, function["returns"], strict=True)
                    ):
                        raise Unsupported("effect-tree return signature mismatch")
                    result = returned(args)
                case "abi_return_bytes" if bytes_return:
                    result = bytes_boundary(args)
                case "revert_return_data" if abi_fork:
                    result = "MIR.Effects.abiRevertData"
                case "returndata" | "revert" | "stop" | "invalid":
                    result = boundary(term["kind"], args)
                case _:
                    raise Unsupported("unmodeled effect-tree terminator")
            return wrap(result)

        return walk(0, current)

    def boundary(kind, args):
        if inline_effects and kind == "return":
            args = list(word_pairs(args))
        if any(bits > 256 for _, bits in args):
            raise Unsupported("effect-tree boundary requires scalar carriers")
        values = ",".join(f"({bits},({value}).setWidth 256)" for value, bits in args)
        return f'MIR.Effects.Tree.done ⟨"{kind}",[{values}]⟩'

    def bytes_boundary(args):
        if len(args) != 1 or args[0][1] != 256:
            raise Unsupported("effect-tree bytes return requires a memory word")
        return f"bytesReturn {args[0][0]}"

    def validate_block(block):
        terminator = block["terminator"]["kind"]
        revert_data = (
            terminator == "revert_return_data"
            and abi_fork
            and all(
                definitions[name]["forks"][abi_fork]["available"]
                for name in ("returndatasize", "returndatacopy")
            )
        )
        if not (
            revert_data or bytes_return and terminator == "abi_return_bytes"
        ) and terminator not in {
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
            width(phi["type"])
            for _, value in phi["incoming"]:
                width(value["type"])
                if value["kind"] == "undefined":
                    raise Unsupported("undefined effect-tree operand")
        for inst in block["instructions"]:
            op = inst["operation"]
            if inst["type"] is not None:
                width(inst["type"])
            if op["kind"] == "opcode":
                row = definitions[op["name"]]
                retained_message = abi_fork and op["name"] in ABI_MESSAGE_REQUESTS
                retained_gas = abi_fork and op["name"] == "gas"
                if (
                    row["observes_code"]
                    and not (abi_fork and op["name"] == "extcodesize")
                    or row["observes_gas"]
                    and row["gas"] != "SStore"
                    and not (retained_gas or retained_message)
                    or row["sets_returndata"]
                    and not retained_message
                ):
                    raise Unsupported(
                        "effect trees exclude code/gas observers and calls"
                    )
                if abi_fork and not row["forks"][abi_fork]["available"]:
                    raise Unsupported(
                        f"unavailable ABI effect-tree opcode: {op['name']}"
                    )
                if row["pure"]:
                    pure(op, [("0#256", 256)] * len(op["args"]), 256, definitions)
            elif op["kind"] in CHECK_REQUESTS and inline_effects:
                if (inst["result"] is not None) != (op["kind"] == "checked") or any(
                    width(v["type"]) > 256 for v in op["args"]
                ):
                    raise Unsupported("invalid conditional-failure effect")
            elif op["kind"] == "returndata_bytes" and inline_effects:
                if (
                    op["args"]
                    or inst["result"] is None
                    or inst["type"].get("location") != "memory"
                ):
                    raise Unsupported("invalid returndata object request")
            elif op["kind"] == "extcall" and inline_effects:
                if (
                    inst["result"] is None
                    or width(inst["type"]) > 256
                    or op["name"] not in ABI_MESSAGE_REQUESTS
                    or not definitions[op["name"]]["forks"][abi_fork]["available"]
                ):
                    raise Unsupported("unsupported typed message boundary")
            elif op["kind"] == "insert_value" and inline_effects:
                ty = op["aggregate_type"]
                if (
                    not word_pair(ty)
                    or inst["type"] != ty
                    or len(op["args"]) != 2
                    or op["args"][0]["type"] != ty
                    or op["index"] not in (0, 1)
                    or op["args"][1]["type"] != ty["fields"][op["index"]]
                ):
                    raise Unsupported("unsupported effect-tree product")
            elif op["kind"] == "mapping_slot" and abi_fork:
                if (
                    len(op["args"]) != 2
                    or inst["result"] is None
                    or width(inst["type"]) != 256
                ):
                    raise Unsupported("effect-tree mapping-slot signature mismatch")
            elif op["kind"] == "abi_decode" and abi_fork:
                if (
                    op["calldata"]
                    or op["rule"] != {"bits": 0, "function": False, "kind": "boolean"}
                    or len(op["args"]) != 1
                    or op["args"][0]["type"].get("location") != "memory"
                    or inst["result"] is None
                    or width(inst["type"]) != 1
                ):
                    raise Unsupported("effect-tree decoder requires a memory boolean")
            elif op["kind"] == "abi_encode_tree" and abi_fork:
                if (
                    op["mode"] != 2
                    or any(op["memory_slices"])
                    or len(op["args"])
                    != len(op["shapes"]) + int(op["selector"]) + int(op["base"])
                    or inst["result"] is None
                    or (inst["type"] or {}).get("kind") not in {"pointer", "slice"}
                    or inst["type"].get("location") != "memory"
                ):
                    raise Unsupported("shared ABI scratch encoder has invalid carriers")
            elif op["kind"] == "abi_encode" and abi_fork:
                if (
                    op["mode"] not in (1, 2)
                    or op["mode"] == 1
                    and op["base"]
                    or any(op["functions"])
                    or len(op["args"])
                    != len(op["functions"]) + int(op["selector"]) + int(op["base"])
                    or inst["result"] is None
                    or (inst["type"] or {}).get("location") != "memory"
                ):
                    raise Unsupported(
                        "effect-tree encoder requires a static word object"
                    )
            elif op["kind"] in {"make_slice", "extract_value"} and abi_fork:
                types = [value["type"] for value in op["args"]]
                result = inst["type"]
                if op["kind"] == "make_slice":
                    if (
                        result.get("kind") != "slice"
                        or len(types) != 2
                        or types[0].get("kind") != "pointer"
                        or types[0].get("location") != result.get("location")
                        or types[1].get("kind") != "word"
                        or types[1].get("bits") != 256
                    ):
                        raise Unsupported("invalid contextual slice")
                elif (
                    len(types) != 1
                    or types[0].get("kind") != "slice"
                    or op.get("aggregate_type") != types[0]
                    or op["index"] not in (0, 1)
                    or width(result) != 256
                    or (
                        op["index"] == 0
                        and (
                            result.get("kind") != "pointer"
                            or result.get("location") != types[0].get("location")
                        )
                    )
                    or (op["index"] == 1 and result.get("kind") != "word")
                ):
                    raise Unsupported("invalid contextual projection")
            elif op["kind"] == "call":
                callee = functions.get(op["function"])
                if callee is None or len(op["args"]) != len(callee["params"]):
                    raise Unsupported("effect-tree call argument mismatch")
                if op["function"] in call_cuts and any(
                    width(ty) > 256 for ty in callee["params"] + callee["returns"]
                ):
                    raise Unsupported("effect-tree call cut requires scalar carriers")
                if any(
                    mir_state.value_type(value["type"]) != mir_state.value_type(ty)
                    for value, ty in zip(op["args"], callee["params"], strict=True)
                ):
                    raise Unsupported("effect-tree call argument type mismatch")
                if inst["result"] is not None and (
                    len(callee["returns"]) != 1
                    or mir_state.value_type(inst["type"])
                    != mir_state.value_type(callee["returns"][0])
                ):
                    raise Unsupported("effect-tree call result type mismatch")
            elif op["kind"] == "allocate":
                if (
                    len(op["args"]) != 1
                    or inst["result"] is None
                    or inst["type"].get("kind") != "pointer"
                ):
                    raise Unsupported("effect-tree allocation signature mismatch")
            elif op["kind"] == "free_pointer":
                ty = inst["type"] or {}
                if (
                    op["args"]
                    or inst["result"] is None
                    or ty.get("kind") != "pointer"
                    or ty.get("location") != "memory"
                ):
                    raise Unsupported("effect-tree FMP signature mismatch")
            elif op["kind"] == "layout":
                if return_widths is not None and op["query"] == "heap_floor":
                    if (
                        op["args"]
                        or inst["result"] is None
                        or width(inst["type"]) != 256
                    ):
                        raise Unsupported(
                            "integer lowering needs a full-width heap floor"
                        )
                    continue
                if abi_fork and op["query"] == "immutable_read":
                    if (
                        op["args"]
                        or op["signed"]
                        or op["width"] != 256
                        or inst["result"] is None
                        or inst["type"].get("kind") != "word"
                        or width(inst["type"]) != 256
                    ):
                        raise Unsupported(
                            "effect trees require full-word immutable reads"
                        )
                    continue
                if abi_fork and op["query"] == "immutable_store":
                    if inst["result"] is not None or len(op["args"]) != 1:
                        raise Unsupported(
                            "effect-tree immutable store signature mismatch"
                        )
                    for value in op["args"]:
                        if width(value["type"]) != 256 or value["kind"] == "undefined":
                            raise Unsupported("invalid immutable store operand")
                    continue
                ty = inst["type"] or {}
                if (
                    set(op) != {"kind", "query", "end", "args"}
                    or op["query"] != "constructor"
                    or type(op["end"]) is not bool
                    or op["args"]
                    or inst["result"] is None
                    or ty.get("kind") != "word"
                    or ty.get("bits") != 256
                ):
                    raise Unsupported(
                        "effect trees require an explicit constructor-range query"
                    )
            elif op["kind"] not in {
                "word",
                "low_bits",
                "sign_extend",
                "ne",
                "select",
                "pointer_add",
            }:
                raise Unsupported(f"unmodeled effect-tree operation `{op['kind']}`")
            for value in op["args"]:
                width(value["type"])
                if value["kind"] == "undefined" and not (
                    inline_effects
                    and op["kind"] == "insert_value"
                    and value is op["args"][0]
                ):
                    raise Unsupported("undefined effect-tree operand")
        for value in block["terminator"].get("args", []):
            width(value["type"])
            if value["kind"] == "undefined":
                raise Unsupported("undefined effect-tree operand")

    # Check every operation, including unreachable blocks and unused values.
    # Unsupported semantics cannot disappear through lazy Lean let reduction.
    for function in functions.values():
        for ty in [*function["params"], *function["returns"]]:
            width(ty)
        if function["id"] not in call_cuts:
            if inline_effects:
                check_pair_initialization(function)
            for block in function["blocks"]:
                validate_block(block)
    entry = functions[model["entry_id"]]
    arguments = [(f"arg{index}", width(ty)) for index, ty in enumerate(entry["params"])]
    if abi_arguments is not None:
        arguments = [
            (word, bits)
            for word, (_, bits) in zip(abi_arguments, arguments, strict=True)
        ]

    def returned(args):
        if not abi_return:
            if return_widths is not None:
                args = [
                    (value, bits)
                    for (value, _), bits in zip(args, return_widths, strict=True)
                ]
            return boundary("return", args)
        if bytes_return:
            return bytes_boundary(args)
        words = ",".join(f"({value}).setWidth 256" for value, _ in args)
        return f"MIR.Effects.abiReturn [{words}]"

    result = enter(
        entry,
        0,
        None,
        {},
        arguments,
        (entry["id"],),
        (),
        returned,
    )
    if len(result) > 262144:
        raise Unsupported("effect-tree text exceeds 256 KiB")
    return result


def check(a, b, timeout, executable, *, void_abi=False):
    from .check import PRELUDE, ROOT, build_model, run

    definitions = {
        row["name"]: row for row in native.catalog(executable, timeout)["evmir"]
    }
    if void_abi and any(
        program.symbolic_mir["model"][signature]
        for program in (a, b)
        for signature in ("params", "returns")
    ):
        raise Unsupported("void ABI effect proof requires empty signatures")
    trees = [
        render(a.symbolic_mir["model"], definitions, abi_return=void_abi),
        render(b.symbolic_mir["model"], definitions),
    ]
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
    rules = """before, after, MIR.compare, MIR.eval, MIR.cast, MIR.shift, Word.shift,
    Word.binary, MIR.unary, Word.unary, MIR.select, MIR.wordBinary, MIR.ternary,
    MIR.LowerAlloc.freePointerRequest, MIR.LowerAlloc.constructorRequest,
    BitVec.ofNatLT_eq_ofNat, BitVec.ult, MIR.Effects.abiReturn"""
    masks = low_masks(a.symbolic_mir["model"]) - low_masks(b.symbolic_mir["model"])
    tactic = f"simp [{rules}]"
    if masks:
        lemmas = []
        for width, bits in sorted(masks):
            right = f"lowMask{width}_{bits}"
            left = f"{right}Left"
            mask = f"{(1 << bits) - 1}#{width}"
            binder = f"(word : BitVec {width}) (small : word.toNat < {1 << bits})"
            code += f"""
private theorem {right} {binder} : word &&& {mask} = word :=
  MIR.Effects.mask_of_bound {width} {bits} word (by decide) small
private theorem {left} {binder} : {mask} &&& word = word := by
  rw [BitVec.and_comm]
  exact {right} word small
"""
            lemmas.extend((right, left))
        tactic = f"""first
  | (solve | {tactic})
  | simp (config := {{contextual := true}}) (disch := omega) [{rules}, {", ".join(lemmas)}]"""
    code += f"""
theorem pair_correct {binders} : before {args} = after {args} := by
  {tactic}
#print axioms pair_correct
"""
    with tempfile.TemporaryDirectory(prefix="solar-tv-mir-effects-") as directory:
        path = Path(directory) / "Pair.lean"
        path.write_text(code)
        result = run(["lake", "env", "lean", "-j1", path], ROOT, timeout)
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
                "kind": "independent-call-effect-tree"
                if len(program.symbolic_mir["model"]["functions"]) > 1
                else "independent-scalar-effect-tree",
                "supplied_functions": len(program.symbolic_mir["model"]["functions"]),
                "allocation_sites": sum(
                    inst["operation"]["kind"] == "allocate"
                    for function in program.symbolic_mir["model"]["functions"]
                    for block in function["blocks"]
                    for inst in block["instructions"]
                ),
                "normalized_fmp_reads": sum(
                    inst["operation"]["kind"] == "free_pointer"
                    for function in program.symbolic_mir["model"]["functions"]
                    for block in function["blocks"]
                    for inst in block["instructions"]
                ),
                "constructor_range_queries": sum(
                    inst["operation"]["kind"] == "layout"
                    and inst["operation"]["query"] == "constructor"
                    for function in program.symbolic_mir["model"]["functions"]
                    for block in function["blocks"]
                    for inst in block["instructions"]
                ),
                "effect_tree_sha256": hashlib.sha256(tree.encode()).hexdigest(),
                "boundary": "trusted compiler parser/schema export and scalar effect-tree renderer; contextual theorem covers arbitrary stateful request handlers, not full-state interpreter correspondence",
            },
        )
        for program, tree in zip((a, b), trees, strict=True)
    )
    return audit.Result("verified", stdout="verified\n", program=pair[0], pair=pair)
