"""Universal effectful prefixes with a proved per-interaction resource policy.

The independently exported block and unchanged suffix come from prefix.check.
Stateful operations retain their schema signature and consumed words; replies,
world changes, dynamic prices and failures are universally quantified. Nothing
assumes constant memory addresses or separates potentially aliasing accesses.
Adjacent static conditional transfers retain both outcomes and the temporary
label stack slot. Their physical charges come from a shared symbolic context;
logical label numbers never stand in for physical offsets or PUSH widths.
"""

import hashlib
import json
import re
import tempfile
from pathlib import Path

from . import audit, prefix, regions
from .common import Program, Unsupported

EFFECTS = {
    "mload",
    "mstore",
    "mstore8",
    "mcopy",
    "msize",
    "keccak256",
    "log0",
    "log1",
    "log2",
    "log3",
    "log4",
}
COMPOSED_EFFECTS = EFFECTS | {"address", "sload", "sstore", "timestamp"}


def instructions(
    lines, catalog, version, labels=None, *, code_tail=False, composed=False
):
    admitted = COMPOSED_EFFECTS if composed else EFFECTS
    rendered = []
    required = peak = delta = 0
    index = 0
    while index < len(lines):
        line = lines[index]
        if line == "size-init-tail" and code_tail:
            # The recognized PUSH/CODESIZE/SUB sequence uses two scratch
            # slots and leaves one word. Its matched relocation and charge
            # are supplied by the bound constructor code context.
            rendered.extend((".reserve 2", ".effect 256 0 1"))
            peak = max(peak, delta + 2)
            delta += 1
            index += 1
            continue
        if guard := re.fullmatch(r"push bb(\d+)", line):
            target = int(guard[1])
            if (
                labels is None
                or f"bb{target}" not in labels
                or index + 1 == len(lines)
                or lines[index + 1] != "jumpi"
            ):
                raise Unsupported(
                    "effectful prefix requires a paired static conditional target"
                )
            rendered.append(f".guard {target} (prices {target}).1 (prices {target}).2")
            # The temporary label PUSH precedes consumption of the condition.
            peak = max(peak, delta + 1)
            required = max(required, 1 - delta)
            delta -= 1
            index += 2
            continue
        scalar = prefix.operation(line, catalog, version)
        if scalar is not None:
            code, (inputs, outputs), _ = scalar
            rendered.append(f".scalar ({code})")
        else:
            row = catalog.get(line)
            if (
                line not in admitted
                or row is None
                or not row["forks"][version]["available"]
                or row["observes_code"]
                or row["observes_gas"]
                and line != "sstore"
                or row["sets_returndata"]
                or row["stack"] is None
            ):
                raise Unsupported(
                    "effectful prefix contains an observer, call or relocation"
                )
            inputs, outputs = row["stack"]
            if outputs > 1:
                raise Unsupported(
                    "effectful prefix requires at most one result per effect"
                )
            rendered.append(f".effect {row['opcode']} {inputs} {outputs}")
        required = max(required, inputs - delta)
        delta += outputs - inputs
        peak = max(peak, delta)
        index += 1
    if not composed and (len(lines) > 64 or required + peak > 32):
        raise Unsupported(
            "effectful prefix exceeds the 64-operation/32-word proof budget"
        )
    return rendered, required, peak


def stack_proof(required, peak, extra="", *, discharger="omega"):
    definitions = (
        "before, after, EVM.Effects.expand, EVM.Effects.Refines, EVM.stepChecked, "
        "EVM.step, EVM.Effects.guardStack, EVM.Gas.pureCost, Fork.atLeast, Evm.add, BitVec.add_comm, "
        "List.length_cons, Nat.add_assoc, "
        "List.getElem?_cons, List.set, Nat.add_lt_iff_lt_sub_right, "
        + ", ".join(f"index{i}" for i in range(16))
        + extra
    )
    solve = f"simp (disch := {discharger}) [{definitions}, {{hypotheses}}]"

    def body():
        offset = required + peak
        hypotheses = ", ".join(f"h{i}" for i in range(offset + 1))
        lines = ["by_cases huge : stack.length > 1024"]
        lines += ["· have h0 : 1024 < stack.length := by omega"]
        lines += [
            f"  have h{i} : 1024 < stack.length + {i} := by omega"
            for i in range(1, offset + 1)
        ]
        lines += ["  " + solve.format(hypotheses=hypotheses)]
        for i in range(offset):
            lines += [
                f"by_cases edge{i} : stack.length = {1024 - i}",
                "· " + solve.format(hypotheses=f"edge{i}"),
            ]
        lines += [
            f"have h{i} : ¬1024 < stack.length{' + ' + str(i) if i else ''} := by omega"
            for i in range(offset + 1)
        ]
        # Subtraction-based effect arities need lower as well as upper bounds.
        lines += [solve.format(hypotheses=hypotheses)]
        return "\n".join(lines)

    def cases(index):
        if index == required:
            return body()
        return (
            "cases stack with\n"
            + "| nil => "
            + solve.format(hypotheses="Nat.add_zero")
            + "\n"
            + f"| cons word{index} stack =>\n"
            + "\n".join("  " + line for line in cases(index + 1).splitlines())
        )

    return cases(0)


def check(texts, models, entry, catalog, version, policy, timeout):
    from .check import PRELUDE, ROOT, build_model, run

    if version == "amsterdam":
        raise Unsupported(
            "effectful prefixes do not model Amsterdam's two-resource gas"
        )
    if policy == "exact":
        raise Unsupported(
            "effectful prefix currently proves resource refinement, not exact gas"
        )
    for model in models:
        for line in model["instructions"]:
            row = catalog.get(line.split()[0])
            if row and (
                row["observes_code"] or row["observes_gas"] or row["sets_returndata"]
            ):
                raise Unsupported(
                    "effectful prefix contains a gas/code observer or call"
                )
    left, right = (model["instructions"] for model in models)
    common = 0
    for before, after in zip(reversed(left), reversed(right)):
        if before != after:
            break
        common += 1
    # Retain the shared admissible tail in the execution proof. Cutting before
    # it can distinguish temporary stack peaks even when both complete prefixes
    # encounter the same later stack fault. Relocations/transfers stay boundaries.
    while common:
        line = left[len(left) - common]
        if prefix.operation(line, catalog, version) is None and line not in EFFECTS:
            break
        common -= 1
    if (
        models[0]["terminal"] != models[1]["terminal"]
        or models[0]["following"] != models[1]["following"]
    ):
        raise Unsupported("effectful prefix continuation changed")
    changed = [code[:-common] if common else code for code in (left, right)]
    labels = set(regions.blocks(texts[0]))
    imported = [instructions(code, catalog, version, labels) for code in changed]
    if not any(".effect " in op for code, _, _ in imported for op in code):
        raise Unsupported("effectful prefix requires an explicit stateful operation")
    need = max(row[1] for row in imported)
    peak = max(row[2] for row in imported)
    if need + peak > 32:
        raise Unsupported("combined effectful prefix exceeds the stack proof budget")
    declarations = "".join(
        f"def {name} (prices : Nat → Nat × Nat) : List EVM.Effects.Op := [{','.join(row[0])}]\n"
        for name, row in zip(("before", "after"), imported, strict=True)
    )
    proof = f"""
theorem interaction (prices : Nat → Nat × Nat) (ctx : EVM.Context) (stack : EVM.Stack) :
 EVM.Effects.Refines (EVM.Effects.expand ctx .{version} (after prices) stack 0)
   (EVM.Effects.expand ctx .{version} (before prices) stack 0) := by
"""
    # Kernel-checked numeral reductions expose Fin stack indices under effect
    # continuations, where Lean's restricted instance transparency keeps them opaque.
    proof += (
        "\n".join(
            f"  have index{i} : (({i} : Fin 256) : Nat) = {i} := by decide"
            for i in range(16)
        )
        + "\n"
    )
    proof += "\n".join("  " + line for line in stack_proof(need, peak).splitlines())
    proof += f"""
theorem contextual {{World : Type}} (handler : EVM.Effects.Handler World)
 (prices : Nat → Nat × Nat) (ctx : EVM.Context) (stack : EVM.Stack)
 (result : EVM.Effects.Exit) (world final : World)
 (gas remaining extra : Nat)
 (success : EVM.Effects.charged handler ctx .{version} (before prices) stack
   gas world = some (result,remaining,final)) :
 ∃ remaining', EVM.Effects.charged handler ctx .{version} (after prices) stack
   (gas + extra) world = some (result,remaining',final) ∧ remaining ≤ remaining' := by
 have reference : EVM.Effects.execute handler (EVM.Effects.expand ctx .{version} (before prices) stack 0)
     gas world = some (result,remaining,final) := by
   simpa only [EVM.Effects.expansion_correct, Nat.not_lt_zero, ite_false, Nat.sub_zero] using success
 have refined := EVM.Effects.resource handler _ _ (interaction prices ctx stack)
   world final result gas remaining extra reference
 simpa only [EVM.Effects.expansion_correct, Nat.not_lt_zero, ite_false, Nat.sub_zero] using refined
#print axioms contextual
"""
    build = build_model(timeout)
    if build.returncode:
        raise Unsupported(build.stdout + build.stderr)
    with tempfile.TemporaryDirectory(prefix="solar-tv-evm-effects-") as folder:
        path = Path(folder) / "Pair.lean"
        path.write_text(
            PRELUDE + "set_option maxHeartbeats 2000000\n" + declarations + proof
        )
        result = run(["lake", "env", "lean", path], ROOT, timeout)
    axioms = re.search(r"'contextual' depends on axioms: \[(.*?)\]", result.stdout)
    if (
        result.returncode
        or not axioms
        or set(axioms[1].split(", ")) - {"propext", "Quot.sound", "Classical.choice"}
    ):
        raise Unsupported(
            "effectful stack proof did not close; no reachable counterexample established\n"
            + result.stdout
            + result.stderr
        )
    programs = tuple(
        Program(
            ("evm-effects", "any"),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            selector=entry,
            scope="effectful-prefix",
            gas="refine",
            evm_version=version,
            import_evidence={
                "kind": "independent-effectful-block-prefix",
                "source_sha256": hashlib.sha256(
                    json.dumps(model, sort_keys=True).encode()
                ).hexdigest(),
                "prefix_operations": len(code),
                "shared_suffix_operations": common,
                "conditional_transfers": sum(
                    bool(re.fullmatch(r"push bb\d+", line)) for line in code
                ),
                "continuation": "identical suffix, terminal and physical successor",
                "boundary": "trusted parser/schema export and stack/effect renderer; generic gas-independent handler correspondence and logical label continuation, no physical code-placement certificate",
            },
        )
        for text, model, code in zip(texts, models, changed, strict=True)
    )
    return audit.Result(
        "verified", stdout="verified\n", program=programs[0], pair=programs
    )
