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


def call_site(before, after, *, from_entry=False):
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
    prefix = (
        transfer_prefix(before, index, address, obj, requested) if from_entry else None
    )
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


def check(paths, solar, executable, version, timeout, function, *, final_call=None):
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
    if any(
        inst["operation"]["kind"] == "store_storage_bytes"
        for block in old["blocks"]
        for inst in block["instructions"]
    ):
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
    else:
        require(version != "amsterdam", "Amsterdam MIR message gas is not modeled")
        callees = unchanged_callees(function, bodies, declarations)
        helpers = []
        address, input_, gas, prefix = call_site(old, new, from_entry=bool(final_call))
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
            kind = "final-bytecode-call-returndata-lowering"
            boundary = (
                "shared state at helper entry; typed MIR and decoded final bytes "
                "produce the same CALL memory, target, warm journal and returndata; "
                "final-bytecode prices derive the common message budget"
            )
            module = "SolarTV.MIR.Transforms.TransferFromFrame"
            code = (
                FINAL_CALL_PROOF.replace("{trace}", final_call["trace"])
                .replace("{memory}", final_call["memory"])
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
    prove(code, timeout, module, final_call["proof"] if final_call else None)
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
            gas="exact-call-boundary" if final_call else None,
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
open SolarTV.EVM.CallBudget (Entry Trace referenceTrace beforeCall)
def finalTrace : Trace := {trace}
theorem finalShape : finalTrace = referenceTrace 6 := by decide
def finalWrites : List Access := {memory}
theorem finalWritesShape : finalWrites = stores := by decide
def importedGuard : List Instruction := {guard}
def importedSetup : List Instruction := {setup}
theorem guardShape : importedGuard = guard := rfl
theorem setupShape : importedSetup = setup := rfl

private theorem contextualAtCut {Observation : Type} (cfg : Config) (fuel : Nat) (frame : Frame)
    (entry : Entry) (sample available child retained words elapsed : Nat)
    (heap : 128 ≤ entry.fmp) (limit : entry.gas ≤ 30000000)
    (fork : cfg.fork = .cancun)
    (reaches : EVM.CallBudget.run finalTrace entry = .call sample available child retained words)
    (earlier : elapsed ≤ beforeCall entry 6)
    (target : Word) (object : Scalar)
    (ext : EVM.State.Config) (extFork : ext.fork = .cancun)
    (externalCfg : cfg.externalConfig = some ext)
    (ready : Ready entry frame object.word target words)
    (ha : read frame {address} = .ok target)
    (hi : readValue frame {input} = .ok (.scalar object))
    (hg : read frame {gas} = .ok (BitVec.ofNat 256 (entry.gas-elapsed)))
    (bounded : ∀ opcode args preCall request success postCall work,
      SolarTV.MIR.LowerAddressCall.loweredMemoryObjectArgs (withBudget cfg frame sample)
        frame target (BitVec.ofNat 256 (entry.gas-elapsed)) object.word =
        .ok (opcode,args,preCall,request) →
      external fuel (withBudget cfg frame sample) preCall opcode args request =
        .ok (some (.returned (success,postCall),work)) →
      postCall.returnData.length + 63 < 2^256)
    (resume : Except String (Option (Outcome (Option Word × Option Word × Frame) × Nat)) →
      Observation) :
    (resume (invokeAndCollect (withBudget cfg frame sample) fuel
      (messageArguments (withBudget cfg frame sample) frame (.address 0xf1 true false true false false)
        [{address},{input},{gas}]) returnDataBytes) =
    resume (invokeAndCollect (withBudget cfg frame sample) fuel
      (SolarTV.MIR.LowerAddressCall.loweredMemoryObjectArgs (withBudget cfg frame sample)
        frame target (BitVec.ofNat 256 sample) object.word) (lowered (withBudget cfg frame sample)))) ∧
    priceAtCut cfg frame sample target object.word = .ok (some (child,retained)) := by
  refine ⟨?_, readyPrice finalTrace finalShape entry heap limit sample available child retained words
    reaches cfg fork frame ext extFork externalCfg object.word target ready⟩
  obtain ⟨priced,_,request,_,_,_⟩ :=
    samples finalTrace finalShape entry heap sample available child retained words elapsed reaches earlier
  have sampleLimit : sample ≤ 30000000 := by omega
  have requestLimit : entry.gas-elapsed ≤ 30000000 := by omega
  have sampleWord : (BitVec.ofNat 256 sample).toNat = sample := by
    simp [BitVec.toNat_ofNat, Nat.mod_eq_of_lt (by omega : sample < 2^256)]
  have requestWord : (BitVec.ofNat 256 (entry.gas-elapsed)).toNat = entry.gas-elapsed := by
    simp [BitVec.toNat_ofNat, Nat.mod_eq_of_lt (by omega : entry.gas-elapsed < 2^256)]
  apply congrArg resume
  rw [memoryObjectCall _ fuel frame {address} {input} {gas} target _ object ha hi hg bounded]
  apply changeRequest cfg frame fuel target object.word _ _ sample fork
  · simpa [requestWord] using request
  · simp [sampleWord]
  · simpa [requestWord] using requestLimit
  · simpa [sampleWord] using sampleLimit
theorem contextual {Observation : Type} (cfg : Config) (fuel : Nat)
    (evm : EVM.State.Frame) (world : Journal) (args : Inputs)
    (sample available child retained words elapsed : Nat)
    (stack : ∃ ret rest, evm.stack = ret :: widenBits args.token :: widenBits args.sender ::
      widenBits args.receiver :: args.amount :: rest)
    (_stackRoom : evm.stack.length + {extra_stack} ≤ 1024)
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
    (bounded : ∀ frame opcode callArgs preCall request success postCall work,
      runMirPrefix importedGuard importedSetup cfg evm world args elapsed = .ok (some frame) →
      SolarTV.MIR.LowerAddressCall.loweredMemoryObjectArgs (withBudget cfg frame sample)
        frame (widenBits args.token) (BitVec.ofNat 256 (evm.gas-elapsed)) (readWord evm.memory 64) =
        .ok (opcode,callArgs,preCall,request) →
      external fuel (withBudget cfg frame sample) preCall opcode callArgs request =
        .ok (some (.returned (success,postCall),work)) →
      postCall.returnData.length + 63 < 2^256)
    (resume : Except String (Option (Outcome (Option Word × Option Word × Frame) × Nat)) →
      Observation) :
    ∃ frame,
      runMirPrefix importedGuard importedSetup cfg evm world args elapsed = .ok (some frame) ∧
      runBytes finalWrites evm world = .ok (some ⟨frame.memory,frame.world,frame.returnData,
        args.token,readBytes frame.memory (readWord evm.memory 64+32).toNat 100⟩) ∧
      (resume (invokeAndCollect (withBudget cfg frame sample) fuel
        (messageArguments (withBudget cfg frame sample) frame (.address 0xf1 true false true false false)
          [{address},{input},{gas}]) returnDataBytes) =
      resume (invokeAndCollect (withBudget cfg frame sample) fuel
        (SolarTV.MIR.LowerAddressCall.loweredMemoryObjectArgs (withBudget cfg frame sample)
          frame (widenBits args.token) (BitVec.ofNat 256 sample) (readWord evm.memory 64))
          (lowered (withBudget cfg frame sample)))) ∧
      priceAtCut cfg frame sample (widenBits args.token) (readWord evm.memory 64) =
        .ok (some (child,retained)) := by
  rw [guardShape,setupShape] at bounded ⊢
  obtain ⟨frame,mir,bytes,⟨ready⟩,ha,hi,hg⟩ := fromEntry cfg evm world args finalTrace finalShape
    finalWrites finalWritesShape sample available child retained words elapsed stack probe gasOp
    (environment.symm ▸ known) codeBound aligned boundedMemory live heap valid reaches
  refine ⟨frame,mir,bytes,?_⟩
  exact contextualAtCut cfg fuel frame (Entry.ofFrame world evm args.token)
    sample available child retained words elapsed heap limit fork reaches earlier
    (widenBits args.token) (Scalar.ofWord 256 (readWord evm.memory 64))
    ext extFork externalCfg ready ha hi hg
    (fun opcode callArgs preCall request success postCall work =>
      bounded frame opcode callArgs preCall request success postCall work mir) resume
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
