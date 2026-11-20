"""Bind builtin-lowering theorems to independently imported typed SSA.

Only exact instances of the proved expansions are contracted. SSA definitions,
branch targets, incoming phi edges and reachable callees outside the expansion
must agree. In particular, contraction cannot hide a second entry to a lowered
block or a use of an expansion's private temporaries.
"""

import hashlib
import re
import tempfile
from pathlib import Path

from . import audit, call_tail, callers, dead_ssa, mir_state, native, prune, type_prune
from .common import Program, Unsupported


def require(condition, reason):
    if not condition:
        raise Unsupported(reason)


def word(bits=256):
    return {"bits": bits, "kind": "word", "name": f"i{bits}"}


def pointer(location="memory"):
    return {
        "bits": 256,
        "kind": "pointer",
        "location": location,
        "name": f"ptr<{location}>",
    }


def slice_type(location):
    return {"kind": "slice", "location": location, "name": f"slice<{location}>"}


def constant(value, ty=None):
    return {
        "kind": "constant",
        "type": word() if ty is None else ty,
        "value": str(value),
    }


def local(inst):
    require(inst["result"] is not None, "builtin expansion is missing a result")
    return {"kind": "local", "type": inst["type"], "index": inst["result"]}


def operation(kind, args=(), **fields):
    return {"kind": kind, "args": list(args), **fields}


def opcode(name, *args):
    return operation("opcode", args, name=name)


def instruction(op, result=None, ty=None):
    return {"operation": op, "result": result, "type": ty}


def expect(inst, op, ty=None):
    require(
        inst["operation"] == op
        and inst["type"] == ty
        and (inst["result"] is None) == (ty is None),
        "typed builtin expansion instruction does not match",
    )
    if ty is not None:
        return local(inst)


def targets(term):
    if term["kind"] == "jump":
        return [term["target"]]
    if term["kind"] == "branch":
        return [term["then"], term["else"]]
    if term["kind"] == "switch":
        return [term["default"], *(target for _, target in term["cases"])]
    return []


def branch(term, condition):
    require(
        term["kind"] == "branch" and term["args"] == [condition],
        "builtin expansion branch condition does not match",
    )
    return term["then"], term["else"]


def panic(block, code):
    require(
        block
        == {
            "phis": [],
            "instructions": [
                instruction(opcode("mstore", constant(32), constant(code))),
                instruction(opcode("mstore", constant(0), constant(0x4E487B71))),
            ],
            "terminator": operation("revert", [constant(28), constant(36)]),
        },
        "builtin expansion panic block does not match",
    )


def normalize(body):
    """Assign block IDs in edge order and local IDs in definition order."""
    blocks = body["blocks"]
    order, labels = [0], {0: 0}
    for index in order:
        for target in targets(blocks[index]["terminator"]):
            if target not in labels:
                labels[target] = len(order)
                order.append(target)
    require(len(order) == len(blocks), "unreachable or extra builtin expansion block")
    locals_ = {}
    for index in order:
        block = blocks[index]
        for definition in (*block["phis"], *block["instructions"]):
            result = definition["result"]
            if result is not None:
                require(result not in locals_, "duplicate builtin expansion definition")
                locals_[result] = len(locals_)

    def rename(value):
        if isinstance(value, list):
            return [rename(item) for item in value]
        if not isinstance(value, dict):
            return value
        result = {key: rename(item) for key, item in value.items()}
        if result.get("kind") == "local":
            index = result["index"]
            require(index in locals_, "builtin expansion temporary escapes")
            result["index"] = locals_[index]
        if result.get("result") is not None:
            result["result"] = locals_[result["result"]]
        if "incoming" in result:
            result["incoming"] = sorted(
                (
                    [labels[predecessor], operand]
                    for predecessor, operand in result["incoming"]
                ),
                key=lambda arm: arm[0],
            )
        for key in ("target", "then", "else", "default"):
            if key in result:
                result[key] = labels[result[key]]
        if "cases" in result:
            result["cases"] = [
                [operand, labels[target]] for operand, target in result["cases"]
            ]
        return result

    return {**body, "blocks": [rename(blocks[index]) for index in order]}


def body_blocks(body):
    return dict(enumerate(body["blocks"]))


def transfer_prefix(body, cut, address, obj, requested):
    """Select the two phi-free blocks leading from helper entry to the CALL."""
    blocks = body["blocks"]
    entry, head = blocks[0], blocks[cut]
    require(
        body["params"] == [word(160)] * 3 + [word()]
        and address == {"kind": "arg", "index": 0, "type": word(160)}
        and obj == {"kind": "local", "index": 6, "type": pointer()}
        and requested == {"kind": "local", "index": 3, "type": word()}
        and not entry["phis"]
        and not head["phis"],
        "transferFrom prefix differs from the proved typed interface",
    )
    success, failure = branch(
        entry["terminator"], {"kind": "local", "index": 2, "type": word(1)}
    )
    require(
        success == cut
        and failure != cut
        and blocks[failure]["terminator"]["kind"] == "invalid"
        and all(
            index == 0
            for index, block in enumerate(blocks)
            if cut in targets(block["terminator"])
        ),
        "transferFrom prefix has different control flow",
    )
    return entry["instructions"], head["instructions"][:-2]


def transfer_continuation(body, cut, call, collect, error):
    """Bind the semantic suffix to the direct MIR decoder and terminal actions."""
    blocks = body["blocks"]
    success, fail = branch(blocks[cut]["terminator"], local(call))
    head = blocks[success]
    require(
        not head["phis"] and len(head["instructions"]) == 2,
        "unexpected returndata test",
    )
    load, compare = head["instructions"]
    length = expect(load, opcode("mload", local(collect)), word())
    empty = expect(compare, opcode("eq", length, constant(0)), word(1))
    join, decode = branch(head["terminator"], empty)
    decoder = blocks[decode]
    require(
        not decoder["phis"]
        and len(decoder["instructions"]) == 1
        and decoder["terminator"] == {"kind": "jump", "target": join},
        "unexpected returndata decode path",
    )
    value = expect(
        decoder["instructions"][0],
        operation(
            "abi_decode",
            [local(collect)],
            calldata=False,
            rule={"bits": 0, "function": False, "kind": "boolean"},
        ),
        word(1),
    )
    result = blocks[join]
    require(
        not result["instructions"] and len(result["phis"]) == 1,
        "unexpected returndata result",
    )
    phi = result["phis"][0]
    require(
        phi["type"] == word(1)
        and sorted(phi["incoming"])
        == sorted([[success, constant(1, word(1))], [decode, value]]),
        "different returndata join",
    )
    ok, false = branch(result["terminator"], local(phi))
    require(
        blocks[ok]
        == {"phis": [], "instructions": [], "terminator": operation("return")},
        "transferFrom success path is not a return",
    )
    for index, reason in ((fail, call_tail.FAILED), (false, call_tail.FALSE)):
        require(
            blocks[index]
            == {
                "phis": [],
                "instructions": [
                    instruction(
                        operation(
                            "call",
                            [
                                constant(len(reason)),
                                constant(
                                    int.from_bytes(reason.ljust(32, b"\0"), "big")
                                ),
                            ],
                            function=error["id"],
                        )
                    )
                ],
                "terminator": {"kind": "invalid"},
            },
            "transferFrom error path differs from the checked continuation",
        )
    expected = [
        instruction(opcode("mstore", constant(32), constant(32))),
        instruction(
            opcode("mstore", constant(64), {"kind": "arg", "index": 0, "type": word()})
        ),
        instruction(
            opcode("mstore", constant(96), {"kind": "arg", "index": 1, "type": word()})
        ),
        instruction(opcode("mstore", constant(0), constant(0x08C379A0))),
    ]
    require(
        error["params"] == [word(), word()]
        and not error["returns"]
        and error["blocks"]
        == [
            {
                "phis": [],
                "instructions": expected,
                "terminator": operation("revert", [constant(28), constant(100)]),
            }
        ],
        "transferFrom revert_error implementation changed",
    )


def caller_continuations(bodies, declarations, sites):
    """Check both MIR caller tails, with the sibling return value live at the cut."""
    names = {site.name for site in sites}
    for table, decls in zip(bodies, declarations, strict=True):
        helper = table["safeTransferFrom"]["id"]
        called_by = {
            name
            for name, body in table.items()
            if any(
                row["operation"].get("function") == helper
                for block in body["blocks"]
                for row in block["instructions"]
            )
        }
        require(called_by == names, "MIR transferFrom caller set changed")
        for site in sites:
            require(
                f"[selector=0x{site.selector:08x}," in decls[site.name]["text"][0],
                f"MIR ABI selector changed in {site.name}",
            )
            body = table[site.name]
            calls = [
                (block, index, row)
                for block in body["blocks"]
                for index, row in enumerate(block["instructions"])
                if row["operation"].get("function") == helper
            ]
            require(len(calls) == 1, f"ambiguous MIR transferFrom call in {site.name}")
            block, index, call = calls[0]
            operands = call["operation"]["args"]
            expect(call, operation("call", operands, function=helper))
            require(
                [operand["type"] for operand in operands] == [word(160)] * 3 + [word()],
                f"transferFrom argument types changed in {site.name}",
            )
            later = block["instructions"][index + 1 :]
            if not site.live_words:
                require(
                    not body["returns"]
                    and not later
                    and block["terminator"] == operation("return"),
                    f"void MIR continuation changed in {site.name}",
                )
                continue
            ty = {"kind": "product", "fields": [word(), word()]}
            require(
                body["returns"] == [ty] and len(later) == 2,
                "unexpected MIR caller return product",
            )
            first, second = later
            values = [row["operation"]["args"][1] for row in later]
            amount_index = 1 if site.name == "liquidate" else 0
            require(
                values[amount_index] == operands[3]
                and all(value["type"] == word() for value in values),
                f"MIR caller did not return the transfer amount in {site.name}",
            )
            head = expect(
                first,
                operation(
                    "insert_value",
                    [{"kind": "undefined", "type": ty}, values[0]],
                    aggregate_type=ty,
                    index=0,
                ),
                ty,
            )
            packed = expect(
                second,
                operation(
                    "insert_value",
                    [head, values[1]],
                    aggregate_type=ty,
                    index=1,
                ),
                ty,
            )
            require(
                block["terminator"] == operation("return", [packed]),
                f"MIR caller returned a different product in {site.name}",
            )


def call_site(before, after, *, error=None):
    """Contract CALL and returndata materialization to the semantic pair."""
    before = normalize({**before, "blocks": body_blocks(before)})
    after = normalize({**after, "blocks": body_blocks(after)})
    sites = [
        (index, block["instructions"][-2:])
        for index, block in enumerate(before["blocks"])
        if len(block["instructions"]) >= 2
        and block["instructions"][-2]["operation"]["kind"] == "extcall"
        and block["instructions"][-1]["operation"]["kind"] == "returndata_bytes"
    ]
    require(
        len(sites) == 1, "expected one paired semantic CALL and returndata operation"
    )
    index, (call, collect) = sites[0]
    require(len(call["operation"]["args"]) == 3, "unexpected semantic CALL arity")
    address, obj, requested = call["operation"]["args"]
    expect(
        call,
        operation(
            "extcall",
            (address, obj, requested),
            name="call",
            clear_word=True,
            gas=True,
            input=True,
            output_word=False,
            value=False,
        ),
        word(1),
    )
    expect(collect, operation("returndata_bytes"), pointer())
    require(
        address["type"] == word(160)
        and obj["type"] == pointer()
        and requested["type"] == word(),
        "unexpected semantic CALL operand types",
    )
    samples = [
        inst
        for inst in before["blocks"][index]["instructions"][:-2]
        if inst["operation"] == opcode("gas")
    ]
    require(
        len(samples) == 1 and local(samples[0]) == requested,
        "CALL must consume the unchanged GAS sample",
    )

    prefix = len(before["blocks"][index]["instructions"]) - 2
    # Lowering can insert blocks before unrelated blocks in traversal order.
    # Follow identical edges up to the first visit to the changed block.
    pending, seen = [(0, 0)], set()
    while pending:
        left, right = pending.pop()
        if left == index:
            cut = right
            break
        if (left, right) in seen:
            continue
        seen.add((left, right))
        outgoing = targets(before["blocks"][left]["terminator"])
        other = targets(after["blocks"][right]["terminator"])
        require(len(outgoing) == len(other), "changed branch before CALL expansion")
        pending.extend(zip(outgoing, other, strict=True))
    else:
        raise Unsupported("unreachable CALL expansion")
    graph = body_blocks(after)
    head = graph[cut]
    require(len(head["instructions"]) == prefix + 9, "unexpected CALL expansion length")
    data, length, cast, raw, rdlen, plus, and_, size, lt = head["instructions"][prefix:]
    # Use actual post-lowering SSA operands, then let complete normalized
    # equality verify their producer identities against the semantic side.
    for inst in (data, cast, raw):
        require(inst["operation"]["args"], "missing lowered CALL operands")
    actual_obj = data["operation"]["args"][0]
    actual_address = cast["operation"]["args"][0]
    actual_requested = raw["operation"]["args"][0]
    require(
        actual_obj["type"] == pointer()
        and actual_address["type"] == word(160)
        and actual_requested["type"] == word(),
        "unexpected lowered CALL operand types",
    )
    data = expect(data, operation("pointer_add", [actual_obj, constant(32)]), pointer())
    length = expect(length, opcode("mload", actual_obj), word())
    cast = expect(cast, operation("word", [actual_address]), word())
    success = expect(
        raw,
        opcode(
            "call",
            actual_requested,
            cast,
            constant(0),
            data,
            length,
            constant(0),
            constant(0),
        ),
        word(1),
    )
    rdlen = expect(rdlen, opcode("returndatasize"), word())
    plus = expect(plus, opcode("add", rdlen, constant(31)), word())
    aligned = expect(and_, opcode("and", plus, constant(2**256 - 32)), word())
    size = expect(size, opcode("add", aligned, constant(32)), word())
    overflow = expect(lt, opcode("lt", size, rdlen), word(1))
    failure, materialize = branch(head["terminator"], overflow)
    require(len({cut, failure, materialize}) == 3, "aliased CALL expansion blocks")
    predecessors = {
        target: {
            i for i, block in graph.items() if target in targets(block["terminator"])
        }
        for target in (failure, materialize)
    }
    require(
        all(edges == {cut} for edges in predecessors.values()),
        "extra entry to CALL expansion",
    )
    panic(graph[failure], 0x41)
    tail = graph[materialize]
    require(
        not tail["phis"] and len(tail["instructions"]) == 7,
        "invalid returndata expansion",
    )
    alloc, store, make, data, extract, count, copy_ = tail["instructions"]
    obj = expect(
        alloc, operation("allocate", [size], checked=False, zeroed=False), pointer()
    )
    expect(store, opcode("mstore", obj, rdlen))
    ty = slice_type("returndata")
    buffer = expect(
        make,
        operation(
            "make_slice",
            [constant(0, pointer("returndata")), rdlen],
            location="returndata",
        ),
        ty,
    )
    data = expect(data, operation("pointer_add", [obj, constant(32)]), pointer())
    source = expect(
        extract,
        operation("extract_value", [buffer], aggregate_type=ty, index=0),
        pointer("returndata"),
    )
    count = expect(
        count, operation("extract_value", [buffer], aggregate_type=ty, index=1), word()
    )
    expect(copy_, opcode("returndatacopy", data, source, count))
    head["instructions"][prefix:] = [
        instruction(
            {
                **call["operation"],
                "args": [actual_address, actual_obj, actual_requested],
            },
            success["index"],
            word(1),
        ),
        instruction(collect["operation"], obj["index"], pointer()),
    ]
    head["terminator"] = tail["terminator"]
    del graph[failure], graph[materialize]
    for block in graph.values():
        require(
            all(target in graph for target in targets(block["terminator"])),
            "control flow enters a contracted CALL expansion",
        )
        for phi in block["phis"]:
            for arm in phi["incoming"]:
                require(arm[0] != failure, "panic edge reaches a phi")
                if arm[0] == materialize:
                    arm[0] = cut
    require(
        before == normalize({**after, "blocks": graph}),
        "CALL expansion changes the SSA context",
    )
    obj = call["operation"]["args"][1]
    prefix = None
    if error is not None:
        prefix = transfer_prefix(before, index, address, obj, requested)
        transfer_continuation(before, index, call, collect, error)
    return address, obj, requested, prefix


def load_pair(paths, solar, executable, version, timeout):
    from .check import ROOT, run

    texts, models, declarations = [], [], []
    for path in paths:
        require(
            path.suffix == ".mir" and path.stat().st_size <= 1024 * 1024,
            "builtin expansion needs MIR files of at most 1 MiB",
        )
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
        require(parsed.returncode == 0, parsed.stderr)
        texts.append(parsed.stdout)
        models.append(type_prune.export(parsed.stdout, executable, timeout))
        declarations.append(prune.declarations(parsed.stdout))
    require(
        declarations[0][0] == declarations[1][0],
        "builtin expansion changed module declarations",
    )
    return texts, models, [decls[1] for decls in declarations]


def functions(model):
    result = {body["return_buffer"]: body for body in model["functions"]}
    require(
        len(result) == len(model["functions"]),
        "ambiguous builtin expansion function table",
    )
    return result


def unchanged_callees(function, bodies, declarations):
    """Require a closed unchanged supplied context, with no callback into the cut."""
    old, new = declarations
    pending, seen = list(old[function]["targets"]), set()
    while pending:
        name = pending.pop()
        require(name != function, "builtin expansion excludes recursive callers")
        if name in seen:
            continue
        require(
            name in old
            and name in new
            and old[name] == new[name]
            and bodies[0].get(name) == bodies[1].get(name),
            "builtin expansion changed a callee",
        )
        pending.extend(old[name]["targets"])
        seen.add(name)
    return sorted(seen)


def prove(code, timeout, module="SolarTV", proof_path=None):
    from .check import PRELUDE, ROOT, build_model, run

    built = build_model(timeout, module)
    require(built.returncode == 0, built.stdout + built.stderr)
    with tempfile.TemporaryDirectory(prefix="solar-tv-builtins-") as folder:
        proof = proof_path or Path(folder) / "Pair.lean"
        proof.write_text(
            PRELUDE.replace("import SolarTV\n", f"import {module}\n", 1) + code
        )
        result = run(["lake", "env", "lean", proof], ROOT, timeout)
    axioms = re.search(r"'contextual' depends on axioms: \[(.*?)\]", result.stdout)
    require(
        result.returncode == 0
        and (
            "'contextual' does not depend on any axioms" in result.stdout
            or axioms
            and not set(axioms[1].split(", "))
            - {"propext", "Quot.sound", "Classical.choice"}
        ),
        "builtin expansion proof did not close\n" + result.stdout + result.stderr,
    )


def check(
    paths,
    solar,
    executable,
    version,
    timeout,
    function,
    *,
    final_call=None,
    proof_path=None,
):
    from . import authorization_lowering

    texts, models, declarations = load_pair(paths, solar, executable, version, timeout)
    bodies = list(map(functions, models))
    if any(
        function not in table or function not in decls
        for table, decls in zip(bodies, declarations, strict=True)
    ):
        raise Unsupported("builtin expansion could not select a unique function")
    old, new = (table[function] for table in bodies)
    module = "SolarTV"
    if final_call:
        require(
            function == "safeTransferFrom" and version == "cancun",
            "final bytecode certificate applies to Cancun Morpho transferFrom",
        )
    require(
        declarations[0][function]["text"][0] == declarations[1][function]["text"][0],
        "builtin expansion changed the function interface",
    )
    operations = [
        inst["operation"] for block in old["blocks"] for inst in block["instructions"]
    ]
    if any(op["kind"] == "store_storage_bytes" for op in operations):
        from . import storage_lowering

        require(not final_call, "final bytecode certificate requires a CALL")
        helpers = storage_lowering.check_source(
            old,
            new,
            bodies[1],
            [d[function]["targets"] for d in declarations],
            executable,
            timeout,
        )
        kind = "contextual-storage-bytes-lowering"
        require(
            declarations[1][helpers[0]]["text"][0]
            == f"fn @{helpers[0]}(arg0: i256, arg1: i256, arg2: i256) {{",
            "unexpected storage clear helper interface",
        )
        boundary = (
            "unpriced storage-byte CFG with exact clear and write loops; "
            "64 KiB memory and 2048-word per-loop work domain"
        )
        callees = []
        code = STORAGE_PROOF
    elif any(authorization_lowering.is_recovery(op) for op in operations):
        require(
            not final_call and version != "amsterdam",
            "unsupported signing expansion target",
        )
        callees = unchanged_callees(function, bodies, declarations)
        helpers = []
        kind = "contextual-signing-expansion"
        boundary = (
            "identical typed context and immutable binding; exact packed object and "
            "scratch-word effects; GAS paid and its sample covers the incoming "
            "message budget; unpriced caller instructions"
        )
        code = authorization_lowering.source(old, new)
    else:
        require(version != "amsterdam", "Amsterdam MIR message gas is not modeled")
        callees = unchanged_callees(function, bodies, declarations)
        helpers = []
        address, input_, gas, prefix = call_site(
            old, new, error=bodies[0]["revert_error"] if final_call else None
        )
        samples = [
            inst
            for name in (function, *callees)
            for block in bodies[0][name]["blocks"]
            for inst in block["instructions"]
            if inst["operation"] == opcode("gas")
        ]
        require(len(samples) == 1, "unexpected extra GAS observer in CALL context")
        kind = "contextual-call-returndata-lowering"
        boundary = (
            "identical shared SSA prefix and continuation with equal call results "
            "and live state; common sampled GAS and parent message budget; "
            "unpriced caller instructions"
        )
        code = CALL_PROOF
        if final_call:
            kind = "final-bytecode-caller-continuations"
            boundary = (
                "shared state at helper entry; one child execution with the bytecode "
                "CALL budget; typed MIR and physical continuation agree on the copied "
                "returndata and remaining gas; the five actual caller continuations "
                "agree on ABI publication or owning-frame rollback"
            )
            caller_continuations(bodies, declarations, final_call["callers"])
            module = "SolarTV.MIR.Transforms.TransferFromCallers"
            code = (
                FINAL_CALL_PROOF.replace("{trace}", final_call["trace"])
                .replace("{memory}", final_call["memory"])
                .replace("{tail_prices}", final_call["tail_prices"])
                .replace(
                    "{caller_suffixes}", callers.lean_suffixes(final_call["callers"])
                )
                .replace(
                    "{return_addresses}", callers.lean_addresses(final_call["callers"])
                )
                .replace(
                    "{extra_stack}",
                    str(final_call["artifact"]["additional_stack_slots"]),
                )
            )
        for key, value in (("address", address), ("input", input_), ("gas", gas)):
            code = code.replace(f"{{{key}}}", mir_state.operand(value))

    catalog = {row["name"]: row for row in native.catalog(executable, timeout)["evmir"]}
    if final_call:
        for key, instructions in zip(("guard", "setup"), prefix, strict=True):
            rendered = ", ".join(
                mir_state.instruction(inst, catalog) for inst in instructions
            )
            code = code.replace(f"{{{key}}}", f"[{rendered}]")
    for table in bodies:
        for name in (function, *callees, *(h for h in helpers if h in table)):
            dead_ssa.reads(table[name])
            for block in table[name]["blocks"]:
                mir_state.block(block, catalog)
                for inst in block["instructions"]:
                    op = inst["operation"]
                    require(
                        op["kind"] != "layout"
                        or kind == "contextual-signing-expansion"
                        and op["query"] == "immutable_read",
                        "builtin expansion excludes placement observations",
                    )
                    if op["kind"] == "opcode":
                        row = catalog[op["name"]]
                        require(
                            row["forks"][version]["available"]
                            and (
                                not row["observes_code"] or op["name"] == "extcodesize"
                            ),
                            "builtin expansion needs available opcodes independent of the code image",
                        )
    prove(code, timeout, module, final_call["proof"] if final_call else proof_path)
    if final_call and final_call.get("caller_entry"):
        from . import caller_entry

        runtime, directory, worlds = final_call["caller_entry"]
        artifact = final_call["artifact"]
        entry_paths, suite, context, boundaries = caller_entry.check(
            runtime,
            artifact["entry_pc"],
            final_call["callers"],
            bodies,
            directory,
            solar,
            executable,
            timeout,
            worlds,
        )
        transfer = directory / "MIRSafeTransfer.lean"
        check(
            paths,
            solar,
            executable,
            version,
            timeout,
            "safeTransfer",
            proof_path=transfer,
        )
        authorization = directory / "MIRAuthorizationWithSig.lean"
        check(
            paths,
            solar,
            executable,
            version,
            timeout,
            "setAuthorizationWithSig",
            proof_path=authorization,
        )
        artifact["abi_entry"] = {
            "scope": "concrete Cancun worlds from ABI dispatch to exit via the helper",
            "reachability": "lean-kernel",
            "dispatcher": "symbolic-lean-kernel",
            "decoder": "symbolic-lean-kernel",
            "end_to_end": "lean-kernel",
            "independent_replay": "lean-and-rust-replay",
            "snapshot_live_ins": "lean-kernel",
            "paths": entry_paths,
            "proof": suite,
            "shared_mir_context": context,
            **boundaries,
            "safe_transfer_call": {
                "proof": transfer.name,
                "scope": "local CALL and returndata expansion with a shared incoming budget",
            },
            "authorization_with_sig": {
                "proof": authorization.name,
                "scope": "local packed envelope and static recovery call with a paid GAS sample",
            },
        }

    pair = tuple(
        Program(
            (function, "builtin-lowering"),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            selector=function,
            scope="builtin-lowering",
            gas="exact-caller-continuation" if final_call else None,
            evm_version=version,
            import_evidence={
                "kind": kind,
                "source_sha256": hashlib.sha256(
                    dead_ssa.stable(model).encode()
                ).hexdigest(),
                "unchanged_callees": callees,
                "proved_helpers": helpers,
                "boundary": boundary,
                **({"final_bytecode": final_call["artifact"]} if final_call else {}),
            },
        )
        for text, model in zip(texts, models, strict=True)
    )
    return audit.Result("verified", stdout="verified\n", program=pair[0], pair=pair)


CALL_PROOF = """
open SolarTV.MIR.State EvmRules SolarTV.Execution
open SolarTV.MIR.LowerReturndata
theorem contextual {Observation : Type} (cfg : Config) (fuel : Nat) (frame : Frame)
    (target requested : Word) (object : Scalar)
    (ha : read frame {address} = .ok target)
    (hi : readValue frame {input} = .ok (.scalar object))
    (hg : read frame {gas} = .ok requested)
    (bounded : ∀ opcode args preCall request success postCall work,
      SolarTV.MIR.LowerAddressCall.loweredMemoryObjectArgs cfg frame target requested object.word =
        .ok (opcode,args,preCall,request) →
      external fuel cfg preCall opcode args request =
        .ok (some (.returned (success,postCall),work)) →
      postCall.returnData.length + 63 < 2^256)
    (resume : Except String (Option (Outcome (Option Word × Option Word × Frame) × Nat)) →
      Observation) :
    resume (invokeAndCollect cfg fuel
      (messageArguments cfg frame (.address 0xf1 true false true false false) [{address},{input},{gas}])
      returnDataBytes) =
    resume (invokeAndCollect cfg fuel
      (SolarTV.MIR.LowerAddressCall.loweredMemoryObjectArgs cfg frame target requested object.word)
      (lowered cfg)) :=
  congrArg resume (memoryObjectCall cfg fuel frame {address} {input} {gas} target requested object
    ha hi hg bounded)
#print axioms contextual
"""


FINAL_CALL_PROOF = """
open SolarTV.MIR.State EvmRules SolarTV.Execution
open SolarTV.MIR.LowerReturndata SolarTV.MIR.FinalCallBudget SolarTV.MIR.TransferFromFrame
open SolarTV.MIR.TransferFromReturn SolarTV.MIR.TransferFromCallers
open SolarTV.EVM.CallBudget (Entry Trace referenceTrace beforeCall)
def finalTrace : Trace := {trace}
theorem finalShape : finalTrace = referenceTrace 6 := by decide
def finalWrites : List Access := {memory}
theorem finalWritesShape : finalWrites = stores := by decide
def importedGuard : List Instruction := {guard}
def importedSetup : List Instruction := {setup}
theorem guardShape : importedGuard = guard := rfl
theorem setupShape : importedSetup = setup := rfl
def tailPrices : TailPrices := {tail_prices}
theorem tailShape : tailPrices = referencePrices := by decide
def callerSuffix : Caller → Suffix
{caller_suffixes}
def returnAddress : Caller → Word
{return_addresses}
theorem suffixShape (caller : Caller) : callerSuffix caller = reference caller := by
  cases caller <;> decide

theorem separateArenas (frame : Frame) (physical : List Byte) (sender receiver amount : Word)
    (mirAligned : frame.memory.length % 32 = 0) (mirBound : frame.memory.length ≤ 65536)
    (mirLive : 96 ≤ frame.memory.length) (mirHeap : 128 ≤ (readWord frame.memory 64).toNat)
    (mirFits : (readWord frame.memory 64).toNat + 160 ≤ 65536)
    (evmAligned : physical.length % 32 = 0) (evmBound : physical.length ≤ 65536)
    (evmLive : 96 ≤ physical.length) (evmHeap : 128 ≤ (readWord physical 64).toNat)
    (evmFits : (readWord physical 64).toNat + 160 ≤ 65536) :
    ∃ mirAfter evmAfter,
      abiEncode frame 1 false true [false,false,false] [selector,sender,receiver,amount] =
        .ok (.scalar (Scalar.ofWord 256 (readWord frame.memory 64)),{frame with memory := mirAfter}) ∧
      runMemory sender receiver amount finalWrites physical 0 = .ok evmAfter ∧
      readBytes mirAfter ((readWord frame.memory 64).toNat+32) 100 =
        readBytes evmAfter ((readWord physical 64).toNat+32) 100 := by
  rw [finalWritesShape]
  exact independentMemoryInput frame physical sender receiver amount mirAligned mirBound
    mirLive mirHeap mirFits evmAligned evmBound evmLive evmHeap evmFits

theorem contextual (cfg : Config) (fuel : Nat)
    (evm : EVM.State.Frame) (world : Journal) (args : Inputs)
    (sample available child retained words elapsed : Nat)
    (caller : Caller) (other : Word) (rest : List Word) (checkpoint : Journal)
    (stack : evm.stack = returnAddress caller :: widenBits args.token :: widenBits args.sender ::
      widenBits args.receiver :: args.amount :: rest)
    (liveWords : (callerSuffix caller).publication.words rest = .ok (liveOperands caller args.amount other))
    (_stackRoom : evm.stack.length + {extra_stack} ≤ 1024)
    (_returnDest : evm.stack.head?.any (EVM.State.validJump evm.code) = true)
    (probe : cfg.available.contains 0x3b = true) (gasOp : cfg.available.contains 0x5a = true)
    (environment : cfg.environment = evm.environment)
    (known : args.token ≠ evm.environment.address)
    (codeBound : (account world args.token).code.length < 2^256)
    (aligned : evm.memory.length % 32 = 0) (boundedMemory : evm.memory.length ≤ 65536)
    (live : 96 ≤ evm.memory.length)
    (heap : 128 ≤ (Entry.ofFrame world evm args.token).fmp)
    (valid : (Entry.ofFrame world evm args.token).fmp + 160 ≤ 65536)
    (limit : evm.gas ≤ 30000000) (fork : cfg.fork = .cancun)
    (reaches : EVM.CallBudget.run finalTrace (Entry.ofFrame world evm args.token) =
      .call sample available child retained words)
    (earlier : elapsed ≤ beforeCall (Entry.ofFrame world evm args.token) 6)
    (ext : EVM.State.Config) (extFork : ext.fork = .cancun)
    (externalCfg : cfg.externalConfig = some ext)
    (bounded : ∀ cut outcome, runBytes finalWrites evm world = .ok (some cut) →
      byteCall ext fuel evm.environment cut child = .ok outcome →
      (readWord evm.memory 64+160).toNat + outcome.data.length + 63 ≤ 2^64-1) :
    ∃ frame,
      runMirPrefix importedGuard importedSetup cfg evm world args elapsed = .ok (some frame) ∧
      let p := readWord evm.memory 64
      let priced := withBudget cfg frame sample
      let semantic := messageArguments priced frame (.address 0xf1 true false true false false)
        [{address},{input},{gas}]
      let raw := SolarTV.MIR.LowerAddressCall.loweredMemoryObjectArgs priced frame
        (widenBits args.token) (BitVec.ofNat 256 sample) p
      let final := executeCut ext fuel evm.environment (runBytes finalWrites evm world)
        referencePrices (p+160) child retained
      mirContinue caller args.amount other checkpoint
        (resumeMir cfg tailPrices frame.memory (invokeAndCollect priced fuel semantic returnDataBytes)) =
        byteContinue (callerSuffix caller) rest checkpoint final ∧
      mirContinue caller args.amount other checkpoint
        (resumeMir cfg tailPrices frame.memory (invokeAndCollect priced fuel raw (lowered priced))) =
        byteContinue (callerSuffix caller) rest checkpoint final := by
  rw [guardShape,setupShape]
  obtain ⟨frame,mir,bytes,⟨ready⟩,ha,hi,hg,cursor⟩ := fromEntry cfg evm world args finalTrace finalShape
    finalWrites finalWritesShape sample available child retained words elapsed
    ⟨returnAddress caller,rest,stack⟩ probe gasOp
    (environment.symm ▸ known) codeBound aligned boundedMemory live heap valid reaches
  refine ⟨frame,mir,?_⟩
  dsimp only
  let p := readWord evm.memory 64
  let priced := withBudget cfg frame sample
  let raw := SolarTV.MIR.LowerAddressCall.loweredMemoryObjectArgs priced frame
    (widenBits args.token) (BitVec.ofNat 256 sample) p
  let cut : Cut := ⟨frame.memory, frame.world, frame.returnData, args.token,
    readBytes frame.memory (p+32).toNat 100⟩
  have present : runBytes finalWrites evm world = .ok (some cut) := bytes
  have castTarget : (widenBits args.token).setWidth 160 = args.token :=
    wordBits_widenBits 160 _ (by decide)
  have cost := childMatches finalTrace finalShape (Entry.ofFrame world evm args.token)
    heap limit sample available child retained words reaches cfg fork frame ext extFork externalCfg fuel
    p (widenBits args.token) ready
  rw [castTarget] at cost
  have used : invoke priced fuel raw = (do
      let outcome ← byteCall ext fuel cfg.environment cut child
      return some (.returned (some outcome.value,postCall frame retained outcome),outcome.work)) := cost
  have range : ∀ outcome, byteCall ext fuel cfg.environment cut child = .ok outcome →
      (p+160).toNat + outcome.data.length + 63 ≤ 2^64-1 := by
    rw [environment]
    exact fun outcome h => bounded cut outcome present h
  obtain ⟨sampled,_,request,_,_,highest⟩ :=
    samples finalTrace finalShape (Entry.ofFrame world evm args.token)
      heap sample available child retained words elapsed reaches earlier
  change sample = evm.gas - beforeCall (Entry.ofFrame world evm args.token) 6 at sampled
  change sample ≤ evm.gas-elapsed at request
  have sampleWord : (BitVec.ofNat 256 sample).toNat = sample := by
    rw [BitVec.toNat_ofNat,Nat.mod_eq_of_lt]; omega
  have requestWord : (BitVec.ofNat 256 (evm.gas-elapsed)).toNat = evm.gas-elapsed := by
    rw [BitVec.toNat_ofNat,Nat.mod_eq_of_lt]; omega
  have alignedCut : frame.memory.length % 32 = 0 := by rw [ready.extent]; simp
  have liveCut : 96 ≤ frame.memory.length := by
    rw [ready.extent,highest]
    omega
  rw [tailShape,present,← environment]
  have resumed := resumeMatched cfg ext fuel referencePrices frame (p+160)
    cut child retained (invoke priced fuel raw) rfl used alignedCut ready.limit liveCut cursor range
  have semanticEq :
      invoke priced fuel (messageArguments priced frame (.address 0xf1 true false true false false)
        [{address},{input},{gas}]) = invoke priced fuel raw := by
    rw [SolarTV.MIR.LowerAddressCall.memoryObjectArguments _ frame {address} {input} {gas}
      (widenBits args.token) _ (Scalar.ofWord 256 p) ha hi hg]
    exact changeRequest cfg frame fuel (widenBits args.token) p _ _ sample fork
      (by simpa only [requestWord] using request) (by rw [sampleWord]; exact Nat.le_refl _)
      (by rw [requestWord]; omega) (by rw [sampleWord]; omega)
  unfold invokeAndCollect
  rw [semanticEq,resumed]
  have published := resume_eq caller (callerSuffix caller) rest args.amount other checkpoint
    (executeCut ext fuel cfg.environment (.ok (some cut)) referencePrices (p+160) child retained)
    (suffixShape caller) liveWords
  refine ⟨published,?_⟩
  have loweredEq : collectMessage (lowered priced) (invoke priced fuel raw) =
      collectMessage returnDataBytes (invoke priced fuel raw) := by
    rw [used]
    cases executed : byteCall ext fuel cfg.environment cut child with
    | error _ => rfl
    | ok outcome =>
      have fits := range outcome executed
      simp only [collectMessage, Bind.bind, Except.bind, Pure.pure, Except.pure]
      rw [materialize priced (postCall frame retained outcome) (by
        change outcome.data.length + 63 < 2^256
        omega)]
  rw [loweredEq,resumed]
  exact published
#print axioms contextual
"""


STORAGE_PROOF = """
open SolarTV.MIR.State EvmRules
open SolarTV.MIR.LowerStorageBytes
theorem contextual (cfg : Config) (frame : Frame) (slot object : Word) :
    lowered cfg frame slot object = storeStorageBytes cfg frame slot object none :=
  store cfg frame slot object
#print axioms contextual
"""
