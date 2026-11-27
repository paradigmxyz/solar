"""Check MIR ABI adapters and their continuations with contextual certificates."""

import hashlib
import re
import tempfile
from copy import deepcopy
from pathlib import Path

from . import (
    abi_loop,
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


def selected_body(module, name, *, tail_calls=False):
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
            term = block["terminator"]
            if tail_calls and term["kind"] == "tail_call":
                pending.append(term["function"])
            pending.extend(
                inst["operation"]["function"]
                for inst in block["instructions"]
                if inst["operation"]["kind"] == "call"
            )
    return {"entry_id": entry["id"], "functions": list(reachable.values())}, entry


def static_word_count(shape):
    if shape["kind"] == "word":
        return 1
    if shape["kind"] == "tuple":
        return sum(static_word_count(field) for field in shape["fields"])
    raise Unsupported("shared ABI scratch encoder needs a static tuple layout")


def slice_projections(model):
    """Resolve local slice projections with known typed components."""
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
                if (
                    op["kind"] in {"abi_encode", "abi_encode_tree"}
                    and op["mode"] == 2
                    and not any(op.get("functions", []))
                    and inst["type"].get("kind") == "slice"
                    and inst["type"].get("location") == "memory"
                ):
                    pointer = {
                        "bits": 256,
                        "kind": "pointer",
                        "location": "memory",
                        "name": "ptr<memory>",
                    }
                    length = {"bits": 256, "kind": "word", "name": "i256"}
                    if op["kind"] == "abi_encode_tree":
                        if any(
                            shape == {"kind": "bytes", "location": 1}
                            for shape in op["shapes"]
                        ):
                            instructions.append(inst)
                            continue
                        count = sum(static_word_count(shape) for shape in op["shapes"])
                    else:
                        count = len(op["functions"])
                    size = 32 * count + 4 * int(op["selector"])
                    slices[inst["result"]] = [
                        {"kind": "local", "index": inst["result"], "type": pointer},
                        {"kind": "constant", "value": str(size), "type": length},
                    ]
                    inst = {**inst, "type": pointer}
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


def product_projections(model, *, flatten_return=False):
    """Expose defined local product fields at the static ABI boundary."""
    model = deepcopy(model)
    for body in model["functions"]:
        flatten = flatten_return and body["id"] == model["entry_id"]
        if flatten:
            body["returns"] = body["returns"][0]["fields"]
        for block in body["blocks"]:
            products, instructions = {}, []
            for inst in block["instructions"]:
                op = inst["operation"]
                layout = op.get("aggregate_type", {})
                if layout.get("kind") == "product" and op["kind"] in {
                    "insert_value",
                    "extract_value",
                }:
                    source = op["args"][0]
                    index, fields = op["index"], layout["fields"]
                    if source["type"] != layout or not 0 <= index < len(fields):
                        raise Unsupported("invalid ABI product projection")
                    values = (
                        products.get(source.get("index"))
                        if source["kind"] == "local"
                        else None
                    )
                    if op["kind"] == "insert_value":
                        if source["kind"] == "undefined":
                            values = [None] * len(fields)
                        value = op["args"][1]
                        if (
                            values is None
                            or inst["type"] != layout
                            or value["kind"] == "undefined"
                            or value["type"] != fields[index]
                        ):
                            raise Unsupported("ABI product requires local typed fields")
                        values = list(values)
                        values[index] = value
                        products[inst["result"]] = values
                        continue
                    if (
                        values is None
                        or values[index] is None
                        or inst["type"] != fields[index]
                    ):
                        raise Unsupported("ABI projection reads an undefined field")
                    inst = {
                        **inst,
                        "operation": {"kind": "word", "args": [values[index]]},
                    }
                instructions.append(inst)
            block["instructions"] = instructions
            if flatten and block["terminator"]["kind"] == "return":
                (product,) = block["terminator"]["args"]
                values = (
                    products.get(product.get("index"))
                    if product["kind"] == "local"
                    else None
                )
                if (
                    values is None
                    or any(value is None for value in values)
                    or [value["type"] for value in values] != body["returns"]
                ):
                    raise Unsupported("ABI product return has uninitialized fields")
                block["terminator"]["args"] = values
    return model


def runtime_parameters(header, types):
    matched = re.search(r"\babi_params=\[([^\]]*)\]", header)
    if not matched:
        raise Unsupported("implicit ABI layout is missing")
    plans = []
    offset = 4
    head = re.findall(r"tuple<[^<>]+>|[^, ]+", matched[1])
    if ", ".join(head) != matched[1] or len(head) != len(types) or len(head) > 8:
        raise Unsupported("unsupported contextual ABI parameter layout")
    for spelling, ty in zip(head, types, strict=True):
        if spelling.startswith("tuple<") and (ty.get("kind"), ty.get("location")) in {
            ("pointer", "memory"),
            ("slice", "calldata"),
        }:
            widths = tuple(map(lower_abi.width, spelling[6:-1].split(", ")))
            plans.append((spelling, offset, widths))
            offset += 32 * len(widths)
        elif (
            spelling == "bytes"
            and ty.get("kind") == "slice"
            and ty.get("location") == "calldata"
        ):
            plans.append((spelling, offset, None))
            offset += 32
        else:
            bits = lower_abi.width(spelling)
            if ty.get("kind") != "word" or ty["bits"] < bits:
                raise Unsupported("contextual ABI scalar carrier mismatch")
            plans.append((spelling, offset, bits))
            offset += 32
    return plans, offset


def guard_source(params, end, local_guard):
    """Compare the explicit decoder with calls into the typed source body."""
    words = [
        f"(Execution.readWord environment.input {offset})"
        if isinstance(bits, int)
        else f"aggregate{index}"
        for index, (_, offset, bits) in enumerate(params)
    ]
    cases = []
    if local_guard:
        cases.append(("nonzero", "environment.value ≠ 0#256"))
    if params:
        short = f"environment.input.length < {end}"
        cases.append(("short", short))
    canonical = []
    for i, (word, (_, _, bits)) in enumerate(zip(words, params, strict=True)):
        if not isinstance(bits, int) or bits == 256:
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


def retained_callee_code(tables, definitions):
    code = ""
    for name, table in zip(("leftCallees", "rightCallees"), tables, strict=True):
        entries = ",".join(mir_state.function_body(f, definitions) for f in table)
        code += f"def {name} : MIR.State.Functions := [{entries}]\n"
    return (
        code
        + """
theorem sameCallees : leftCallees = rightCallees := rfl
#print axioms sameCallees
"""
    )


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
            slice_projections(model), definitions, abi_fork=version, call_cuts=cuts
        )
        for model in (old, new)
    ]
    params = [
        (f"arg{i}", mir_effects.scalar_width(ty)) for i, ty in enumerate(body["params"])
    ]
    binders = " ".join(f"({name} : BitVec {bits})" for name, bits in params)
    args = " ".join(name for name, _ in params)
    callee_proof = retained_callee_code(tables, definitions) if cuts else ""
    code = (
        prelude
        + callee_proof
        + f"""
set_option maxRecDepth 16384
set_option maxHeartbeats 4000000
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
    MIR.wordBinary, MIR.unary, Word.unary, Word.binary, Nat.mod_mod,
    MIR.Effects.highHalfWord, MIR.Effects.highHalfNat, MIR.Effects.highHalfZero]
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


def contextual_callees(old, new, definitions, version):
    """Check rewritten internal helpers before treating their calls as requests."""
    targets = {body["id"]: body for body in new["functions"]}
    cuts, certificates, names = [], [], []
    for source in old["functions"]:
        index, name = source["id"], source["return_buffer"]
        target = targets.get(index)
        if (
            index == old["entry_id"]
            or index == new["entry_id"]
            or target is None
            or target == source
            or target["return_buffer"] != name
            or not any(
                block["terminator"]["kind"] == "revert_return_data"
                or any(
                    inst["operation"]["kind"] == "abi_decode"
                    for inst in block["instructions"]
                )
                for block in source["blocks"]
            )
        ):
            continue
        a, _ = selected_body(old, name)
        b, _ = selected_body(new, name)
        try:
            code, _ = internal_code(a, b, source, target, definitions, version, "")
        except Unsupported:
            continue
        cuts.append(index)
        names.append(name)
        certificates.append(f"namespace Callee{index}\n{code}\nend Callee{index}\n")
    return cuts, "".join(certificates), names


def certificate(
    code, texts, function, version, timeout, evidence, *, scope="abi-effect-tree"
):
    from .check import build_model

    entry_proof.success(build_model(timeout), "ABI effect proof model")
    with tempfile.TemporaryDirectory(prefix="solar-tv-abi-effects-") as directory:
        output = entry_proof.success(
            entry_proof.lean(Path(directory), "Abi", code, timeout),
            "ABI contextual effect certificate",
        )
    checks = sum(line.startswith("#print axioms ") for line in code.splitlines())
    entry_proof.check_axioms(output.splitlines(), checks)
    evidence = {**evidence, "proof_sha256": hashlib.sha256(code.encode()).hexdigest()}
    pair = tuple(
        Program(
            (scope, function),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            scope=scope,
            selector=function,
            evm_version=version,
            import_evidence=evidence,
        )
        for text in texts
    )
    return audit.Result("verified", stdout="verified\n", program=pair[0], pair=pair)


def constructor_code(
    old, new, body, wrapper, declarations, function, definitions, version, prelude
):
    (old_header, old_functions), (new_header, new_functions) = declarations
    headers = [funcs[function]["text"][0] for funcs in (old_functions, new_functions)]
    payable = r"(?:\[|, )payable(?:\]|, )"
    accepts_value = bool(re.search(payable, headers[0]))
    if (
        old_header != new_header
        or wrapper["params"]
        or wrapper["returns"]
        or body["returns"]
        or "abi_wrapper" in headers[0]
        or "abi_wrapper" not in headers[1]
        or accepts_value != bool(re.search(payable, headers[1]))
    ):
        raise Unsupported(
            "constructor effect proof needs unchanged bindings and an ABI wrapper"
        )
    params = (
        lower_abi.layout(headers[0], "abi_params", body["params"], arguments=True)
        if body["params"] or "abi_params=" in headers[0]
        else []
    )
    if len(params) > 1:
        raise Unsupported("constructor effect proof supports one staged word")
    cuts, tables = unchanged_callees(old, new)
    inputs = [f"(rawArg.setWidth {body['params'][0]['bits']})"] if params else []
    left, right = [
        mir_effects.render(
            slice_projections(model),
            definitions,
            abi_fork=version,
            call_cuts=cuts,
            abi_arguments=arguments,
        )
        for model, arguments in ((old, inputs), (new, []))
    ]
    guard = (
        ""
        if accepts_value
        else "if environment.value ≠ 0#256 then MIR.Effects.abiDecodeFailure else\n"
    )
    if params:
        bits = lower_abi.width(params[0])
        canonical = (
            f"if {(1 << bits) - 1} < rawArg.toNat then MIR.Effects.abiDecodeFailure else"
            if bits < 256
            else ""
        )
        left = f"""
  MIR.Effects.Tree.request (MIR.LowerAlloc.constructorRequest false) (fun base =>
    MIR.Effects.Tree.request (MIR.LowerAlloc.constructorRequest true) (fun last =>
      if last.toNat < (base + 32).toNat then MIR.Effects.abiDecodeFailure else
        MIR.Effects.Tree.request ⟨.opcode 0x51, [base], true⟩ (fun rawArg =>
          {canonical}
          (
{left}
          ))))
"""
    return (
        prelude
        + (retained_callee_code(tables, definitions) if cuts else "")
        + f"""
set_option maxRecDepth 16384
set_option maxHeartbeats 4000000
def before (environment : World.Environment) : MIR.Effects.Tree :=
{guard}
{left}
def after (environment : World.Environment) : MIR.Effects.Tree :=
{right}
theorem wrapped (environment : World.Environment) : before environment = after environment := by
  simp [before, after, MIR.Effects.abiDecodeFailure, MIR.eval, MIR.compare,
    BitVec.ult_iff_toNat_lt, MIR.LowerAlloc.constructorRequest]
theorem contextual {{State Halt : Type}} (environment : World.Environment)
    (handler : MIR.Effects.Request → State → Except Halt (Word × State))
    (finish : MIR.Effects.Boundary → State → Halt) (state : State) :
    MIR.Effects.interpret handler finish (before environment) state =
      MIR.Effects.interpret handler finish (after environment) state :=
  MIR.Effects.contextual _ _ (wrapped environment) handler finish state
#print axioms wrapped
#print axioms contextual
"""
    ), {"kind": "kernel-checked-abi-constructor-effects", "retained_callees": cuts}


def scratch_returns(model, shape, kind):
    model = deepcopy(model)
    entry = next(f for f in model["functions"] if f["id"] == model["entry_id"])
    count = 0
    for block in entry["blocks"]:
        if block["terminator"]["kind"] != "returndata":
            continue
        instructions = block["instructions"]
        if len(instructions) < 3:
            raise Unsupported(
                "ABI return requires a scratch encoding and two projections"
            )
        encode, offset, size = instructions[-3:]
        op = encode["operation"]
        arguments = op["args"]
        ref = {"kind": "local", "index": encode["result"], "type": encode["type"]}
        projection = {
            "kind": "extract_value",
            "aggregate_type": encode["type"],
            "args": [ref],
        }
        if (
            encode["result"] is None
            or op.get("kind") != "abi_encode_tree"
            or op.get("mode") != 2
            or op.get("base")
            or op.get("selector")
            or op.get("shapes") != [shape]
            or op.get("memory_slices") != [False]
            or len(arguments) != 1
            or arguments[0]["type"].get("location") != "memory"
            or encode["type"].get("kind") != "slice"
            or encode["type"].get("location") != "memory"
            or offset["operation"] != {**projection, "index": 0}
            or size["operation"] != {**projection, "index": 1}
            or offset["type"].get("kind") != "pointer"
            or offset["type"].get("location") != "memory"
            or size["type"].get("kind") != "word"
            or size["type"].get("bits") != 256
            or block["terminator"]["args"]
            != [
                {"kind": "local", "type": inst["type"], "index": inst["result"]}
                for inst in (offset, size)
            ]
        ):
            raise Unsupported("ABI return requires ordered memory-scratch projections")
        del instructions[-3:]
        block["terminator"] = {"kind": kind, "args": arguments}
        count += 1
    if not count:
        raise Unsupported("missing explicit scratch return boundary")
    return model


def typed_pair(paths, solar, replay, version, timeout):
    from .check import ROOT, run

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
    declarations = [prune.declarations(text) for text in texts]
    if declarations[0][0] != declarations[1][0]:
        raise Unsupported("ABI effect proof requires unchanged module declarations")
    modules = [type_prune.export(text, replay, timeout) for text in texts]
    return texts, declarations, modules


def check(paths, solar, replay, version, timeout, function):
    from .check import PRELUDE

    texts, declarations, modules = typed_pair(paths, solar, replay, version, timeout)
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
    if all(
        model["entry_id"] == module["constructor"]
        for module, model in zip(modules, (old, new), strict=True)
    ):
        code, evidence = constructor_code(
            old,
            new,
            body,
            wrapper,
            declarations,
            function,
            definitions,
            version,
            PRELUDE,
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
    header = declarations[0][1][function]["text"][0]
    if abi_loop.array_entry(header, body):
        returned = scratch_returns(new, abi_loop.WORD_ARRAY, "return")
        code, evidence = abi_loop.code(
            body, returned, definitions, version, PRELUDE, local_guard
        )
        return certificate(code, texts, function, version, timeout, evidence)
    params, head_end = runtime_parameters(header, body["params"])
    bytes_return = (
        "abi_returns=[memory_bytes]" in header
        and "abi_return_params=[bytes]" in header
        and len(body["returns"]) == 1
        and body["returns"][0].get("kind") == "pointer"
        and body["returns"][0].get("location") == "memory"
    )
    if bytes_return:
        returns = ["bytes"]
        new = scratch_returns(new, {"kind": "bytes", "location": 0}, "abi_return_bytes")
        cuts, tables = unchanged_callees(old, new)
    else:
        leaves = [leaf for ty in body["returns"] for leaf in mir_state.leaf_types(ty)]
        returns = lower_abi.layout(header, "abi_return_params", leaves, arguments=False)
        cuts, tables = (), ()
    if len(body["returns"]) == 1 and body["returns"][0].get("kind") == "product":
        old = product_projections(old, flatten_return=True)
        new = product_projections(new)
    retained_code = retained_callee_code(tables, definitions) if cuts else ""
    callee_code, composed = "", []
    if any(not isinstance(layout, int) for _, _, layout in params):
        if not cuts:
            unchanged, tables = unchanged_callees(old, new)
            try:
                retained_code = (
                    retained_callee_code(tables, definitions) if unchanged else ""
                )
                cuts = unchanged
            except Unsupported:
                pass
        rewritten, callee_code, composed = contextual_callees(
            old, new, definitions, version
        )
    else:
        rewritten = []
    retained = cuts
    cuts = [*retained, *rewritten]
    encoder = " (bytesReturn : Word → MIR.Effects.Tree)" if bytes_return else ""
    encode_arg = " bytesReturn" if bytes_return else ""
    words, rejected, splits = guard_source(params, head_end, local_guard)
    inputs = [
        f"({word}.setWidth {ty['bits']})" if ty["kind"] != "slice" else word
        for word, ty in zip(words, body["params"], strict=True)
    ]
    left_tree = mir_effects.render(
        slice_projections(old),
        definitions,
        abi_return=True,
        abi_arguments=inputs,
        abi_fork=version,
        call_cuts=cuts,
        bytes_return=bytes_return,
    )
    for index, (_, source, fields) in reversed(list(enumerate(params))):
        if fields is None:
            relative = f"(Execution.readWord environment.input {source})"
            start = f"({relative} + 4#256)"
            data = f"({start} + 32#256)"
            length = f"(Execution.readWord environment.input {start}.toNat)"
            total = "(BitVec.ofNat 256 environment.input.length)"
            left_tree = f"""if ({total} - 4#256 - 32#256).toNat < {relative}.toNat ∨
    ({total} - {data}).toNat < {length}.toNat then MIR.Effects.abiDecodeFailure else
  let aggregate{index} := {data} ++ {length}
  (
{left_tree}
  )
"""
        elif not isinstance(fields, int):
            if body["params"][index]["kind"] == "slice":
                left_tree = f"let aggregate{index} := {source}#256 ++ {32 * len(fields)}#256\n({left_tree})"
            else:
                left_tree = f"""
MIR.Effects.decodeCalldataTuple environment.input {source} {list(fields)} (fun aggregate{index} =>
{left_tree}
)
"""
    right_tree = mir_effects.render(
        slice_projections(new),
        definitions,
        abi_fork=version,
        call_cuts=cuts,
        bytes_return=bytes_return,
    )
    code = (
        PRELUDE
        + retained_code
        + callee_code
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
def before (environment : World.Environment){encoder} : MIR.Effects.Tree :=
  if {rejected} then .done ⟨"revert",[(256,0),(256,0)]⟩ else
  (
{left_tree}
  )
def after (environment : World.Environment){encoder} : MIR.Effects.Tree :=
{right_tree}
macro "normalize_abi_tree" bound:term : tactic => `(tactic|
  simp [*, before, after, MIR.compare, MIR.eval, MIR.cast, MIR.shift, Word.shift,
    Word.binary, MIR.unary, Word.unary, MIR.select, MIR.wordBinary, MIR.ternary,
    BitVec.ofNatLT_eq_ofNat, BitVec.ult_iff_toNat_lt, BitVec.toNat_ofNat,
    Nat.mod_eq_of_lt $bound, MIR.Effects.abiReturn, MIR.Effects.writeAbiWords,
    Nat.mod_mod, MIR.Effects.decodeCalldataTuple, MIR.Effects.stageCalldataFields,
    MIR.Effects.abiDecodeFailure, MIR.Effects.abiRevertData, MIR.Effects.decodeMemoryBool,
    MIR.Effects.signedBoolHead, Decidable.imp_iff_not_or, BitVec.add_assoc, BitVec.extractLsb'_append_eq_left,
    BitVec.extractLsb'_append_eq_right, BitVec.setWidth_append_eq_right,
    MIR.Effects.bitOrTruth, MIR.Effects.highHalfWord, MIR.Effects.highHalfNat,
    MIR.Effects.highHalfZero, MIR.Effects.highHalfInputEq])
theorem wrapped (environment : World.Environment){encoder} (bounded : environment.input.length < 2^256) :
    before environment{encode_arg} = after environment{encode_arg} := by
{splits}
  normalize_abi_tree bounded
theorem contextual {{State Halt : Type}} (environment : World.Environment){encoder}
    (bounded : environment.input.length < 2^256)
    (handler : MIR.Effects.Request → State → Except Halt (Word × State))
    (finish : MIR.Effects.Boundary → State → Halt) (state : State) :
    MIR.Effects.interpret handler finish (before environment{encode_arg}) state =
      MIR.Effects.interpret handler finish (after environment{encode_arg}) state :=
  MIR.Effects.contextual _ _ (wrapped environment{encode_arg} bounded) handler finish state
#print axioms wrapped
#print axioms contextual
"""
    )
    evidence = {
        "kind": "kernel-checked-abi-bytes-effects"
        if bytes_return
        else "kernel-checked-abi-contextual-effects",
        "retained_callees": retained,
        "composed_callees": composed,
        "selector": left["selector"],
        "abi_params": [name for name, _, _ in params],
        "abi_returns": returns,
        "rejects_value_in_wrapper": local_guard,
    }
    return certificate(code, texts, function, version, timeout, evidence)
