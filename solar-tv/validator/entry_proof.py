"""Build and join small kernel-checked equations for each caller entry trace."""

import json
import os
from textwrap import indent

from .callers import require
from .check import ROOT, run

MODULE = "SolarTV.MIR.Transforms.EntryTrace"
WIDTH = 4
GROUP_SIZE = 6
HEADER = """open SolarTV EvmRules MIR.TransferFromEntry
set_option autoImplicit false
set_option maxRecDepth 16384
set_option maxHeartbeats 4000000
set_option Elab.async false
"""


def lean(directory, name, source, timeout, *, module=False):
    path = directory / f"{name}.lean"
    path.write_text(source)
    command = [
        "lake",
        "env",
        "lean",
        "-j1",
        "--memory=2048",
        f"--root={directory}",
    ]
    if module:
        command.extend(["-o", str(path.with_suffix(".olean"))])
    env = os.environ.copy()
    env["LEAN_PATH"] = os.pathsep.join(
        part for part in (str(directory), env.get("LEAN_PATH")) if part
    )
    return run([*command, path], ROOT, timeout, env=env)


def success(result, name):
    require(
        result.returncode == 0,
        f"{name} failed\n" + result.stdout[-4000:] + result.stderr[-4000:],
    )
    return result.stdout


def image_source(image):
    leaves = []
    start = stop = 0
    while start < len(image):
        used = 0
        jumps = []
        while stop < len(image) and stop - start < 48:
            code = image[stop]
            if code == 0x5B:
                jumps.append(stop - start)
            stop += 1 + (code - 0x5F if 0x60 <= code <= 0x7F else 0)
            used += 1
        require(stop <= len(image), "truncated PUSH in the emitted runtime")
        leaves.append((list(image[start:stop]), used, jumps))
        start = stop

    def tree(first, count):
        if count == 1:
            return f"imageBlock{first}"
        left = count // 2
        return f"(.append {tree(first, left)} {tree(first + left, count - left)})"

    source = f"import {MODULE}\n{HEADER}\n"
    for index, (data, used, jumps) in enumerate(leaves):
        source += f"""def imageBlock{index} : EVM.State.CodeChunks :=
  .leaf ⟨{data}, {len(data)}, by decide, {used}, {jumps}, by decide +kernel⟩
"""
    return (
        source
        + f"""def code : EVM.State.CodeChunks := {tree(0, len(leaves))}
def image := code.bytes
"""
    )


def calc(equations, indent="  "):
    return (
        indent
        + "calc\n"
        + "".join(f"{indent}  {equation} := {proof}\n" for equation, proof in equations)
    )


def expression(label, fuel, index, namespace=""):
    return f"Execution.Trace.run {label}Step {fuel - WIDTH * index} {namespace}state{index}"


def group(
    directory, label, base, states, initial_type, cut_type, fuel, start, end, timeout
):
    name = f"{base}{label}{start}"
    lines = [f"import {base}", HEADER, f"namespace {name}"]
    for index in range(start, end + 1):
        ty = cut_type if index == len(states) - 1 else initial_type
        lines.append(
            f"noncomputable def state{index} : {ty} :=\n{indent(states[index], '  ')}"
        )

    equations = []
    lhs = expression(label, fuel, start)
    for index in range(start, end):
        final = index + 1 == len(states) - 1
        next_state = f".{'inl' if final else 'inr'} state{index + 1}"
        lines.append(
            f"check_chunk check{index} : Execution.Trace.segment {label}Step {WIDTH} "
            f"{fuel - WIDTH * index} state{index} = .ok ({next_state})"
        )
        rhs = f".ok state{index + 1}" if final else expression(label, fuel, index + 1)
        lemma = "finish_run" if final else "run_segment"
        operand = f"state{index + 1}" if final else f"({next_state})"
        equations.append(
            (
                f"{lhs} = {rhs}",
                f"Execution.Trace.{lemma} {WIDTH} {fuel - WIDTH * index} state{index} {operand} check{index}",
            )
        )
        lhs = "_"
    lines.append(
        f"theorem checked : {expression(label, fuel, start)} = {rhs} := by\n"
        + calc(equations)
        + f"end {name}"
    )
    source = "\n".join(lines) + "\n"
    result = lean(directory, name, source, timeout, module=True)
    if (
        result.returncode
        and "excessive memory consumption" in result.stdout
        and end - start > 1
    ):
        mid = (start + end) // 2
        return group(
            directory,
            label,
            base,
            states,
            initial_type,
            cut_type,
            fuel,
            start,
            mid,
            timeout,
        ) + group(
            directory,
            label,
            base,
            states,
            initial_type,
            cut_type,
            fuel,
            mid,
            end,
            timeout,
        )
    success(result, name)
    return [(name, start, end)]


def segments(directory, base, label, initial_type, cut_type, fuel, proposals, timeout):
    middle, final = proposals
    states = [f"{label}Start", *middle, final]
    end = len(states) - 1
    groups = []
    for start in range(0, end, GROUP_SIZE):
        groups.extend(
            group(
                directory,
                label,
                base,
                states,
                initial_type,
                cut_type,
                fuel,
                start,
                min(end, start + GROUP_SIZE),
                timeout,
            )
        )
    cut = f"{groups[-1][0]}.state{end}"
    equations = []
    for name, start, stop in groups:
        next_state = (
            f".ok {cut}" if stop == end else expression(label, fuel, stop, name + ".")
        )
        equations.append(
            (
                f"{expression(label, fuel, start, name + '.')} = {next_state}",
                f"{name}.checked",
            )
        )
    imports = "\n".join(f"import {name}" for name, _, _ in groups)
    source = (
        imports
        + "\n"
        + HEADER
        + f"""
noncomputable def {label}Cut : {cut_type} := {cut}
theorem {label}Reached :
    Execution.Trace.run {label}Step {fuel} {label}Start = .ok {label}Cut := by
{calc(equations)}
"""
    )
    module = f"{base}{label}Reached"
    success(lean(directory, module, source, timeout, module=True), module)
    return module, end


def checked_hashes(directory, base, layers, timeout):
    setup = f"import {base}\n{HEADER}"
    inspect = {
        "physical": "(byteHashInput code)",
        "before": "(mirHashInput beforeFunctions)",
        "after": "(mirHashInput afterFunctions)",
    }
    recipe = setup + "".join(
        f"export_hashes ({label}With SolarTV.Keccak.hash) from {label}Start "
        f"using {inspect[label]} fuel {fuel}\n"
        for label, _, _, fuel in layers
    )
    output = success(
        lean(directory, base + "Hashes", recipe, timeout), base + " hashes"
    )
    hashes = {
        tuple(data): int(digest)
        for line in output.splitlines()
        for data, digest in json.loads(line)
    }
    modules = []
    for index, (data, digest) in enumerate(hashes.items()):
        name = f"{base}Hash{index}"
        source = f"""import SolarTV.State.CheckedHashes
open SolarTV EvmRules
set_option maxRecDepth 16384
set_option maxHeartbeats 4000000
def {name} : Keccak.CheckedHash :=
  ⟨{list(data)}, {digest}, by decide +kernel⟩
"""
        success(lean(directory, name, source, timeout, module=True), name)
        modules.append(name)
    memo = f"{base}Memo"
    imports = "".join(f"import {name}\n" for name in modules)
    source = (
        imports
        + setup
        + f"""
def checkedHashes : List SolarTV.Keccak.CheckedHash := [{", ".join(modules)}]
def hashChecked := SolarTV.Keccak.withChecks checkedHashes
theorem hashChecked_eq : hashChecked = SolarTV.Keccak.hash :=
  SolarTV.Keccak.withChecks_eq checkedHashes
"""
    )
    source += "".join(
        f"def {label}Step := {label}With hashChecked\n" for label, _, _, _ in layers
    )
    success(lean(directory, memo, source, timeout, module=True), memo)
    return memo


def certify(directory, name, fixture, observed, pc, timeout):
    base = f"Entry{name[0].upper() + name[1:]}"
    success(lean(directory, base, fixture, timeout, module=True), name)
    layers = (
        ("physical", "ByteCut", "ByteCut", 2048),
        ("before", "Except String MirCursor", "MirCut", 1024),
        ("after", "Except String MirCursor", "MirCut", 1024),
    )
    memo = checked_hashes(directory, base, layers, timeout)
    proposed = f"import {memo}\n{HEADER}"
    for label, _, _, fuel in layers:
        proposed += (
            f"export_prefix {label}Step from {label}Start "
            f"shared (code.bytes) world env width {WIDTH} fuel {fuel}\n"
        )
    stdout = success(lean(directory, base + "Proposals", proposed, timeout), name)
    imports = []
    count = 0
    for (label, state, cut, fuel), line in zip(
        layers, stdout.splitlines(), strict=True
    ):
        module, checked = segments(
            directory, memo, label, state, cut, fuel, json.loads(line), timeout
        )
        imports.append(f"import {module}\n")
        count += checked

    source = (
        "".join(imports)
        + HEADER
        + f"""
open MIR.TransferFromCallers
theorem bytecode_reaches : runBytes {pc} 2048 tvWorldConfig initial = .ok physicalCut := by
  unfold runBytes
  change Execution.Trace.run (byteStep {pc} tvWorldConfig code.bytes) 2048 initial = _
  rw [←chunkedByteStep_eq, ←hashChecked_eq]
  exact physicalReached

theorem cut_pc : physicalCut.frame.pc = {pc} := by decide +kernel
theorem cut_gas : physicalCut.frame.gas = {observed["gas"]} := by decide +kernel
theorem before_reaches :
    mirPrefix mirCfg beforeFunctions beforeEntry beforeHelper beforeReturned selector caller checkpoint
      physicalCut 1024 = .ok beforeCut := by
  unfold mirPrefix
  rw [←hashChecked_eq]
  exact beforeReached
theorem after_reaches :
    mirPrefix mirCfg afterFunctions afterEntry afterHelper afterReturned selector caller checkpoint
      physicalCut 1024 = .ok afterCut := by
  unfold mirPrefix
  rw [←hashChecked_eq]
  exact afterReached

theorem before_live :
    related caller suffix returnPC inputs other physicalCut.frame.stack beforeCut.args beforeCut.returns :=
  by decide +kernel
theorem after_live :
    related caller suffix returnPC inputs other physicalCut.frame.stack afterCut.args afterCut.returns :=
  by decide +kernel

#print axioms bytecode_reaches
#print axioms cut_pc
#print axioms cut_gas
#print axioms before_reaches
#print axioms after_reaches
#print axioms before_live
#print axioms after_live
"""
    )
    name = base + "Proof"
    output = success(lean(directory, name, source, timeout), name)
    require(
        len(output.splitlines()) == 7
        and all(
            "does not depend on any axioms" in line
            or (
                "depends on axioms: [" in line
                and set(line.split("[", 1)[1].removesuffix("]").split(", "))
                <= {"propext", "Quot.sound", "Classical.choice"}
            )
            for line in output.splitlines()
        ),
        "unexpected axioms in caller-entry proof",
    )
    return {"segments": count, "proof": f"kernel/{name}.lean"}
