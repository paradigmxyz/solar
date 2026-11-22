"""Import complete acyclic EVM regions, preserving stack/memory across transfers.

Follow every syntactic successor through terminal outcomes, including shared
tails and physical JUMPI continuations. No artificial outgoing frontier or stack
correspondence is assumed inside the region. Bounded expansion rejects cycles,
dynamic/escaping labels and operations outside the existing memory fragment.
"""

import re
import tempfile
from pathlib import Path

from . import audit, evmir, memory, regions
from .common import Program, Unsupported

UNFOLD = "EVM.Region.execute, EVM.Region.instructions"


class UnmodeledInstruction(Unsupported):
    """An instruction failed projection before any import certificate ran."""


def translate(text: str, entry: str, height: int | None) -> Program:
    bodies = regions.blocks(text)
    if entry not in bodies or height is not None and not 0 <= height <= 1024:
        raise Unsupported("missing region entry or invalid stack height")
    # Preserve the original physical successor even when reachable blocks are
    # non-contiguous or the selected entry is not first in layout order.
    labels = list(bodies)
    following = dict(zip(labels, [*labels[1:], None], strict=True))
    visited = set()
    expansions = 0
    instruction_count = 0
    peak = 0

    def block(label, constants, path):
        nonlocal expansions, instruction_count
        if label not in bodies:
            raise Unsupported("missing region successor")
        if label in path:
            raise Unsupported("cyclic EVM regions require loop invariants")
        if len(path) >= 128:
            raise Unsupported("region path depth exceeds 128 blocks")
        expansions += 1
        instruction_count += len(bodies[label])
        if expansions > 256 or instruction_count > 2048:
            raise Unsupported("region expansion budget exceeded")
        visited.add(label)
        return sequence(label, bodies[label], list(constants), {*path, label})

    def sequence(label, body, constants, path):
        nonlocal peak
        ops = []
        index = 0
        while index < len(body):
            line = body[index]
            if pushed := re.fullmatch(r"push (bb\d+)", line):
                if index + 1 == len(body) or body[index + 1] not in {"jump", "jumpi"}:
                    raise Unsupported("region label escapes its static transfer")
                target = pushed[1]
                if body[index + 1] == "jump":
                    if index + 2 != len(body):
                        raise Unsupported(
                            "instructions after unconditional region transfer"
                        )
                    peak = max(peak, len(constants) + 1)
                    flow = f".jump 1 ({block(target, constants, path)})"
                else:
                    if not constants:
                        raise Unsupported("region branch lacks its condition")
                    peak = max(peak, len(constants) + 1)
                    yes = block(target, constants[1:], path)
                    no = sequence(label, body[index + 2 :], constants[1:], path)
                    flow = f".branch ({yes}) ({no})"
                break
            if jump := re.fullmatch(r"jump (bb\d+)", line):
                if index + 1 != len(body):
                    raise Unsupported("instructions after region terminator")
                target = jump[1]
                peak = max(peak, len(constants) + int(target != following[label]))
                flow = f".jump {int(target != following[label])} ({block(target, constants, path)})"
                break
            if branch := re.fullmatch(r"jumpi (bb\d+), (bb\d+)", line):
                if index + 1 != len(body) or not constants:
                    raise Unsupported("invalid region branch")
                peak = max(peak, len(constants) + 1)
                yes = block(branch[1], constants[1:], path)
                no = block(branch[2], constants[1:], path)
                flow = f".branch ({yes}) ({no})"
                break
            if line in {"return", "revert", "stop", "invalid"}:
                if index + 1 != len(body):
                    raise Unsupported("instructions after region halt")
                if line in {"return", "revert"}:
                    if len(constants) < 2:
                        raise Unsupported("region output lacks offset and size")
                    memory.active_words(constants[0], constants[1])
                    terminal = f".output {'true' if line == 'revert' else 'false'}"
                else:
                    terminal = f".control .{line}"
                flow = f".terminal ({terminal})"
                break
            try:
                added, constants, _, _ = evmir.instructions(
                    [line], constants, memory=True, context=height is None
                )
            except IndexError:
                raise Unsupported(
                    "region entry stack cannot execute its instructions"
                ) from None
            ops.extend(added)
            peak = max(peak, len(constants))
            index += 1
        else:
            target = following[label]
            flow = (
                f".jump 0 ({block(target, constants, path)})"
                if target is not None
                else ".terminal (.control .stop)"
            )
        return f".instructions [{','.join(ops)}] ({flow})" if ops else flow

    # No incoming word is assumed constant. Unknown memory addresses remain
    # unsupported, while execution still uses the complete real incoming stack,
    # including underflow and overflow during a concrete transfer.
    code = block(entry, [None] * (height or 0), set())
    stack = [f"stack{i}" for i in range(height or 0)]
    inputs = ["calldatasize", "returndatasize", *stack, *memory.INPUTS]
    projection = (
        "@module Region\n"
        + "\n".join(
            f"{label}:\n" + "\n".join(bodies[label])
            for label in labels
            if label in visited
        )
        + "\n"
    )
    if len(projection.encode()) > 128 * 1024:
        raise Unsupported("selected region exceeds 128 KiB")
    if height is None:
        return Program(
            ("evm-universal-region", "memory"),
            [],
            "EVM.Memory.Execution",
            "EVM.Region.execute ctx __IMPORT___code stack memory",
            canonical_ir=projection,
            memory=True,
            replay_ir=text,
            selector=entry,
            scope="universal-region",
            region_code=code,
            stack_peak=peak,
        )
    return Program(
        ("evm-region", str(height), "memory"),
        inputs,
        "EVM.Memory.Execution",
        f"EVM.Region.execute ⟨calldatasize, returndatasize, 0, [], 0⟩ (__IMPORT___code) [{','.join(stack)}] ({memory.INITIAL})",
        (*(256 for _ in range(2 + height)), *memory.WIDTHS),
        canonical_ir=projection,
        memory=True,
        replay_ir=text,
        selector=entry,
        scope="region",
        region_code=code,
    )


def cyclic(text, entry, height):
    """Keep an unbounded source graph; acceptance needs graph correspondence."""
    return Program(
        ("evm-cyclic-region", str(height)),
        ["calldatasize", "returndatasize"],
        "Correspondence.Graph EVM.Region.State EVM.Memory.Execution",
        "__IMPORT___graph ⟨calldatasize,returndatasize,0,[], 0⟩",
        (256, 256),
        canonical_ir=text,
        replay_ir=text,
        selector=entry,
        scope="cyclic-region",
    )


def check_universal(a, b, timeout):
    """Prove complete terminal regions for every stack and projected memory."""
    from .check import PRELUDE, ROOT, build_model, run

    if (
        a.scope != b.scope
        or a.interface != b.interface
        or a.evm_version != b.evm_version
    ):
        raise Unsupported("universal EVM region interfaces differ")
    required = max(a.stack_required, b.stack_required)
    peak = required + max(a.stack_peak, b.stack_peak)
    if peak > 32:
        raise Unsupported("universal region exceeds the 32-word proof budget")
    build = build_model(timeout)
    if build.returncode:
        raise Unsupported(build.stdout + build.stderr)

    def declaration(name, program):
        return (
            program.import_certificate
            + f"def {name} (ctx : EVM.Context) (stack : EVM.Stack) (memory : Memory.State) : EVM.Memory.Execution :=\n"
            + f"  {program.body}\n"
            + program.import_obligation
        ).replace("__IMPORT__", name)

    unfold = (
        "before, after, before_code, after_code, "
        "EVM.Region.execute, EVM.Region.instructions, EVM.Memory.step, "
        "EVM.Memory.finish, EVM.finish, EVM.stepChecked, EVM.step, "
        "Except.bind, Except.map, Bind.bind, Functor.map, List.length_cons, Nat.add_assoc"
    )
    proof = ["by_cases huge : stack.length > 1024"]
    proof += ["· have h0 : 1024 < stack.length := by omega"]
    proof += [
        f"  have h{i} : 1024 < stack.length + {i} := by omega"
        for i in range(1, peak + 1)
    ]
    hypotheses = ", ".join(f"h{i}" for i in range(peak + 1))
    proof += [f"  simp [{unfold}, {hypotheses}]"]
    for i in range(peak):
        proof += [
            f"by_cases edge{i} : stack.length = {1024 - i}",
            f"· simp [{unfold}, edge{i}]",
        ]
    proof += [
        f"have h{i} : ¬1024 < stack.length{' + ' + str(i) if i else ''} := by omega"
        for i in range(peak + 1)
    ]
    proof += [f"simp [{unfold}, {hypotheses}]"]

    def cases(index):
        if index == required:
            return "\n".join(proof)
        return (
            "cases stack with\n"
            + f"| nil => simp [{unfold}]\n"
            + f"| cons word{index} stack =>\n"
            + "\n".join("  " + line for line in cases(index + 1).splitlines())
        )

    theorem = (
        "theorem pair_correct (ctx : EVM.Context) (stack : EVM.Stack) (memory : Memory.State) :\n"
        " before ctx stack memory = after ctx stack memory := by\n"
        + "\n".join("  " + line for line in cases(0).splitlines())
        + "\n#print axioms pair_correct\n"
    )
    with tempfile.TemporaryDirectory(prefix="solar-tv-universal-region-") as directory:
        path = Path(directory) / "Pair.lean"
        path.write_text(
            PRELUDE + declaration("before", a) + declaration("after", b) + theorem
        )
        result = run(["lake", "env", "lean", path], ROOT, timeout)
    axioms = re.search(r"'pair_correct' depends on axioms: \[(.*?)\]", result.stdout)
    if result.returncode or not (
        "'pair_correct' does not depend on any axioms" in result.stdout
        or axioms
        and not set(axioms[1].split(", "))
        - {"propext", "Quot.sound", "Classical.choice"}
    ):
        # An arbitrary incoming stack need not be reachable in the full module.
        # Failure of this stronger theorem is not a compiler counterexample.
        raise Unsupported(
            "universal EVM region proof did not close\n" + result.stdout + result.stderr
        )
    return audit.Result("verified", stdout="verified\n", program=a, pair=(a, b))
