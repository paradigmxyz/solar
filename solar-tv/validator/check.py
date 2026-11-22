"""Check a concrete pair with the pinned Lean kernel and replay candidate witnesses."""

import argparse
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
from functools import partial
from pathlib import Path

from . import (
    audit,
    correspondence,
    evm_region,
    evmir,
    gas,
    memory,
    mir,
    mir_symbolic,
    native,
    operations,
    prefix,
    regions,
    report,
    world,
)
from .common import Inconclusive, Program, Unsupported

ROOT = Path(__file__).resolve().parents[1]
PRELUDE = "import SolarTV\nopen SolarTV EvmRules\nset_option autoImplicit false\nset_option linter.unusedSimpArgs false\nset_option linter.unusedVariables false\nset_option maxRecDepth 4096\nset_option exponentiation.threshold 1024\n"
UNFOLD = (
    "before, after, MIR.eval, MIR.choose, MIR.phi, MIR.cast, MIR.compare, MIR.branch, MIR.guard, "
    "Outcome.bind_return, "
    "EVM.execute, EVM.finish, EVM.stepChecked, EVM.step, Evm.eq, Evm.iszero, Evm.not, "
    "Evm.sub, Evm.add, Evm.mul, Evm.and, Evm.or, Evm.xor, MIR.unary, MIR.shift, "
    "MIR.select, MIR.wordBinary, MIR.ternary, Word.binary, Word.unary, Word.shift, "
    "Evm.div, Evm.sdiv, Evm.mod, Evm.smod, Evm.exp, Evm.addmod, Evm.mulmod, "
    "Evm.lt, Evm.gt, Evm.slt, Evm.sgt, Evm.shl, Evm.shr, Evm.sar, Evm.byte, Evm.signextend, Evm.clz"
)


def run(
    args: list, cwd: Path, timeout: int, *, env: dict | None = None
) -> subprocess.CompletedProcess:
    # Lake spawns Lean. A timeout must reap the entire invocation, not leave the
    # prover consuming CPU after the driver has reported an inconclusive result.
    with subprocess.Popen(
        args,
        cwd=cwd,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        start_new_session=os.name != "nt",
    ) as process:
        try:
            stdout, stderr = process.communicate(timeout=timeout)
        except (subprocess.TimeoutExpired, KeyboardInterrupt) as error:
            if os.name == "nt":
                subprocess.run(
                    ["taskkill", "/F", "/T", "/PID", str(process.pid)],
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    check=False,
                    timeout=10,
                )
            else:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            stdout, stderr = process.communicate()
            if isinstance(error, subprocess.TimeoutExpired):
                raise subprocess.TimeoutExpired(
                    args, timeout, output=stdout, stderr=stderr
                ) from error
            raise
        return subprocess.CompletedProcess(args, process.returncode, stdout, stderr)


def load(
    kind: str,
    path: Path,
    solar: str,
    timeout: int,
    *,
    gas_mode: str | None = None,
    evm_version: str | None = None,
    function: str | None = None,
    window: str | None = None,
    region: str | None = None,
    stack_height: int | None = None,
    world_path: Path | None = None,
    replay_executable: str | None = None,
) -> Program:
    if gas_mode is not None and kind != "evmir" and world_path is None:
        raise Unsupported("gas auditing requires scheduled EVM IR, not MIR")
    suffix = ".mir" if kind == "mir" else ".evmir"
    limit = (
        1024 * 1024
        if function is not None or window is not None or region is not None
        else 128 * 1024
    )
    if path.suffix != suffix or path.stat().st_size > limit:
        raise Unsupported(f"expected a {suffix} file of at most {limit // 1024} KiB")
    pipeline = "mir" if kind == "mir" else "evm-ir"
    parsed = run(
        [
            solar,
            path.resolve(),
            f"-Z{pipeline}-pipeline=none",
            "-Zvalidate-ir=true",
            *([f"--evm-version={evm_version}"] if evm_version is not None else []),
        ],
        ROOT,
        timeout,
    )
    if parsed.returncode:
        raise Unsupported(parsed.stderr.strip() or "compiler rejected input")
    if world_path is not None:
        if window is not None or region is not None or stack_height is not None:
            raise Unsupported(
                "world mode requires a complete entry, not a block projection"
            )
        fixture = world.read(world_path, solar, gas_mode, kind=kind)
        return world.translate(
            parsed.stdout,
            kind,
            solar,
            evm_version or "cancun",
            fixture,
            timeout,
            function,
            replay_executable,
        )
    projection = parsed.stdout
    if region is not None:
        if kind != "evmir":
            raise Unsupported("--region requires EVM IR")
        if gas_mode is not None:
            raise Unsupported(
                "gas-priced EVM regions require physical transfer pricing"
            )
        try:
            program = evm_region.translate(projection, region, stack_height)
        except Unsupported as error:
            if str(error).startswith("unmodeled EVM IR instruction: "):
                raise evm_region.UnmodeledInstruction(str(error)) from None
            if (
                stack_height is None
                or str(error) != "cyclic EVM regions require loop invariants"
            ):
                raise
            program = evm_region.cyclic(projection, region, stack_height)
        program.evm_version = evm_version or "cancun"
        # The source witness belongs to the compiler parse, not to a field an
        # importer can accidentally replace along with its generated tree.
        program.replay_ir = parsed.stdout
        program.selector = region
        correspondence.certify(
            program,
            replay_executable or native.default(solar),
            timeout,
            entry_label=region,
            stack_height=stack_height,
        )
        return program
    following = None
    if function is not None:
        if kind != "mir":
            raise Unsupported("--function requires MIR")
        projection = regions.function(projection, function)
    if window is not None:
        if kind != "evmir" or stack_height is None:
            raise Unsupported("--window requires EVM IR and --stack-height")
        projection, following = regions.window(projection, window, stack_height)
        with tempfile.TemporaryDirectory(prefix="solar-tv-window-") as directory:
            selected = Path(directory) / "window.evmir"
            selected.write_text(projection)
            arguments = list(parsed.args)
            arguments[1] = selected
            checked = run(arguments, ROOT, timeout)
            if checked.returncode:
                raise Unsupported(
                    "compiler rejected window projection\n" + checked.stderr
                )
            projection = checked.stdout
    if len(projection.encode()) > 128 * 1024:
        raise Unsupported("selected region exceeds 128 KiB")
    if kind == "mir":
        # The direct opcode model and independent replay must use the same fork.
        # Obtain availability from the opcode schema, rather than maintaining a
        # second list of target-dependent operation classifications here.
        catalog = native.catalog(native.default(solar), timeout)
        definitions = {op["name"]: op for op in catalog["evmir"]}
        for name in re.findall(r"\bv\d+ = ([a-z_]+)\b", projection):
            if name in operations.UNARY | operations.BINARY | operations.TERNARY:
                fork = definitions[name]["forks"].get(evm_version or "cancun")
                if fork is None or not fork["available"]:
                    raise Unsupported(
                        f"MIR opcode `{name}` is unavailable on {evm_version or 'cancun'}"
                    )
    if kind == "mir":
        try:
            program = mir.translate(projection)
        except Unsupported:
            names = re.findall(r"^fn @([^ (]+)\(", projection, re.MULTILINE)
            if len(names) != 1:
                raise Unsupported("select one MIR function") from None
            return mir_symbolic.translate(
                parsed.stdout,
                function or names[0],
                replay_executable or native.default(solar),
                evm_version or "cancun",
                timeout,
            )
    else:
        program = evmir.translate(
            projection,
            gas=gas_mode,
            evm_version=evm_version or "cancun",
            entry=window or "bb0",
            following=following,
            scoped=window is not None,
        )

    program.canonical_ir = projection
    if function is not None or window is not None:
        program.replay_ir = parsed.stdout
        program.selector = function or window
        program.scope = "function" if function is not None else "window"
        program.following = following
    program.evm_version = evm_version or "cancun"
    if kind == "mir":
        names = re.findall(r"^fn @([^ (]+)\(", projection, re.MULTILINE)
        if names != [program.interface[0]]:
            raise Unsupported("MIR import changed the selected function identity")
        correspondence.certify_scalar_mir(
            program, solar, replay_executable or native.default(solar), timeout
        )
    return program


def definition(name: str, program: Program) -> str:
    widths = program.input_widths or (256,) * len(program.inputs)
    args = " ".join(
        f"({arg} : BitVec {width})"
        for arg, width in zip(program.inputs, widths, strict=True)
    )
    body = "\n".join("  " + line for line in program.body.splitlines())
    declaration = (
        program.import_certificate
        + f"def {name} {args} : {program.result_type} :=\n{body}\n"
        + program.import_obligation
    )
    return declaration.replace("__IMPORT__", name)


def build_model(timeout: int, module: str = "SolarTV") -> subprocess.CompletedProcess:
    # Multiple compiler workers can enter on a cold Lake cache. Only the build
    # needs exclusion; independent pair proofs can run in parallel afterward.
    if os.name == "nt":
        import msvcrt

        cache = ROOT / ".lake"
        cache.mkdir(exist_ok=True)
        with (cache / "solar-tv.lock").open("a+b") as lock:
            lock.seek(0)
            msvcrt.locking(lock.fileno(), msvcrt.LK_LOCK, 1)
            return run(["lake", "build", module], ROOT, timeout)

    import fcntl

    # Share the dependency's build lock with the existing EVM rule prover.
    with (ROOT.parent / "scripts/evm-rules/lean/lakefile.toml").open("rb") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        return run(["lake", "build", module], ROOT, timeout)


def check(
    a: Program,
    b: Program,
    timeout: int,
    *,
    replay_command: str,
    replay_executable: str,
    verbose: bool = False,
) -> audit.Result:
    if a.scope == "universal-region" or b.scope == "universal-region":
        return evm_region.check_universal(a, b, timeout)
    if a.symbolic_mir is not None or b.symbolic_mir is not None:
        return mir_symbolic.check(
            a,
            b,
            timeout,
            replay_command=replay_command,
            replay_executable=replay_executable,
            verbose=verbose,
        )
    if a.scope == "cyclic-region" or b.scope == "cyclic-region":
        return correspondence.check_cyclic(a, b, timeout)
    if a.world is not None or b.world is not None:
        from .world_check import check as check_world

        return check_world(
            a,
            b,
            timeout,
            replay_command=replay_command,
            replay_executable=replay_executable,
            verbose=verbose,
        )
    if a.scope == "window" and b.scope == "window" and a.selector is not None:
        regions.compatible_windows(a.replay_ir, b.replay_ir, a.selector)
    if a.memory != b.memory:
        translate = evmir.translate if a.interface[0] == "evm-window" else mir.translate
        for program in (a, b):
            if not program.memory:
                options = (
                    {
                        "entry": program.selector or "bb0",
                        "following": program.following,
                        "scoped": program.scope == "window",
                    }
                    if translate is evmir.translate
                    else {}
                )
                enriched = translate(program.canonical_ir, memory=True, **options)
                enriched.canonical_ir = program.canonical_ir
                enriched.replay_ir = program.replay_ir
                enriched.selector = program.selector
                enriched.scope = program.scope
                enriched.following = program.following
                enriched.evm_version = program.evm_version
                if program is a:
                    a = enriched
                else:
                    b = enriched
    if a.interface != b.interface:
        raise Unsupported("input interfaces differ")
    build = build_model(timeout)
    if build.returncode:
        raise Unsupported(build.stderr or build.stdout)
    definitions = PRELUDE + definition("before", a) + definition("after", b)
    args = " ".join(a.inputs)
    widths = a.input_widths or (256,) * len(a.inputs)
    binders = " ".join(
        f"({arg} : BitVec {width})" for arg, width in zip(a.inputs, widths, strict=True)
    )

    def relation(lhs: str, rhs: str) -> str:
        if a.gas == "refine":
            return f"EVM.Gas.refines ({lhs}) ({rhs})"
        if a.result_type.startswith("Outcome "):
            return f"refines ({lhs}) ({rhs})"
        return f"({lhs}) = ({rhs})"

    statement = f"theorem pair_correct {binders} : {relation(f'before {args}', f'after {args}')} := by\n"
    unfold = UNFOLD + (", " + memory.UNFOLD if a.memory else "")
    if a.scope == "region":
        # Normalize transfers and stack state before byte memory. Shared tails
        # often leave identical memory expressions; unfolding every symbolic
        # byte before comparing them needlessly exhausts the simplifier.
        unfold = (
            UNFOLD
            + ", "
            + evm_region.UNFOLD
            + ", EVM.Memory.step, EVM.Memory.finish, Except.map, Except.bind, Bind.bind, Pure.pure, Functor.map"
        )
    if a.gas is not None:
        unfold += ", " + gas.UNFOLD
    projection = (
        "EVM.Gas.code"
        if a.gas is not None
        else "Memory.code"
        if a.memory and a.result_type.startswith("Outcome ")
        else "EVM.Memory.code"
        if a.memory
        else "Outcome.code"
        if a.result_type.startswith("Outcome ")
        else "EVM.code"
    )
    # Simplify dead definitions first. Splitting branches and reasoning about
    # unsigned bounds with omega produces kernel proofs for guarded cleanups.
    # bv_decide can find witnesses; its native proof axioms remain rejected below.
    simplify = (
        f"simp [refines_iff_eq, {unfold}, BitVec.add_comm, BitVec.sub_eq_iff_eq_add]"
        if a.result_type in {"EVM.Execution", "EVM.Memory.Execution", "EVM.Gas.Result"}
        else f"simp only [refines_iff_eq, {unfold}]"
    )
    proof = (
        f"  {simplify}\n"
        "  repeat (first | intro _ | split)\n"
        "  all_goals simp_all [MIR.checked, Outcome.bind, BitVec.uaddOverflow, "
        "BitVec.usubOverflow, BitVec.umulOverflow, BitVec.saddOverflow, "
        "BitVec.ssubOverflow, BitVec.smulOverflow, "
        "BitVec.toNat_eq, BitVec.ult_iff_toNat_lt, BitVec.add_comm] <;> omega\n"
    )
    if a.gas is not None:
        proof = (
            f"  {simplify}\n"
            "  repeat' (first | omega | (split <;> simp_all [BitVec.add_comm]))\n"
            "  all_goals omega\n"
        )
    if a.scope == "region":
        # Transfer cleanup often changes only grouping. Kernel conversion can
        # compare the unchanged memory expressions without expanding their bytes.
        proof = (
            "  first\n    | rfl\n    |\n"
            + "\n".join("    " + line for line in proof.splitlines())
            + "\n"
        )
    # Kernel reduction is cheap for closed word arithmetic and explicit stack
    # windows. Do not normalize symbolic memory to individual bits first: that
    # can exhaust the budget before the overwrite/expansion lemmas get a turn.
    if not a.memory:
        proof = (
            "  first\n"
            "    | (simp only [refines_iff_eq, before, after]; decide +kernel)\n"
            f"    | ({simplify}; all_goals decide +kernel)\n"
            "    | rfl\n"
            "    | (simp only [refines_iff_eq]; rfl)\n"
            "    |\n" + "\n".join("    " + line for line in proof.splitlines()) + "\n"
        )
    with tempfile.TemporaryDirectory() as directory:
        file = Path(directory) / "Pair.lean"

        def lean(body: str) -> subprocess.CompletedProcess:
            file.write_text(definitions + body)
            return run(["lake", "env", "lean", file], ROOT, timeout)

        proposed = False
        if a.scope == "region":
            # A closed VM sample only proposes a witness. Avoid spending the
            # universal proof budget reducing unequal symbolic byte lists when
            # zero already distinguishes the regions. The kernel and original
            # native modules must still confirm that exact counterexample below.
            concrete = " ".join(f"0#{width}" for width in widths)
            probe = lean(
                f'#eval IO.println (if decide ({relation("before " + concrete, "after " + concrete)}) then "same" else "different")\n'
            )
            if probe.returncode:
                raise Inconclusive(
                    "import correspondence certificate failed\n"
                    + probe.stdout
                    + probe.stderr
                )
            if probe.stdout.strip() not in {"same", "different"}:
                raise Inconclusive(
                    "invalid region witness probe\n" + probe.stdout + probe.stderr
                )
            proposed = probe.stdout.strip() == "different"
        result = (
            probe
            if proposed
            else lean(statement + proof + "#print axioms pair_correct\n")
        )
        if result.returncode and (a.import_obligation or b.import_obligation):
            errors = re.finditer(
                rf"(?m)^{re.escape(str(file))}:(\d+):\d+: error:",
                result.stdout + result.stderr,
            )
            if any(int(error[1]) <= definitions.count("\n") for error in errors):
                raise Inconclusive(
                    "import correspondence certificate failed\n"
                    + result.stdout
                    + result.stderr
                )
        proof_log = (
            "VM proposes a mismatch; kernel witness required"
            if proposed
            else result.stdout + result.stderr
        )
        query_only = proposed
        if result.returncode and not proposed:
            # Outcomes with payloads are outside the solver's scalar fragment.
            # A tagged projection proposes witnesses; it never verifies a pair.
            query_only = a.result_type.startswith("Outcome ") or a.result_type in {
                "EVM.Execution",
                "EVM.Memory.Execution",
                "EVM.Gas.Result",
            }
            query = (
                f"example {binders} : {projection} (before {args}) = "
                f"{projection} (after {args}) := by\n"
                if query_only
                else statement
            )
            query_unfold = f"{unfold}, MIR.checked, EVM.code"
            if a.memory:
                query_unfold += f", {projection}, Memory.snapshot"
            result = lean(
                query
                + f"  simp [{query_unfold}] <;> bv_decide\n"
                + ("" if query_only else "#print axioms pair_correct\n")
            )
        if result.returncode == 0 and not query_only:
            # Reject any proof that introduces an axiom outside Lean's standard
            # foundations, including sorryAx or an unchecked native computation.
            log = result.stdout
            axioms = re.search(
                r"^'pair_correct' depends on axioms: \[(.*?)\]$", log, re.MULTILINE
            )
            if "'pair_correct' does not depend on any axioms" not in log and (
                not axioms
                or set(axioms[1].split(", "))
                - {"propext", "Quot.sound", "Classical.choice"}
            ):
                raise Unsupported("unexpected proof axioms\n" + log)
            return audit.Result("verified", "verified\n", program=a, pair=(a, b))
        log = result.stdout + result.stderr
        # Lean may simplify a universally false goal before bit-blasting it.
        # Try zero for unconstrained words. Report a mismatch only if the
        # kernel checks the negated equality at that concrete assignment.
        model = {
            name: (int(value), int(width))
            for name, value, width in re.findall(
                r"^(\w+) = (\d+)#(\d+)$", log, re.MULTILINE
            )
        }
        vals = []
        for arg, width in zip(a.inputs, widths, strict=True):
            value, actual_width = model.get(arg, (0, width))
            if actual_width != width or value >= 2**width:
                raise Unsupported("invalid counterexample input width")
            vals.append(value)

        def display(term: str) -> str:
            if a.gas is not None:
                return f"#eval IO.println (EVM.Gas.describe ({term}))"
            if a.memory:
                describe = (
                    "Memory.describeReturn"
                    if a.result_type.startswith("Outcome ")
                    else "EVM.Memory.describe"
                )
                return f"#eval IO.println ({describe} ({term}))"
            if a.result_type.startswith("Outcome "):
                return f"#eval IO.println (Outcome.describe ({term}))"
            if a.result_type == "Word" or a.result_type.startswith("BitVec "):
                return f"#eval IO.println (({term}).toNat)"
            return f"#eval IO.println (EVM.describe ({term}))"

        replay_proof = (
            "simp only [EVM.Gas.refines]; decide +kernel"
            if a.gas == "refine"
            else "simp only [refines_iff_eq]; decide +kernel"
            if a.result_type.startswith("Outcome ")
            else "decide +kernel"
        )
        # Prefer a small witness when zero really demonstrates the mismatch.
        # Both it and the solver assignment must pass the same kernel replay.
        zero = [0] * len(vals)
        assignments = [zero] if vals == zero else [zero, vals]
        if a.memory:
            assignments.extend(memory.witnesses(a.inputs))
        if a.gas is not None:
            assignments.extend(gas.witnesses(a.inputs, a.gas_cost, b.gas_cost))
        for vals in assignments:
            inputs = " ".join(
                f"{value}#{width}" for value, width in zip(vals, widths, strict=True)
            )
            lhs, rhs = f"before {inputs}", f"after {inputs}"
            replay = lean(
                f"example : ¬ ({relation(lhs, rhs)}) := by {replay_proof}\n"
                f"{display(lhs)}\n{display(rhs)}\n"
            )
            if replay.returncode == 0:
                break
        else:
            raise Inconclusive(
                "Lean could neither prove equivalence nor replay a counterexample\n"
                + proof_log
                + "\nWitness query diagnostics\n"
                + log
                + "\nConcrete replay diagnostics\n"
                + replay.stdout
                + replay.stderr
            )
        outputs = [line for line in replay.stdout.splitlines() if line.strip()]
        if a.scope == "window":
            # Zero is a real stable label in full modules. Only the standalone pilot
            # window uses it for an abstract fallthrough edge in Lean's formatter.
            outputs = [
                "bb0" + line[len("fallthrough") :]
                if line.startswith("fallthrough,")
                else line
                for line in outputs
            ]
        observed = native.replay(
            a, b, vals, executable=replay_executable, timeout=timeout
        )
        if observed["observations"] != outputs[-2:]:
            raise Inconclusive(
                "Lean and Rust interpreter replay disagree\n"
                + f"Lean: {outputs[-2:]}\nRust: {observed['observations']}"
            )
        return audit.Result(
            "counterexample",
            report.counterexample(
                a,
                b,
                vals,
                outputs[-2],
                outputs[-1],
                replay_command,
                verbose=verbose,
                native_checked=True,
            ),
            program=a,
            pair=(a, b),
            witness={
                "inputs": {
                    name: str(value) for name, value in zip(a.inputs, vals, strict=True)
                },
                "lean": outputs[-2:],
                "rust": observed["observations"],
            },
        )


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "transaction":
        from . import transaction

        return transaction.main(sys.argv[2:])
    if len(sys.argv) > 1 and sys.argv[1] == "heap-entry":
        from . import heap_entry

        return heap_entry.main(sys.argv[2:])
    if len(sys.argv) > 1 and sys.argv[1] == "call-budget":
        from . import call_budget

        return call_budget.main(sys.argv[2:])
    if len(sys.argv) > 1 and sys.argv[1] == "summarize":
        return audit.summarize(sys.argv[2:])
    parser = argparse.ArgumentParser(
        description="Validate a pair of MIR or EVM IR programs with Lean."
    )
    parser.add_argument("ir", choices=["mir", "evmir"])
    parser.add_argument("before", type=Path)
    parser.add_argument("after", type=Path)
    default = ROOT.parent / "target/debug/solar"
    parser.add_argument(
        "--solar",
        default=os.getenv("SOLAR") or (str(default) if default.exists() else "solar"),
    )
    parser.add_argument(
        "--timeout",
        type=int,
        default=60,
        help="seconds per compiler or Lean invocation",
    )
    parser.add_argument(
        "--verbose",
        action="store_true",
        help="show full IR diffs and prover diagnostics",
    )
    parser.add_argument(
        "--gas",
        choices=["exact", "refine"],
        default=os.getenv("SOLAR_TV_GAS"),
        help="audit exact gas behavior or resource refinement (EVM IR only)",
    )
    parser.add_argument(
        "--evm-version",
        help="target version for parsing and gas auditing (gas default: Cancun)",
    )
    parser.add_argument(
        "--replay", help="Rust interpreter helper (default: next to --solar)"
    )
    parser.add_argument(
        "--report", type=Path, help="write a versioned JSON audit record"
    )
    parser.add_argument(
        "--debug-reverts",
        action="store_true",
        help="validate dispatch with compiler-generated Error(string) payloads",
    )
    selection = parser.add_mutually_exclusive_group()
    selection.add_argument(
        "--function", help="check one function in the full MIR module"
    )
    selection.add_argument(
        "--storage-loop", help="check a supported MIR storage search-to-shift region"
    )
    selection.add_argument(
        "--window", help="check one complete EVM block by stable label"
    )
    selection.add_argument(
        "--region", help="check a complete acyclic EVM CFG from a stable entry label"
    )
    selection.add_argument(
        "--prefix", help="prove a block prefix for every incoming stack"
    )
    selection.add_argument(
        "--return-path",
        metavar="SELECTOR",
        help="check a relocated EVM ABI return path",
    )
    selection.add_argument(
        "--mint-layout",
        dest="erc20_layout",
        action="store_const",
        const="mint",
        help="prove gas refinement of the ERC20Mock mint layout",
    )
    selection.add_argument(
        "--burn-layout",
        dest="erc20_layout",
        action="store_const",
        const="burn",
        help="prove gas refinement of the ERC20Mock burn layout",
    )
    selection.add_argument(
        "--prune",
        action="store_true",
        help="prove a structural MIR module change (pruning, phase advance, or empty dispatch)",
    )
    parser.add_argument(
        "--stack-height",
        type=int,
        help="fixed incoming EVM height (regions otherwise prove all stacks)",
    )
    parser.add_argument(
        "--world",
        type=Path,
        help="explicit initial-world fixture for complete-entry stateful checks",
    )
    args = parser.parse_args()
    version = args.evm_version or "cancun"
    replay = report.replay_command(
        args.ir,
        args.before,
        args.after,
        args.solar,
        args.timeout,
        gas=args.gas,
        evm_version=version,
        replay=args.replay,
        function=args.function,
        storage_loop=args.storage_loop,
        window=args.window,
        region=args.region,
        stack_height=args.stack_height,
        world=args.world,
        prefix=args.prefix,
        return_path=args.return_path,
        erc20_layout=args.erc20_layout,
        prune=args.prune,
        debug_reverts=args.debug_reverts,
    )
    programs: list[Program] = []
    model = audit.semantics()
    try:
        gas.validate(args.gas, version or "cancun")
        if args.debug_reverts and not args.prune:
            raise Unsupported("--debug-reverts requires --prune")
        solar = shutil.which(args.solar)
        if not solar or args.timeout <= 0:
            raise Unsupported(
                "supply a built compiler with --solar and a positive timeout"
            )
        solar = str(Path(solar).resolve())
        rust_replay = args.replay or native.default(solar)
        replay = report.replay_command(
            args.ir,
            args.before,
            args.after,
            solar,
            args.timeout,
            gas=args.gas,
            evm_version=version,
            replay=rust_replay,
            function=args.function,
            storage_loop=args.storage_loop,
            window=args.window,
            region=args.region,
            stack_height=args.stack_height,
            world=args.world,
            prefix=args.prefix,
            return_path=args.return_path,
            erc20_layout=args.erc20_layout,
            prune=args.prune,
            debug_reverts=args.debug_reverts,
        )
        if args.prune:
            if (
                args.ir != "mir"
                or args.world
                or args.gas
                or args.stack_height is not None
            ):
                raise Unsupported(
                    "--prune requires unpriced MIR without a world fixture or stack height"
                )
            from . import prune

            result = prune.check(
                (args.before, args.after),
                solar,
                version,
                args.timeout,
                executable=rust_replay,
                debug_reverts=args.debug_reverts,
            )
            programs = list(result.pair)
        elif args.storage_loop:
            if (
                args.ir != "mir"
                or args.world
                or args.gas
                or args.stack_height is not None
            ):
                raise Unsupported(
                    "--storage-loop requires unpriced MIR without a world fixture or stack height"
                )
            from . import storage_induction

            result = storage_induction.check(
                (args.before, args.after),
                solar,
                rust_replay,
                version,
                args.timeout,
                args.storage_loop,
                replay=replay,
                verbose=args.verbose,
            )
            programs = list(result.pair)
        elif args.erc20_layout:
            if (
                args.ir != "evmir"
                or not args.world
                or args.gas not in (None, "refine")
                or args.stack_height is not None
            ):
                raise Unsupported(
                    f"--{args.erc20_layout}-layout requires EVM IR and a concrete replay world, with gas refinement"
                )
            from . import erc20_layout

            result = erc20_layout.check(
                (args.before, args.after),
                args.world,
                solar,
                rust_replay,
                version,
                args.timeout,
                erc20_layout.ENTRIES[args.erc20_layout],
            )
            programs = list(result.pair)
        elif args.return_path:
            if (
                args.ir != "evmir"
                or not args.world
                or args.gas
                or args.stack_height is not None
            ):
                raise Unsupported(
                    "--return-path requires EVM IR and a concrete world without a gas comparison or fixed height"
                )
            from . import relocated_return

            result = relocated_return.check(
                (args.before, args.after),
                solar,
                rust_replay,
                version,
                args.timeout,
                args.return_path,
                args.world,
                replay,
            )
            programs = list(result.pair)
        elif args.prefix:
            if args.ir != "evmir" or args.world or args.stack_height is not None:
                raise Unsupported(
                    "--prefix requires EVM IR without a world fixture or fixed height"
                )
            result = prefix.check(
                (args.before, args.after),
                solar,
                rust_replay,
                args.timeout,
                args.prefix,
                version,
                args.gas,
            )
            programs = list(result.pair)
        else:
            try:
                programs = [
                    load(
                        args.ir,
                        path,
                        solar,
                        args.timeout,
                        gas_mode=args.gas,
                        evm_version=version,
                        function=args.function,
                        window=args.window,
                        region=args.region,
                        stack_height=args.stack_height,
                        world_path=args.world,
                        replay_executable=rust_replay,
                    )
                    for path in (args.before, args.after)
                ]
            except Unsupported as error:
                if (
                    args.ir == "evmir"
                    and args.region
                    and not args.world
                    and not args.gas
                    and (
                        isinstance(error, evm_region.UnmodeledInstruction)
                        or str(error)
                        in {
                            "region entry stack cannot execute its instructions",
                            "cyclic EVM regions require loop invariants",
                            "region expansion budget exceeded",
                        }
                    )
                ):
                    from . import source_identity

                    result = None
                    if (
                        args.stack_height is not None
                        and str(error).removeprefix("unmodeled EVM IR instruction: ")
                        in evmir.CONTEXT_READS
                    ):
                        # A fixed scalar entry lacks these frame inputs. Retry
                        # with the stronger all-stacks, arbitrary-context proof.
                        try:
                            contextual = [
                                load(
                                    "evmir",
                                    path,
                                    solar,
                                    args.timeout,
                                    evm_version=version,
                                    region=args.region,
                                    replay_executable=rust_replay,
                                )
                                for path in (args.before, args.after)
                            ]
                            result = evm_region.check_universal(
                                *contextual, args.timeout
                            )
                        except Unsupported:
                            pass
                    if result is None:
                        # Equal resolved graphs still admit structural identity.
                        try:
                            result = source_identity.check(
                                (args.before, args.after),
                                solar,
                                rust_replay,
                                version,
                                args.timeout,
                                args.region,
                            )
                        except Unsupported:
                            raise error from None
                    programs = list(result.pair)
                elif (
                    args.ir == "mir"
                    and args.function
                    and not args.world
                    and not args.gas
                    and str(error)
                    in {
                        "symbolic MIR cycles require an invariant",
                        "MIR source: GAS requires an isolated opcode window",
                    }
                ):
                    from . import (
                        bounded_fill,
                        builtin_lowering,
                        coalesce,
                        dead_ssa,
                        induction,
                        memory_induction,
                        mir_cfg,
                    )

                    validators = (
                        (
                            coalesce.check,
                            dead_ssa.check,
                            mir_cfg.check,
                            partial(
                                induction.check, replay=replay, verbose=args.verbose
                            ),
                            partial(
                                memory_induction.check,
                                replay=replay,
                                verbose=args.verbose,
                            ),
                            partial(
                                bounded_fill.check, replay=replay, verbose=args.verbose
                            ),
                            builtin_lowering.check,
                        )
                        if str(error) == "symbolic MIR cycles require an invariant"
                        else (mir_cfg.check, builtin_lowering.check)
                    )
                    for validate in validators:
                        try:
                            result = validate(
                                (args.before, args.after),
                                solar,
                                rust_replay,
                                version,
                                args.timeout,
                                args.function,
                            )
                        except Unsupported:
                            continue
                        break
                    else:
                        raise error from None
                    programs = list(result.pair)
                # Scalar windows lack caller/calldata context and dynamic-return
                # correspondence. Try a universal contextual certificate rather
                # than supplying defaults or replacing an actual counterexample.
                elif (
                    args.ir != "evmir"
                    or not args.window
                    or args.world
                    or str(error)
                    not in {
                        "unmodeled EVM IR instruction: caller",
                        "unmodeled EVM IR instruction: callvalue",
                        "unmodeled EVM IR instruction: calldataload",
                        "unmodeled EVM IR instruction: gas",
                        "unmodeled EVM IR instruction: staticcall",
                        "GAS requires --gas=exact or --gas=refine",
                        "expected a valid static JUMP destination",
                    }
                ):
                    raise
                else:
                    result = prefix.check(
                        (args.before, args.after),
                        solar,
                        rust_replay,
                        args.timeout,
                        args.window,
                        version,
                        args.gas,
                    )
                    programs = list(result.pair)
            else:
                result = check(
                    programs[0],
                    programs[1],
                    args.timeout,
                    replay_command=replay,
                    replay_executable=rust_replay,
                    verbose=args.verbose,
                )
    except (Unsupported, OSError, subprocess.TimeoutExpired) as error:
        status = (
            "timeout"
            if isinstance(error, subprocess.TimeoutExpired)
            else "inconclusive"
            if isinstance(error, Inconclusive)
            else "unsupported"
            if isinstance(error, Unsupported)
            else "error"
        )
        result = audit.Result(
            status,
            stderr=report.inconclusive(error, replay, verbose=args.verbose),
            program=programs[0] if len(programs) == 2 else None,
            pair=(programs[0], programs[1]) if len(programs) == 2 else None,
        )
    try:
        if audit.semantics() != model:
            result = audit.Result(
                "inconclusive",
                stderr=report.inconclusive(
                    Inconclusive("semantics changed during this check"),
                    replay,
                    verbose=args.verbose,
                ),
                program=result.program,
                pair=result.pair,
            )
        record = audit.record(
            result,
            layer=args.ir,
            target=version,
            policy=args.gas,
            before=args.before,
            after=args.after,
            model=model,
        )
        if args.storage_loop:
            record["whole_pass"] = False
            record["region"] = {
                **record.get("region", {}),
                "kind": "loop-region",
                "name": args.storage_loop,
            }
        if args.function or args.window or args.region or args.prefix:
            record["region"] = {
                **record.get("region", {}),
                "kind": "function"
                if args.function
                else "region"
                if args.region
                else "window",
                "name": args.function or args.window or args.region or args.prefix,
                "stack_height": args.stack_height,
            }
            record["whole_pass"] = False
            if (
                args.prefix
                or result.program
                and result.program.scope
                in {
                    "prefix-window",
                    "effectful-prefix",
                    "shared-prefix-suffix",
                    "interior-window",
                }
            ):
                shared = result.program and result.program.import_evidence.get(
                    "shared_prologue_operations"
                )
                record["region"]["proof"] = (
                    "universal-post-precompile-suffix"
                    if result.program and result.program.scope == "shared-prefix-suffix"
                    else "universal-branching-effect-prefix"
                    if result.program
                    and result.program.scope == "effectful-prefix"
                    and result.program.import_evidence.get("conditional_transfers")
                    else "universal-effectful-stack-prefix"
                    if result.program and result.program.scope == "effectful-prefix"
                    else "universal-memory-prefix"
                    if shared
                    else "universal-exact-scalar-island"
                    if result.program and result.program.scope == "interior-window"
                    else "universal-stack-prefix"
                )
                if args.stack_height is not None:
                    record["region"]["requested_stack_height"] = args.stack_height
                record["region"]["stack_height"] = None
            if args.region and len(programs) == 2:
                record["region"]["blocks_before"] = list(
                    regions.blocks(programs[0].canonical_ir)
                )
                record["region"]["blocks_after"] = list(
                    regions.blocks(programs[1].canonical_ir)
                )
                if result.program and result.program.scope in {
                    "source-identity",
                    "stack-layout",
                }:
                    record["region"]["proof"] = (
                        "kernel-checked-schema-stack-layout"
                        if result.program.scope == "stack-layout"
                        else "universal-source-graph-identity"
                    )
                    record["region"]["stack_height"] = None
                    if args.stack_height is not None:
                        record["region"]["requested_stack_height"] = args.stack_height
        if args.report:
            audit.write(args.report, record)
    except OSError as error:
        print(f"cannot write audit report: {error}", file=sys.stderr)
        return 2
    print(result.stdout, end="")
    print(result.stderr + audit.context(record), file=sys.stderr, end="")
    return result.exit_code
