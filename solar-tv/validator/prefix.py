"""Universal stack-prefix proofs with an unchanged contextual continuation.

Import complete blocks independently through Rust. An identical memory prologue
may precede the changed scalar prefix; it is charged and executed, not erased.
No operation is dropped: the entire suffix, terminal and successor must match. The theorem
quantifies over every stack and live world, rather than inventing an entry height
or a zero-initialized memory fixture. Failed proofs are unsupported, since an
arbitrary stack counterexample need not be reachable in the original module.
"""

import hashlib
import json
import re
import subprocess
import tempfile
from pathlib import Path

from . import audit, evmir, native, regions
from .common import Program, Unsupported

UNFOLD = (
    "EVM.Prefix.run, EVM.stepChecked, EVM.step, Evm.add, BitVec.add_comm, "
    "EVM.ContextualPeephole.odd_word"
)


def export(text, entry, version, executable, timeout, *, code_tail=False):
    response = subprocess.run(
        [executable, "--evm-model"],
        input=json.dumps(
            {
                "version": 1,
                "text": text,
                "entry": int(entry.removeprefix("bb")),
                "evm_version": version,
                "window": True,
                **({"window_code_tail": True} if code_tail else {}),
            }
        ),
        text=True,
        capture_output=True,
        check=False,
        timeout=timeout,
    )
    try:
        value = json.loads(response.stdout)
    except ValueError:
        raise Unsupported("invalid independent EVM window export") from None
    if response.returncode or value.get("version") != 1 or value.get("status") != "ok":
        raise Unsupported("EVM window: " + value.get("reason", "invalid export"))
    model = value["model"]
    if (
        not isinstance(model.get("instructions"), list)
        or not all(isinstance(op, str) for op in model["instructions"])
        or not isinstance(model.get("terminal"), str)
    ):
        raise Unsupported("invalid independent EVM window operations")
    return model


def operation(line, catalog, version):
    # Stack instructions use the actual schema's minimum depth and net effect.
    name = line.split()[0]
    if line.startswith("push "):
        value = line[5:]
        if not value.isdecimal():
            return None
        name = (
            "push0"
            if int(value) == 0 and catalog["push0"]["forks"][version]["available"]
            else "push1"
        )
    elif match := re.fullmatch(r"(dup|swap) (\d+)", line):
        if not 1 <= int(match[2]) <= 16:
            return None
        name = match[1] + match[2]
    definition = catalog.get(name)
    if not definition or not (
        definition["pure"]
        or name.startswith(("push", "dup", "swap"))
        or name in evmir.CONTEXT_READS | {"pop", "calldatasize", "returndatasize"}
    ):
        return None
    price = definition["forks"][version]
    if not price["available"] or price["dynamic_gas"] or definition["stack"] is None:
        return None
    try:
        operations, _, _, _ = evmir.instructions(
            [line], None, memory=False, context=True
        )
    except Unsupported:
        return None
    if len(operations) != 1:
        raise Unsupported("prefix import changed operation count")
    return operations[0], definition["stack"], price["base_gas"]


def split(model, catalog, version):
    rendered = []
    required = peak = delta = price = 0
    for line in model["instructions"]:
        op = operation(line, catalog, version)
        if op is None:
            break
        text, (inputs, outputs), charge = op
        required = max(required, inputs - delta)
        delta += outputs - inputs
        peak = max(peak, delta)
        price += charge
        rendered.append(text)
    if len(rendered) > 64 or required + peak > 32:
        raise Unsupported("prefix exceeds the 64-operation/32-word proof budget")
    suffix = {**model, "instructions": model["instructions"][len(rendered) :]}
    return rendered, suffix, required, peak, price


def proof_body(stack, offset):
    definitions = f"before, after, {UNFOLD}, List.length_cons, Nat.add_assoc"
    # Exhaustive symbolic partition: an unbounded tail plus the small band near
    # the stack limit. No entry height is sampled or inferred here.
    hypotheses = ", ".join(f"h{i}" for i in range(offset + 1))
    lines = [f"by_cases huge : {stack}.length > 1024"]
    lines += [f"· have h0 : 1024 < {stack}.length := by omega"]
    lines += [
        f"  have h{i} : 1024 < {stack}.length + {i} := by omega"
        for i in range(1, offset + 1)
    ]
    lines += [f"  simp [{definitions}, {hypotheses}]"]
    for i in range(offset):
        lines += [
            f"by_cases edge{i} : {stack}.length = {1024 - i}",
            f"· simp [{definitions}, edge{i}]",
        ]
    lines += [
        f"have h{i} : ¬1024 < {stack}.length{' + ' + str(i) if i else ''} := by omega"
        for i in range(offset + 1)
    ]
    lines += [f"simp [{definitions}, {hypotheses}]"]
    return "\n".join(lines)


def stack_proof(required, peak):
    def cases(index):
        if index == required:
            return proof_body("stack", required + peak)
        return (
            "cases stack with\n"
            + f"| nil => simp [before, after, {UNFOLD}]\n"
            + f"| cons word{index} stack =>\n"
            + "\n".join("  " + line for line in cases(index + 1).splitlines())
        )

    return cases(0)


def check(paths, solar, executable, timeout, entry, version, policy):
    from .check import PRELUDE, ROOT, build_model, run

    if not re.fullmatch(r"bb[0-9]{1,10}", entry) or int(entry[2:]) > 2**32 - 1:
        raise Unsupported("prefix entry must be a valid block label")
    catalog = {row["name"]: row for row in native.catalog(executable, timeout)["evmir"]}
    texts, models = [], []
    for path in paths:
        if path.suffix != ".evmir" or path.stat().st_size > 1024 * 1024:
            raise Unsupported("prefix checks require EVM IR files of at most 1 MiB")
        result = run(
            [
                solar,
                path.resolve(),
                "-Zevm-ir-pipeline=none",
                "-Zvalidate-ir=true",
                f"--evm-version={version}",
            ],
            ROOT,
            timeout,
        )
        if result.returncode:
            raise Unsupported(result.stderr)
        texts.append(result.stdout)
        models.append(export(result.stdout, entry, version, executable, timeout))
    regions.compatible_windows(*texts, entry)
    headers = [re.split(r"(?m)^bb\d+", text, maxsplit=1)[0] for text in texts]
    headers = [
        "\n".join(line for line in text.splitlines() if not line.startswith(";"))
        for text in headers
    ]
    if headers[0] != headers[1]:
        raise Unsupported("prefix shared declarations changed")
    if policy is None:
        from . import stop_tail

        terminal = stop_tail.try_check(
            texts, models, entry, catalog, version, executable, timeout
        )
        if terminal is not None:
            return terminal
    for model in models:
        for line in model["instructions"]:
            name = line.split()[0]
            definition = catalog.get(name)
            if definition and (
                definition["observes_code"] or definition["sets_returndata"]
            ):
                from . import suffix

                return suffix.check(
                    texts, models, entry, catalog, version, policy, timeout
                )
    from . import interior

    shared = interior.common_length(
        models[0]["instructions"], models[1]["instructions"]
    )
    if models[0]["instructions"] != models[1]["instructions"] and any(
        catalog.get(line, {}).get("observes_gas", False)
        for line in models[0]["instructions"][:shared]
    ):
        # An exact scalar island can follow an unchanged gas-sensitive prefix.
        # Keep that prefix and its early exits, rather than assuming it succeeds.
        return interior.check(texts, models, entry, catalog, version, timeout)
    old, suffix, need_a, peak_a, price_a = split(models[0], catalog, version)
    new, next_suffix, need_b, peak_b, price_b = split(models[1], catalog, version)
    prologue = []
    if suffix != next_suffix:
        # Partition only at an identical leading instruction sequence. Its
        # memory effects and charges remain in the generated execution theorem.
        common = 0
        for index, (left, right) in enumerate(
            zip(models[0]["instructions"], models[1]["instructions"])
        ):
            if left != right:
                break
            if operation(left, catalog, version) is None:
                # Keep common scalar producers inside the scalar proof. Moving
                # them into the prologue would lose useful known stack values.
                common = index + 1
        if not 0 < common <= 64:
            raise Unsupported("prefix continuation changed")
        try:
            prologue, _, _, _ = evmir.instructions(
                models[0]["instructions"][:common],
                [None] * 32,
                memory=True,
                context=True,
            )
        except IndexError:
            raise Unsupported(
                "shared memory prologue exceeds the stack budget"
            ) from None
        except Unsupported:
            from . import evm_effects

            # A projected heap cannot represent symbolic addresses. A distinct
            # interaction certificate retains every effect and dynamic charge.
            return evm_effects.check(
                texts, models, entry, catalog, version, policy, timeout
            )
        tails = [
            {**model, "instructions": model["instructions"][common:]}
            for model in models
        ]
        old, suffix, need_a, peak_a, price_a = split(tails[0], catalog, version)
        new, next_suffix, need_b, peak_b, price_b = split(tails[1], catalog, version)
    if suffix != next_suffix:
        raise Unsupported("prefix continuation changed")
    if max(need_a, need_b) + max(peak_a, peak_b) > 32:
        raise Unsupported("combined prefix exceeds the 32-word proof budget")
    exact = price_a == price_b
    if not exact and any(
        catalog.get(line.split()[0], {}).get("observes_gas", False)
        for line in suffix["instructions"]
    ):
        raise Unsupported("cheaper prefix has a gas-sensitive continuation")
    if price_b > price_a or policy == "exact" and not exact:
        raise Unsupported("prefix does not preserve the requested gas policy")
    programs = [
        Program(
            ("evm-prefix", "any"),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            selector=entry,
            scope="prefix-window",
            gas="exact" if exact else "refine",
            evm_version=version,
            import_evidence={
                "kind": "independent-complete-block-prefix",
                "source_sha256": hashlib.sha256(
                    json.dumps(model, sort_keys=True).encode()
                ).hexdigest(),
                "prefix_operations": len(code),
                **({"shared_prologue_operations": len(prologue)} if prologue else {}),
                "continuation": "identical complete suffix, terminal and physical successor",
                "boundary": "trusted Rust parser/source export, schema and opcode mapping; logical code-insensitive continuation",
            },
        )
        for text, model, code in zip(texts, models, (old, new), strict=True)
    ]
    theorem = (
        f"def before : List EVM.Op := [{','.join(old)}]\n"
        f"def after : List EVM.Op := [{','.join(new)}]\n"
        "theorem stacks (ctx : EVM.Context) (stack : EVM.Stack) :\n"
        " EVM.Prefix.run ctx before stack = EVM.Prefix.run ctx after stack := by\n"
        + "\n".join(
            "  " + line
            for line in stack_proof(
                max(need_a, need_b), max(peak_a, peak_b)
            ).splitlines()
        )
        + f"\ntheorem prices : EVM.Prefix.cost .{version} {'before = EVM.Prefix.cost .' + version + ' after' if exact else 'after ≤ EVM.Prefix.cost .' + version + ' before'} := by decide\n"
    )
    contextual = (
        "theorem contextual {World Result : Type} (ctx : EVM.Context)\n"
        + (
            " (continuation : EVM.Stack → Nat → World → Result)"
            if exact
            else " (continuation : EVM.Stack → World → Result)"
        )
        + " (fault : Result) (stack : EVM.Stack) (gas : Nat) (world : World)"
        + ("" if exact else f" (enough : EVM.Prefix.cost .{version} before ≤ gas)")
        + " :\n"
        + f" (match EVM.Prefix.charged ctx .{version} before stack gas with | none => fault | some (next, remaining) => continuation next {'remaining ' if exact else ''}world) =\n"
        + f" (match EVM.Prefix.charged ctx .{version} after stack gas with | none => fault | some (next, remaining) => continuation next {'remaining ' if exact else ''}world) :=\n"
        + f" EVM.Prefix.{'contextual' if exact else 'sufficient_contextual'} ctx .{version} before after (stacks ctx) prices continuation fault stack gas {'enough ' if not exact else ''}world\n"
        + "#print axioms contextual\n"
    )
    theorem += (
        shared_theorem(prologue, old, new, version, exact) if prologue else contextual
    )
    build = build_model(timeout)
    if build.returncode:
        raise Unsupported(build.stdout + build.stderr)
    with tempfile.TemporaryDirectory(prefix="solar-tv-prefix-") as folder:
        proof = Path(folder) / "Prefix.lean"
        proof.write_text(PRELUDE + theorem)
        result = run(["lake", "env", "lean", proof], ROOT, timeout)
    allowed = {"propext", "Quot.sound", "Classical.choice"}
    axioms = re.search(r"'contextual' depends on axioms: \[(.*?)\]", result.stdout)
    if result.returncode or not axioms or set(axioms[1].split(", ")) - allowed:
        raise Unsupported(
            "universal prefix proof did not close; no reachable counterexample established\n"
            + result.stdout
            + result.stderr
        )
    return audit.Result(
        "verified", stdout="verified\n", program=programs[0], pair=tuple(programs)
    )


def shared_theorem(prologue, before, after, version, exact):
    """Bind the scalar proof to charged execution of the common memory prefix."""

    def fixed(name, ops):
        proof = (
            "simp only ["
            + name
            + ", List.mem_cons, List.not_mem_nil, or_false] at member\n"
        )
        if ops:
            proof += "  rcases member with " + " | ".join("rfl" for _ in ops) + "\n"
            proof += "  all_goals simp [EVM.Gas.cost, EVM.Gas.baseCost, EVM.Gas.touched, EVM.Gas.expansionCost]\n"
        return (
            f"theorem {name}_fixed : EVM.MemoryPrefix.Fixed .{version} {name} := by\n"
            + "  intro op member stack memory\n  "
            + proof
        )

    code = f"def prologue : List EVM.Memory.Op := [{','.join(prologue)}]\n"
    code += fixed("before", before) + fixed("after", after)
    code += (
        "theorem contextual (ctx : EVM.Context) (stack : EVM.Stack) (gas : Nat) (memory : Memory.State)\n"
        + (
            ""
            if exact
            else f" (enough : ∀ next remaining heap, EVM.MemoryPrefix.charged ctx .{version} prologue stack gas memory = some (next,remaining,heap) → EVM.Prefix.cost .{version} before ≤ remaining)\n"
        )
        + " :\n"
    )
    observations = [
        f"EVM.MemoryPrefix.charged ctx .{version} (prologue ++ {name}.map EVM.Memory.Op.stack) stack gas memory"
        for name in ("before", "after")
    ]
    if not exact:
        observations = [f"EVM.MemoryPrefix.forgetGas ({term})" for term in observations]
    code += " " + " =\n ".join(observations) + " :=\n"
    code += (
        f" EVM.MemoryPrefix.shared_{'exact' if exact else 'sufficient'} ctx .{version} prologue before after before_fixed after_fixed (stacks ctx) prices stack gas memory"
        + ("" if exact else " enough")
        + "\n#print axioms contextual\n"
    )
    return code
