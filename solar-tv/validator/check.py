"""Check a concrete pair with the pinned Lean kernel and replay candidate witnesses."""

import argparse
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from . import audit, evmir, gas, memory, mir, native, operations, regions, report, world
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


def run(args: list, cwd: Path, timeout: int) -> subprocess.CompletedProcess:
    return subprocess.run(
        args, check=False, cwd=cwd, text=True, capture_output=True, timeout=timeout
    )


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
    stack_height: int | None = None,
    world_path: Path | None = None,
    replay_executable: str | None = None,
) -> Program:
    if gas_mode is not None and kind != "evmir" and world_path is None:
        raise Unsupported("gas auditing requires scheduled EVM IR, not MIR")
    suffix = ".mir" if kind == "mir" else ".evmir"
    limit = 1024 * 1024 if function is not None or window is not None else 128 * 1024
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
        if window is not None or stack_height is not None:
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
    program = (
        mir.translate(projection)
        if kind == "mir"
        else evmir.translate(
            projection,
            gas=gas_mode,
            evm_version=evm_version or "cancun",
            entry=window or "bb0",
            following=following,
            scoped=window is not None,
        )
    )
    program.canonical_ir = projection
    if function is not None or window is not None:
        program.replay_ir = parsed.stdout
        program.selector = function or window
        program.scope = "function" if function is not None else "window"
        program.following = following
    program.evm_version = evm_version or "cancun"
    return program


def definition(name: str, program: Program) -> str:
    widths = program.input_widths or (256,) * len(program.inputs)
    args = " ".join(
        f"({arg} : BitVec {width})"
        for arg, width in zip(program.inputs, widths, strict=True)
    )
    body = "\n".join("  " + line for line in program.body.splitlines())
    return f"def {name} {args} : {program.result_type} :=\n{body}\n"


def build_model(timeout: int) -> subprocess.CompletedProcess:
    # Multiple compiler workers can enter on a cold Lake cache. Only the build
    # needs exclusion; independent pair proofs can run in parallel afterward.
    if os.name == "nt":
        import msvcrt

        cache = ROOT / ".lake"
        cache.mkdir(exist_ok=True)
        with (cache / "solar-tv.lock").open("a+b") as lock:
            lock.seek(0)
            msvcrt.locking(lock.fileno(), msvcrt.LK_LOCK, 1)
            return run(["lake", "build", "SolarTV"], ROOT, timeout)

    import fcntl

    # Share the dependency's build lock with the existing EVM rule prover.
    with (ROOT.parent / "scripts/evm-rules/lean/lakefile.toml").open("rb") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        return run(["lake", "build", "SolarTV"], ROOT, timeout)


def check(
    a: Program,
    b: Program,
    timeout: int,
    *,
    replay_command: str,
    replay_executable: str,
    verbose: bool = False,
) -> audit.Result:
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

        result = lean(statement + proof + "#print axioms pair_correct\n")
        proof_log = result.stdout + result.stderr
        query_only = False
        if result.returncode:
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
    selection = parser.add_mutually_exclusive_group()
    selection.add_argument(
        "--function", help="check one function in the full MIR module"
    )
    selection.add_argument(
        "--window", help="check one complete EVM block by stable label"
    )
    parser.add_argument(
        "--stack-height", type=int, help="fixed incoming EVM window height"
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
        window=args.window,
        stack_height=args.stack_height,
        world=args.world,
    )
    programs: list[Program] = []
    model = audit.semantics()
    try:
        gas.validate(args.gas, version or "cancun")
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
            window=args.window,
            stack_height=args.stack_height,
            world=args.world,
        )
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
                stack_height=args.stack_height,
                world_path=args.world,
                replay_executable=rust_replay,
            )
            for path in (args.before, args.after)
        ]
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
        if args.function or args.window:
            record["region"] = {
                "kind": "function" if args.function else "window",
                "name": args.function or args.window,
                "stack_height": args.stack_height,
            }
            record["whole_pass"] = False
        if args.report:
            audit.write(args.report, record)
    except OSError as error:
        print(f"cannot write audit report: {error}", file=sys.stderr)
        return 2
    print(result.stdout, end="")
    print(result.stderr + audit.context(record), file=sys.stderr, end="")
    return result.exit_code
