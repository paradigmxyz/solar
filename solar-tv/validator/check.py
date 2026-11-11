"""Check a concrete pair with the pinned Lean kernel and replay candidate witnesses."""

import argparse
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from . import evmir, mir
from .common import Program, Unsupported

ROOT = Path(__file__).resolve().parents[1]
PRELUDE = "import SolarTV\nopen SolarTV EvmRules\nset_option autoImplicit false\nset_option linter.unusedSimpArgs false\nset_option linter.unusedVariables false\nset_option maxRecDepth 4096\n"
UNFOLD = (
    "before, after, MIR.eval, MIR.choose, MIR.phi, MIR.cast, MIR.compare, MIR.branch, MIR.guard, "
    "Outcome.bind_return, "
    "EVM.execute, EVM.finish, EVM.stepChecked, EVM.step, Evm.eq, Evm.iszero, Evm.not, "
    "Evm.sub, Evm.add, Evm.mul, Evm.and, Evm.or, Evm.xor"
)


def run(args: list, cwd: Path, timeout: int) -> subprocess.CompletedProcess:
    return subprocess.run(
        args, check=False, cwd=cwd, text=True, capture_output=True, timeout=timeout
    )


def load(kind: str, path: Path, solar: str, timeout: int) -> Program:
    suffix = ".mir" if kind == "mir" else ".evmir"
    if path.suffix != suffix or path.stat().st_size > 128 * 1024:
        raise Unsupported(f"expected a {suffix} file of at most 128 KiB")
    pipeline = "mir" if kind == "mir" else "evm-ir"
    parsed = run(
        [solar, path.resolve(), f"-Z{pipeline}-pipeline=none", "-Zvalidate-ir=true"],
        ROOT,
        timeout,
    )
    if parsed.returncode:
        raise Unsupported(parsed.stderr.strip() or "compiler rejected input")
    return (mir.translate if kind == "mir" else evmir.translate)(parsed.stdout)


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


def check(a: Program, b: Program, timeout: int) -> int:
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
        if a.result_type.startswith("Outcome "):
            return f"refines ({lhs}) ({rhs})"
        return f"({lhs}) = ({rhs})"

    statement = f"theorem pair_correct {binders} : {relation(f'before {args}', f'after {args}')} := by\n"
    # Simplify dead definitions first. Splitting branches and reasoning about
    # unsigned bounds with omega produces kernel proofs for guarded cleanups.
    # bv_decide can find witnesses; its native proof axioms remain rejected below.
    simplify = (
        f"simp [refines_iff_eq, {UNFOLD}, BitVec.add_comm, BitVec.sub_eq_iff_eq_add]"
        if a.result_type == "EVM.Execution"
        else f"simp only [refines_iff_eq, {UNFOLD}]"
    )
    proof = (
        f"  {simplify}\n"
        "  repeat (first | intro _ | split)\n"
        "  all_goals simp_all [MIR.checked, Outcome.bind, BitVec.uaddOverflow, "
        "BitVec.usubOverflow, BitVec.umulOverflow, BitVec.saddOverflow, "
        "BitVec.ssubOverflow, BitVec.smulOverflow, "
        "BitVec.toNat_eq, BitVec.ult_iff_toNat_lt, BitVec.add_comm] <;> omega\n"
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
            query_only = (
                a.result_type.startswith("Outcome ") or a.result_type == "EVM.Execution"
            )
            projection = (
                "Outcome.code" if a.result_type.startswith("Outcome ") else "EVM.code"
            )
            query = (
                f"example {binders} : {projection} (before {args}) = "
                f"{projection} (after {args}) := by\n"
                if query_only
                else statement
            )
            result = lean(
                query
                + f"  simp [{UNFOLD}, MIR.checked, EVM.code] <;> bv_decide\n"
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
            print("verified")
            return 0
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
            if a.result_type.startswith("Outcome "):
                return f"#eval IO.println (Outcome.describe ({term}))"
            if a.result_type == "Word" or a.result_type.startswith("BitVec "):
                return f"#eval IO.println (({term}).toNat)"
            return f"#eval IO.println (EVM.describe ({term}))"

        replay_proof = (
            "simp only [refines_iff_eq]; decide"
            if a.result_type.startswith("Outcome ")
            else "decide"
        )
        # Prefer a small witness when zero really demonstrates the mismatch.
        # Both it and the solver assignment must pass the same kernel replay.
        zero = [0] * len(vals)
        assignments = [zero] if vals == zero else [zero, vals]
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
            raise Unsupported(
                "Lean could neither prove equivalence nor replay a counterexample\n"
                + proof_log
                + "\nWitness query diagnostics\n"
                + log
                + "\nConcrete replay diagnostics\n"
                + replay.stdout
                + replay.stderr
            )
        outputs = [line for line in replay.stdout.splitlines() if line.strip()]
        print("counterexample")
        for arg, value in zip(a.inputs, vals, strict=True):
            print(f"  {arg} = {value}")
        print(f"  before = {outputs[-2]}\n  after  = {outputs[-1]}")
        return 1


def main() -> int:
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
    args = parser.parse_args()
    try:
        solar = shutil.which(args.solar)
        if not solar or args.timeout <= 0:
            raise Unsupported(
                "supply a built compiler with --solar and a positive timeout"
            )
        programs = [
            load(args.ir, path, str(Path(solar).resolve()), args.timeout)
            for path in (args.before, args.after)
        ]
        return check(programs[0], programs[1], args.timeout)
    except (Unsupported, OSError, subprocess.TimeoutExpired) as error:
        print(f"inconclusive: {error}", file=sys.stderr)
        return 2
