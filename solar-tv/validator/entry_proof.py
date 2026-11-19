"""Build and join small kernel-checked equations for each caller entry trace."""

import json
import os
from functools import partial
from textwrap import indent

from . import dispatch_proof
from .callers import require
from .check import ROOT, run

MODULE = "SolarTV.MIR.Transforms.EntryTrace"
WIDTH = 4
GROUP_SIZE = 6
PROPOSAL_BATCH = 48
HEADER = """open SolarTV EvmRules MIR.TransferFromEntry
set_option autoImplicit false
set_option maxRecDepth 16384
set_option maxHeartbeats 4000000
set_option Elab.async false
"""


def lean(directory, name, source, timeout, *, module=False, namespace=None):
    path = directory / f"{name}.lean"
    if namespace is not None:
        imports, _, body = source.partition("\nopen SolarTV")
        require(body, "caller proof has no Lean header")
        source = (
            f"{imports}\nnamespace {namespace}\nopen SolarTV{body}\nend {namespace}\n"
        )
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
    prove, label, base, states, initial_type, cut_type, fuel, start, end, timeout
):
    name = f"{base}{label}{start}"
    lines = [f"import {base}", HEADER, f"namespace {name}"]
    for index in range(start, end + 1):
        terminal = index == len(states) - 1
        ty = cut_type if terminal else initial_type
        declaration = "def" if terminal else "noncomputable def"
        lines.append(
            f"{declaration} state{index} : {ty} :=\n{indent(states[index], '  ')}"
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
    result = prove(name, source, timeout, module=True)
    if (
        result.returncode
        and "excessive memory consumption" in result.stdout
        and end - start > 1
    ):
        mid = (start + end) // 2
        return group(
            prove,
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
            prove,
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


def segments(prove, base, label, initial_type, cut_type, fuel, proposals, timeout):
    middle, final = proposals
    states = [f"{label}Start", *middle, final]
    end = len(states) - 1
    groups = []
    for start in range(0, end, GROUP_SIZE):
        groups.extend(
            group(
                prove,
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
def {label}Cut : {cut_type} := {cut}
theorem {label}Reached :
    Execution.Trace.run {label}Step {fuel} {label}Start = .ok {label}Cut := by
{calc(equations)}
"""
    )
    module = f"{base}{label}Reached"
    success(prove(module, source, timeout, module=True), module)
    return module, end


def checked_hashes(prove, base, layers, timeout):
    setup = f"import {base}\n{HEADER}"
    inspect = {
        "physical": "(byteHashInput code)",
        "byteTail": "(byteHashInput code)",
        "before": "(mirHashInput beforeFunctions)",
        "beforeTail": "(mirHashInput beforeFunctions)",
        "after": "(mirHashInput afterFunctions)",
        "afterTail": "(mirHashInput afterFunctions)",
    }
    recipe = setup + "".join(
        f"export_hashes ({label}With SolarTV.Keccak.hash) from {label}Start "
        f"using {inspect[label]} fuel {fuel}\n"
        for label, _, _, fuel in layers
    )
    output = success(prove(base + "Hashes", recipe, timeout), base + " hashes")
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
        success(prove(name, source, timeout, module=True), name)
        modules.append(name)
    memo = f"{base}Memo"
    imports = "".join(f"import {name}\n" for name in modules)
    source = (
        imports
        + setup
        + f"""
namespace {memo}
def checkedHashes : List SolarTV.Keccak.CheckedHash := [{", ".join(modules)}]
def hashChecked := SolarTV.Keccak.withChecks checkedHashes
theorem hashChecked_eq : hashChecked = SolarTV.Keccak.hash :=
  SolarTV.Keccak.withChecks_eq checkedHashes
end {memo}
"""
    )
    source += "".join(
        f"def {label}Step := {label}With {memo}.hashChecked\n"
        for label, _, _, _ in layers
    )
    success(prove(memo, source, timeout, module=True), memo)
    return memo


def phase(prove, base, layers, timeout):
    memo = checked_hashes(prove, base, layers, timeout)
    imports = []
    count = 0
    for label, state, cut, fuel in layers:
        middle = []
        while True:
            start = len(middle)
            name = f"{base}{label}Proposals{start}"
            proposed = (
                f"import {memo}\n{HEADER}"
                f"export_prefix {label}Step from {label}Start "
                f"shared (code.bytes) world env width {WIDTH} fuel {fuel} "
                f"skip {start} limit {PROPOSAL_BATCH}\n"
            )
            batch, final = json.loads(success(prove(name, proposed, timeout), name))
            middle.extend(batch)
            if final is not None:
                break
            require(
                len(batch) == PROPOSAL_BATCH and WIDTH * len(middle) <= fuel,
                "invalid checkpoint export",
            )
        module, checked = segments(
            prove, memo, label, state, cut, fuel, (middle, final), timeout
        )
        imports.append(module)
        count += checked
    return memo, imports, count


def import_modules(modules):
    return "".join(f"import {name}\n" for name in modules)


def check_axioms(lines, count):
    require(
        len(lines) == count
        and all(
            "does not depend on any axioms" in line
            or (
                "depends on axioms: [" in line
                and set(line.split("[", 1)[1].removesuffix("]").split(", "))
                <= {"propext", "Quot.sound", "Classical.choice"}
            )
            for line in lines
        ),
        "unexpected axioms in caller proof",
    )


def certify_dispatch(prove, base, route, helper_pc, selector, runtime_size, timeout):
    name = base + "Dispatch"
    imports = (base, "SolarTV.MIR.Transforms.TransferFromABI")
    source = (
        import_modules(imports)
        + HEADER
        + "set_option linter.unusedVariables false\nset_option linter.unusedSimpArgs false\n"
        + dispatch_proof.source(route, helper_pc, selector, runtime_size)
    )
    output = success(prove(name, source, timeout, module=True), name)
    check_axioms(output.splitlines(), 2)
    return name


def certify(directory, site, fixture, observed, pc, route, runtime_size, timeout):
    name = site.name
    base = f"Entry{name[0].upper() + name[1:]}"
    prove = partial(lean, directory, namespace=base)
    success(prove(base, fixture, timeout, module=True), name)
    dispatch = certify_dispatch(
        prove, base, route, pc, site.selector, runtime_size, timeout
    )
    layers = (
        ("physical", "ByteCut", "ByteCut", 2048 - len(route.instructions)),
        ("before", "Except String MirCursor", "MirCut", 1024),
        ("after", "Except String MirCursor", "MirCut", 1024),
    )
    memo, imports, count = phase(prove, dispatch, layers, timeout)

    source = (
        import_modules(imports)
        + HEADER
        + f"""
open MIR.TransferFromCallers
theorem bytecode_reaches : runBytes {pc} 2048 tvWorldConfig initial = .ok physicalCut := by
  rw [dispatch_concrete, ←{memo}.hashChecked_eq]
  exact physicalReached

theorem cut_pc : physicalCut.frame.pc = {pc} := by decide +kernel
theorem cut_gas : physicalCut.frame.gas = {observed["gas"]} := by decide +kernel
theorem before_reaches :
    mirPrefix mirCfg beforeFunctions beforeEntry beforeHelper beforeReturned selector caller checkpoint
      physicalCut 1024 = .ok beforeCut := by
  unfold mirPrefix
  rw [←{memo}.hashChecked_eq]
  exact beforeReached
theorem after_reaches :
    mirPrefix mirCfg afterFunctions afterEntry afterHelper afterReturned selector caller checkpoint
      physicalCut 1024 = .ok afterCut := by
  unfold mirPrefix
  rw [←{memo}.hashChecked_eq]
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
    output = success(prove(name, source, timeout, module=True), name)
    check_axioms(output.splitlines(), 7)
    return {
        "segments": count,
        "proof": f"kernel/{name}.lean",
        "dispatch": {
            "proof": f"kernel/{dispatch}.lean",
            "pc": route.pc,
            "steps": len(route.instructions),
            "gas": route.gas,
        },
    }


def certify_exit(directory, prefix, pc, observed, timeout):
    prefix = prefix.rsplit("/", 1)[-1].removesuffix(".lean")
    prove = partial(lean, directory, namespace=prefix.removesuffix("Proof"))
    byte = prefix + "ByteExit"
    source = f"""import {prefix}
{HEADER}
def byteTailStart := byteExitStart physicalCut
def byteTailWith (hash : List Execution.Byte → Word) := chunkedByteExitStep tvWorldConfig code hash
"""
    success(prove(byte, source, timeout, module=True), byte)
    byte_memo, byte_imports, byte_count = phase(
        prove, byte, (("byteTail", "ByteCut", "ByteExit", 1024),), timeout
    )

    mir = prefix + "MirExit"
    source = import_modules(byte_imports) + HEADER
    for label in ("before", "after"):
        source += f"""
def {label}Body : CallerBody := ⟨{label}Functions,{label}Entry,{label}Helper,{label}Returned⟩
def {label}TailStart := mirExitStart mirCfg {label}Functions physicalCut {label}Cut byteTailCut
def {label}TailWith (hash : List Execution.Byte → Word) :=
  mirExitStep mirCfg {label}Functions (physicalCut.budgets ++ byteTailCut.budgets)
    (unpaidHelperCall mirCfg {label}Functions physicalCut {label}Cut) hash
"""
    success(prove(mir, source, timeout, module=True), mir)
    mir_memo, mir_imports, mir_count = phase(
        prove,
        mir,
        tuple(
            (label + "Tail", "Except String MirCursor", "EVM.State.Result", 1024)
            for label in ("before", "after")
        ),
        timeout,
    )

    proof = link_exit(
        prove,
        prefix,
        pc,
        observed,
        byte_memo,
        byte_imports,
        mir_memo,
        mir_imports,
        timeout,
    )
    return {**proof, "segments": byte_count + mir_count}


def link_exit(
    prove, prefix, pc, observed, byte_memo, byte_imports, mir_memo, mir_imports, timeout
):
    byte_facts = prefix + "ByteFacts"
    source = (
        import_modules(byte_imports)
        + HEADER
        + f"""
theorem bytecode_exit : runByteExit tvWorldConfig 1024 physicalCut = .ok byteTailCut := by
  unfold runByteExit
  change Execution.Trace.run (byteExitStep tvWorldConfig code.bytes) 1024 byteTailStart = _
  rw [←chunkedByteExitStep_eq, ←{byte_memo}.hashChecked_eq]
  exact byteTailReached

def physicalResult := finishByte checkpoint byteTailCut.result
def result := EVM.State.observe tvWorldConfig .effects physicalResult
theorem bytecode_gas : physicalResult.gas = {observed[2]} := by decide +kernel
theorem bytecode_refund : physicalResult.world.refund = ({observed[3]}) := by decide +kernel
#print axioms bytecode_gas
#print axioms bytecode_refund
"""
    )
    check_axioms(
        success(
            prove(byte_facts, source, timeout, module=True), byte_facts
        ).splitlines(),
        2,
    )

    name = prefix + "Replay"
    source = f"""import {byte_facts}
{HEADER}
#eval IO.println (EVM.State.describe tvWorldConfig .exact (.ok physicalResult))
"""
    output = success(prove(name, source, timeout), name)
    require(
        json.loads(output) == observed,
        "kernel-checked ABI exit disagrees with Rust replay",
    )

    imports = []
    for label, trace in zip(("before", "after"), mir_imports, strict=True):
        fmp = prefix + label + "Fmp"
        source = f"""import {prefix}
{HEADER}
theorem {label}_fmp :
    Execution.readWord {label}Cut.frame.memory 64 = Execution.readWord physicalCut.frame.memory 64 :=
  by rfl
#print axioms {label}_fmp
"""
        check_axioms(
            success(prove(fmp, source, timeout, module=True), fmp).splitlines(), 1
        )
        link = prefix + label + "Link"
        source = (
            import_modules((trace, byte_facts, fmp))
            + HEADER
            + f"""
theorem {label}_exit :
    runMirExit mirCfg {label}Functions 1024 physicalCut byteTailCut {label}Cut = .ok {label}TailCut := by
  unfold runMirExit
  rw [←{mir_memo}.hashChecked_eq]
  exact {label}TailReached

theorem {label}_effects :
    EVM.State.observe tvWorldConfig .effects (finishByte checkpoint {label}TailCut) = result :=
  by decide +kernel

theorem {label}_pre_sample :
    preSampleExhausted mirCfg {label}Functions physicalCut {label}Cut = true →
      byteTailCut.result.stop = .outOfGas ∧ byteTailCut.samples = [] ∧ byteTailCut.budgets = [] :=
  by decide +kernel

theorem {label}_unpaid_call :
    (unpaidHelperCall mirCfg {label}Functions physicalCut {label}Cut).map Prod.snd =
      byteTailCut.unpaidCall.map Prod.snd :=
  by decide +kernel

theorem {label}_from_abi :
    fromAbi tvWorldConfig mirCfg initial {pc} selector caller {label}Body 2048 1024 1024 false = .ok result :=
  (fromAbi_mir tvWorldConfig mirCfg initial {pc} selector caller {label}Body 2048 1024 1024
    physicalCut byteTailCut {label}Cut {label}TailCut bytecode_reaches bytecode_exit
    {label}_reaches {label}_exit).trans (congrArg Except.ok {label}_effects)
"""
        )
        success(prove(link, source, timeout, module=True), link)
        imports.append(link)

    source = (
        import_modules(imports)
        + HEADER
        + f"""
def invocation : Invocation :=
  ⟨tvWorldConfig,mirCfg,initial,{pc},selector,caller,beforeBody,afterBody,2048,1024,1024⟩

theorem physical_from_abi :
    fromAbi tvWorldConfig mirCfg initial {pc} selector caller beforeBody 2048 1024 1024 true = .ok result :=
  fromAbi_byte tvWorldConfig mirCfg initial {pc} selector caller beforeBody 2048 1024 1024
    physicalCut byteTailCut bytecode_reaches bytecode_exit

theorem abi_entry_to_exit : invocation.Verified :=
  ⟨result,physical_from_abi,before_from_abi,after_from_abi⟩

#print axioms abi_entry_to_exit
"""
    )
    name = prefix + "ToExit"
    check_axioms(
        success(prove(name, source, timeout, module=True), name).splitlines(), 1
    )
    return {"proof": f"kernel/{name}.lean", "price_proof": f"kernel/{byte_facts}.lean"}


def certify_suite(directory, paths, timeout):
    modules = {caller: f"Entry{caller[0].upper() + caller[1:]}" for caller in paths}
    source = (
        import_modules(module + "ProofToExit" for module in modules.values())
        + HEADER
        + "open SolarTV.MIR.TransferFromCallers\ndef invocation : Caller → Invocation\n"
        + "".join(
            f"  | .{caller} => {module}.invocation\n"
            for caller, module in modules.items()
        )
        + "\ntheorem all_callers (caller : Caller) : (invocation caller).Verified := by\n"
        + "  cases caller with\n"
        + "".join(
            f"  | {caller} => exact {module}.abi_entry_to_exit\n"
            for caller, module in modules.items()
        )
        + "\n#print axioms all_callers\n"
    )
    name = "MorphoAbiSuite"
    output = success(lean(directory, name, source, timeout), name)
    check_axioms(output.splitlines(), 1)
    return f"kernel/{name}.lean"
