"""Automatic typed SSA checks over arbitrary caller frames and environments.

The default proof uses the independent Rust source export and direct Lean
interpreter, without a synthesized initial world. Both executions must complete
for all typed arguments and every stated initial frame. Large scalar bodies may
instead use a separately reported compositional effect-tree certificate; that
route retains a trusted renderer boundary rather than claiming direct-state
correspondence. Equal model errors, work exhaustion and unproved memory ranges
cannot satisfy the direct relation. Fixtures only confirm counterexamples.
"""

import hashlib
import json
import re
import subprocess
import tempfile
from pathlib import Path

from . import audit, mir, mir_effects, mir_state, native
from .common import Inconclusive, Program, Unsupported


def acyclic(blocks):
    visited, active = set(), set()

    def visit(index):
        if index in active:
            raise Unsupported("symbolic MIR cycles require an invariant")
        if index in visited:
            return
        if type(index) is not int or not 0 <= index < len(blocks):
            raise Unsupported("invalid source MIR edge")
        active.add(index)
        term = blocks[index]["terminator"]
        match term["kind"]:
            case "jump":
                targets = [term["target"]]
            case "branch":
                targets = [term["then"], term["else"]]
            case "switch":
                targets = [term["default"], *(target for _, target in term["cases"])]
            case _:
                targets = []
        for target in targets:
            visit(target)
        active.remove(index)
        visited.add(index)

    visit(0)


def call_work(functions, entry):
    cached, active = {}, set()

    def visit(index):
        if index in active:
            raise Unsupported("recursive symbolic MIR calls require an invariant")
        if index in cached:
            return cached[index]
        if index not in functions:
            raise Unsupported("missing symbolic MIR callee")
        active.add(index)
        work = 0
        for block in functions[index]["blocks"]:
            work += 2 + len(block["instructions"]) + len(block["phis"])
            for inst in block["instructions"]:
                if inst["operation"]["kind"] == "call":
                    work += visit(inst["operation"]["function"])
            if block["terminator"]["kind"] == "tail_call":
                work += visit(block["terminator"]["function"])
            if work > 8192:
                raise Unsupported("symbolic MIR call graph exceeds 8192 work units")
        active.remove(index)
        cached[index] = work
        return work

    return visit(entry)


def translate(text, function, executable, version, timeout):
    response = subprocess.run(
        [executable, "--mir-model"],
        input=json.dumps({"version": 1, "text": text, "function": function}),
        text=True,
        capture_output=True,
        check=False,
        timeout=timeout,
    )
    try:
        value = json.loads(response.stdout)
        if (
            response.returncode
            or value.get("version") != 1
            or value.get("status") != "ok"
        ):
            raise Unsupported("MIR source: " + value.get("reason", "invalid export"))
        model = value["model"]
        functions = {body["id"]: body for body in model["functions"]}
        if model["entry_id"] not in functions or len(functions) != len(
            model["functions"]
        ):
            raise Unsupported("independent MIR entry mismatch")
        for body in functions.values():
            acyclic(body["blocks"])
        work = call_work(functions, model["entry_id"])
        definitions = {
            row["name"]: row for row in native.catalog(executable, timeout)["evmir"]
        }
        for function_body in model["functions"]:
            for block in function_body["blocks"]:
                for inst in block["instructions"]:
                    op = inst["operation"]
                    if (
                        op["kind"] == "opcode"
                        and not definitions[op["name"]]["forks"][version]["available"]
                    ):
                        raise Unsupported(
                            f"MIR opcode `{op['name']}` is unavailable on {version}"
                        )
        code = (
            "["
            + ",".join(
                mir_state.function_body(body, definitions)
                for body in model["functions"]
            )
            + "]"
        )
        widths = [
            int(mir_state.width(leaf["bits"]))
            for ty in model["params"]
            for leaf in mir_state.leaf_types(ty)
        ]
        returns = [leaf for ty in model["returns"] for leaf in mir_state.leaf_types(ty)]
        if len(widths) > 32 or len(returns) > 256:
            raise Unsupported(
                "symbolic MIR boundary exceeds 32 input/256 return leaves"
            )
    except Unsupported:
        raise
    except (ValueError, KeyError, TypeError, IndexError) as error:
        raise Unsupported("invalid independent MIR source export") from error
    available = [
        str(row["opcode"])
        for row in definitions.values()
        if row["forks"][version]["available"]
    ]
    names = iter(f"arg{i}" for i in range(len(widths)))
    arguments = (
        "["
        + ",".join(
            "(" + mir_state.argument_value(ty, names) + ")" for ty in model["params"]
        )
        + "]"
    )
    # The call/CFG analysis bounds all acyclic executions, including repeated calls.
    fuel = work + 4
    header = next(
        (
            line
            for line in text.splitlines()
            if line.startswith(f"fn @{model['name']}(")
        ),
        "",
    )
    header = mir.interface_attributes(header)
    source = (
        f"let cfg : MIR.State.Config := ⟨.{version},environment,context,[{','.join(available)}],false,128,none,[],{{}},[],debug,none⟩\n"
        f"MIR.State.execute {fuel} cfg {code} {model['entry_id']} {{frame with arguments := {arguments}, locals := []}}"
    )
    return Program(
        (
            model["name"],
            "symbolic-mir",
            json.dumps(model["params"], sort_keys=True),
            json.dumps(model["returns"], sort_keys=True),
            header,
            model["layout_declarations"],
        ),
        [f"arg{i}" for i in range(len(widths))],
        "MIR.State.Run",
        source,
        tuple(widths),
        canonical_ir=text,
        replay_ir=text,
        selector=model["name"],
        scope="symbolic-function",
        evm_version=version,
        import_evidence={
            "kind": "independent-typed-source-interpreter",
            "source_sha256": hashlib.sha256(
                json.dumps(model, sort_keys=True).encode()
            ).hexdigest(),
            "boundary": "trusted compiler parser/schema export and constructor renderer; direct typed SSA execution without an expression projection",
        },
        symbolic_mir={"model": model, "fuel": fuel, "code": code},
    )


def declaration(name, program):
    binders = " ".join(
        f"({arg} : BitVec {width})"
        for arg, width in zip(program.inputs, program.input_widths, strict=True)
    )
    code = program.symbolic_mir["code"]
    body = program.body.replace("cfg " + code, "cfg " + name + "Code")
    return (
        f"def {name}Code : MIR.State.Functions :=\n  {code}\n"
        + f"def {name} (environment : World.Environment) (context : Execution.Block)\n"
        f" (debug : Bool) (frame : MIR.State.Frame) {binders} : MIR.State.Run :=\n"
        + "\n".join("  " + line for line in body.splitlines())
        + "\n"
    )


def check(a, b, timeout, *, replay_command, replay_executable, verbose):
    from .check import PRELUDE, ROOT, build_model, run
    from .world_check import MIR_UNFOLD

    programs = []
    for program in (a, b):
        # Reconstruct from the original independently parsed source. A cached
        # execution body or interface cannot certify itself after being changed.
        source = translate(
            program.replay_ir or program.canonical_ir,
            program.selector or program.interface[0],
            replay_executable,
            program.evm_version,
            timeout,
        )
        if program.symbolic_mir is not None and (
            program.body != source.body
            or program.interface != source.interface
            or program.inputs != source.inputs
            or program.input_widths != source.input_widths
            or program.result_type != source.result_type
        ):
            raise Unsupported(
                "symbolic MIR import does not match independent source export"
            )
        programs.append(source)
    a, b = programs
    if a.interface != b.interface:
        raise Unsupported("symbolic MIR interfaces or declarations differ")
    # Large call-free effectful bodies can spend most of their proof budget
    # unfolding identical storage and memory. Try the exact interaction-trace
    # obligation first; it is explicitly reported as a different certificate.
    if (
        sum(
            len(block["instructions"])
            for function in a.symbolic_mir["model"]["functions"]
            for block in function["blocks"]
        )
        >= 40
    ):
        try:
            return mir_effects.check(a, b, timeout, replay_executable)
        except Unsupported:
            pass
    build = build_model(timeout)
    if build.returncode:
        raise Unsupported(build.stdout + build.stderr)
    binders = " ".join(
        f"({arg} : BitVec {width})"
        for arg, width in zip(a.inputs, a.input_widths, strict=True)
    )
    args = " ".join(a.inputs)
    # Keep the arbitrary journal and byte contents abstract. Only the finite SSA
    # machine and known scalar operations are normalized before comparing effects.
    unfold = MIR_UNFOLD
    for term in (
        "before",
        "after",
        "mirWorldConfig",
        "MIR.State.equivalent",
        "MIR.State.observe",
        "Execution.readWord",
        "Execution.readBytes",
        "Execution.wordBytes",
        "Execution.writeBytes",
        "Execution.account",
        "Execution.putAccount",
        "Execution.warmSlot",
        "Execution.warmAddress",
        "Execution.slotsRead",
        "Execution.slotsWrite",
        "Execution.normalizedSlots",
        "Execution.ordered",
        "Execution.orderedInsert",
        "MIR.State.function",
    ):
        unfold = unfold.replace(term + ", ", "")
    unfold += ", ↓MIR.State.reduceFunction, ↓MIR.State.reduceEquivalent, BitVec.ofNatLT_eq_ofNat"
    declarations = (
        PRELUDE
        + "set_option maxRecDepth 16384\nset_option maxHeartbeats 2000000\n"
        + declaration("before", a)
        + declaration("after", b)
    )
    # Normalize bounds in the reached path before another split. Observation
    # stays behind a completed-run guard, so pending continuations do not expand
    # into a product of duplicated result projections.
    theorem = f"""
theorem pair_correct (environment : World.Environment) (context : Execution.Block)
 (debug : Bool) (frame : MIR.State.Frame) {binders}
 (valid : MIR.Universal.MemoryValid frame) :
 MIR.State.equivalent (before environment context debug frame {args})
   (after environment context debug frame {args}) := by
  rcases valid with ⟨bounded, aligned⟩
  have alignment : 32 * (frame.memory.length / 32) = frame.memory.length := by omega
  unfold before after
  conv => arg 1; simp [{unfold}]
  conv => arg 2; simp [{unfold}]
  repeat (split_mir_guard <;>
    (try simp_all [{unfold}, Nat.add_comm]) <;>
    (try simp (disch := omega) [*, {unfold}, Decidable.imp_iff_not_or,
      Decidable.not_and_iff_not_or_not, or_comm, Nat.add_comm]) <;> try omega)
  all_goals first | rfl | omega | (solve | simp [MIR.State.equivalent, MIR.State.observe, BitVec.add_comm])
#print axioms pair_correct
"""
    with tempfile.TemporaryDirectory(prefix="solar-tv-symbolic-mir-") as folder:
        path = Path(folder) / "Pair.lean"
        path.write_text(declarations + theorem)
        result = run(["lake", "env", "lean", path], ROOT, timeout)
    axioms = re.search(r"'pair_correct' depends on axioms: \[(.*?)\]", result.stdout)
    if not result.returncode and (
        "'pair_correct' does not depend on any axioms" in result.stdout
        or axioms
        and not set(axioms[1].split(", "))
        - {"propext", "Quot.sound", "Classical.choice"}
    ):
        return audit.Result("verified", stdout="verified\n", program=a, pair=(a, b))
    witness = counterexample(a, b, timeout, replay_command, replay_executable, verbose)
    if witness is not None:
        return witness
    raise Unsupported(
        "universal typed MIR proof did not close; unsupported execution is not equivalence\n"
        + result.stdout
        + result.stderr
    )


def counterexample(a, b, timeout, replay_command, executable, verbose):
    from . import world, world_check

    def safe(program):
        model = program.symbolic_mir["model"]
        # Erased pointer types can carry caller/layout preconditions. A bare
        # lowered body needs explicit frame evidence before proposing a witness.
        if re.search(r"^@phase lowered", program.replay_ir, re.MULTILINE):
            return False
        types = [*model["params"], *model["returns"]]
        for function in model["functions"]:
            types.extend([*function["params"], *function["returns"]])
            for block in function["blocks"]:
                for phi in block["phis"]:
                    types.append(phi["type"])
                for inst in block["instructions"]:
                    if inst["type"] is not None:
                        types.append(inst["type"])
                    types.extend(value["type"] for value in inst["operation"]["args"])
        if any(
            leaf.get("kind", "word") != "word"
            for ty in types
            for leaf in mir_state.leaf_types(ty)
        ):
            return False
        # Empty memory is only a witness candidate, never a proof precondition.
        # Pointer/FMP/allocation-dependent inputs need separate validity evidence.
        pure = {
            name
            for name, row in (
                (row["name"], row)
                for row in native.catalog(executable, timeout)["evmir"]
            )
            if row["pure"]
        }
        kinds = {
            "word",
            "low_bits",
            "sign_extend",
            "checked",
            "check",
            "insert_value",
            "extract_value",
            "call",
        }

        def valid_memory(op):
            return (
                op["kind"] == "opcode"
                and op["name"] in {"mload", "mstore", "mstore8"}
                and op["args"][0]["kind"] == "constant"
                and op["args"][0]["type"].get("kind", "word") == "word"
                and int(op["args"][0]["value"]) < 65504
            )

        return all(
            valid_memory(inst["operation"])
            or inst["operation"]["kind"] in kinds
            or inst["operation"]["kind"] == "opcode"
            and inst["operation"]["name"]
            in pure | {"sload", "sstore", "tload", "tstore"}
            for body in model["functions"]
            for block in body["blocks"]
            for inst in block["instructions"]
        )

    if not safe(a) or not safe(b):
        return None
    slots = {"storage": {}, "transient": {}}
    for program in (a, b):
        for function in program.symbolic_mir["model"]["functions"]:
            for block in function["blocks"]:
                for inst in block["instructions"]:
                    op = inst["operation"]
                    if (
                        op["kind"] == "opcode"
                        and op["name"] in {"sload", "sstore", "tload", "tstore"}
                        and op["args"][0]["kind"] == "constant"
                    ):
                        space = "transient" if op["name"].startswith("t") else "storage"
                        if (
                            space == "transient"
                            and program.symbolic_mir["model"]["entry"]
                        ):
                            continue
                        if len(slots[space]) < 4:
                            slots[space][op["args"][0]["value"]] = "1"
    # Try one nonempty state as well: an empty world can hide an omitted load.
    # These are witness proposals only; each accepted mismatch is kernel-checked
    # and independently replayed. No fixture establishes universal verification.
    payloads = [{"version": 1}]
    if any(slots.values()):
        payloads.append({"version": 1, "accounts": [{"address": 65536, **slots}]})
    for payload in payloads:
        payload["fuel"] = max(a.symbolic_mir["fuel"], b.symbolic_mir["fuel"])
        with tempfile.TemporaryDirectory(prefix="solar-tv-mir-witness-") as folder:
            path = Path(folder) / "world.json"
            path.write_text(json.dumps(payload))
            fixture = world.read(path, "", None, kind="mir")
        try:
            pairs = [
                mir_state.translate(
                    program.replay_ir,
                    "",
                    program.evm_version,
                    fixture,
                    timeout,
                    program.selector,
                    executable,
                )
                for program in (a, b)
            ]
            witness = world_check.check(
                *pairs,
                timeout,
                replay_command=replay_command,
                replay_executable=executable,
                verbose=verbose,
            )
        except Inconclusive:
            raise
        except Unsupported:
            continue
        if witness.status == "counterexample":
            return witness
    return None
