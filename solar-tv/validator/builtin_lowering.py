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

from . import audit, dead_ssa, mir_state, native, prune, type_prune
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


def call_site(before, after):
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
    return address, call["operation"]["args"][1], requested


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


def prove(code, timeout):
    from .check import PRELUDE, ROOT, build_model, run

    built = build_model(timeout)
    require(built.returncode == 0, built.stdout + built.stderr)
    with tempfile.TemporaryDirectory(prefix="solar-tv-builtins-") as folder:
        proof = Path(folder) / "Pair.lean"
        proof.write_text(PRELUDE + "set_option maxRecDepth 4096\n" + code)
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


def check(paths, solar, executable, version, timeout, function):
    texts, models, declarations = load_pair(paths, solar, executable, version, timeout)
    bodies = list(map(functions, models))
    if any(
        function not in table or function not in decls
        for table, decls in zip(bodies, declarations, strict=True)
    ):
        raise Unsupported("builtin expansion could not select a unique function")
    old, new = (table[function] for table in bodies)
    require(
        declarations[0][function]["text"][0] == declarations[1][function]["text"][0],
        "builtin expansion changed the function interface",
    )
    if any(
        inst["operation"]["kind"] == "store_storage_bytes"
        for block in old["blocks"]
        for inst in block["instructions"]
    ):
        from . import storage_lowering

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
    else:
        require(version != "amsterdam", "Amsterdam MIR message gas is not modeled")
        callees = unchanged_callees(function, bodies, declarations)
        helpers = []
        address, input_, gas = call_site(old, new)
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
        for key, value in (("address", address), ("input", input_), ("gas", gas)):
            code = code.replace(f"{{{key}}}", mir_state.operand(value))

    catalog = {row["name"]: row for row in native.catalog(executable, timeout)["evmir"]}
    for table in bodies:
        for name in (function, *callees, *(h for h in helpers if h in table)):
            dead_ssa.reads(table[name])
            for block in table[name]["blocks"]:
                mir_state.block(block, catalog)
                for inst in block["instructions"]:
                    op = inst["operation"]
                    require(
                        op["kind"] != "layout",
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
    prove(code, timeout)
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
            evm_version=version,
            import_evidence={
                "kind": kind,
                "source_sha256": hashlib.sha256(
                    dead_ssa.stable(model).encode()
                ).hexdigest(),
                "unchanged_callees": callees,
                "proved_helpers": helpers,
                "boundary": boundary,
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


STORAGE_PROOF = """
open SolarTV.MIR.State EvmRules
open SolarTV.MIR.LowerStorageBytes
theorem contextual (cfg : Config) (frame : Frame) (slot object : Word) :
    lowered cfg frame slot object = storeStorageBytes cfg frame slot object none :=
  store cfg frame slot object
#print axioms contextual
"""
