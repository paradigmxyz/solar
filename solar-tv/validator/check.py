"""Check a concrete pair with the pinned Lean kernel and replay candidate witnesses."""

import argparse
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from . import audit, evmir, gas, memory, mir, native, report
from .common import Inconclusive, Program, Unsupported

ROOT = Path(__file__).resolve().parents[1]
PRELUDE = "import SolarTV\nopen SolarTV EvmRules\nset_option autoImplicit false\nset_option linter.unusedSimpArgs false\nset_option linter.unusedVariables false\nset_option maxRecDepth 4096\n"
UNFOLD = (
    "before, after, MIR.eval, MIR.choose, MIR.phi, MIR.cast, MIR.compare, MIR.branch, MIR.guard, "
    "Outcome.bind_return, "
    "EVM.execute, EVM.finish, EVM.stepChecked, EVM.step, Evm.eq, Evm.iszero, Evm.not, "
    "Evm.sub, Evm.add, Evm.mul, Evm.and, Evm.or, Evm.xor, Evm.div, Evm.sdiv, Evm.mod, "
    "Evm.smod, Evm.addmod, Evm.mulmod, Evm.exp, Evm.signextend, Evm.lt, Evm.gt, Evm.slt, "
    "Evm.sgt, Evm.byte, Evm.shl, Evm.shr, Evm.sar"
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
) -> Program:
    if gas_mode is not None and kind != "evmir":
        raise Unsupported("gas auditing requires scheduled EVM IR, not MIR")
    suffix = ".mir" if kind == "mir" else ".evmir"
    if path.suffix != suffix or path.stat().st_size > 128 * 1024:
        raise Unsupported(f"expected a {suffix} file of at most 128 KiB")
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
    program = (
        mir.translate(parsed.stdout)
        if kind == "mir"
        else evmir.translate(
            parsed.stdout, gas=gas_mode, evm_version=evm_version or "cancun"
        )
    )
    program.canonical_ir = parsed.stdout
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
    if a.memory != b.memory:
        translate = evmir.translate if a.interface[0] == "evm-window" else mir.translate
        for program in (a, b):
            if not program.memory:
                enriched = translate(program.canonical_ir, memory=True)
                enriched.canonical_ir = program.canonical_ir
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
            "simp only [EVM.Gas.refines]; decide"
            if a.gas == "refine"
            else "simp only [refines_iff_eq]; decide"
            if a.result_type.startswith("Outcome ")
            else "decide"
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
        )
        programs = [
            load(
                args.ir,
                path,
                solar,
                args.timeout,
                gas_mode=args.gas,
                evm_version=version,
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
        if args.report:
            audit.write(args.report, record)
    except OSError as error:
        print(f"cannot write audit report: {error}", file=sys.stderr)
        return 2
    print(result.stdout, end="")
    print(result.stderr + audit.context(record), file=sys.stderr, end="")
    return result.exit_code
