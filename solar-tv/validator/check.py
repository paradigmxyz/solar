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
PRELUDE = "import SolarTV\nopen SolarTV EvmRules\nset_option autoImplicit false\nset_option linter.unusedSimpArgs false\n"
UNFOLD = (
    "before, after, MIR.eval, MIR.choose, MIR.phi, EVM.run, EVM.step, Evm.eq, Evm.iszero, "
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
    args = " ".join(f"({arg} : Word)" for arg in program.inputs)
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
    binders = f"({args} : Word)" if args else ""
    statement = f"theorem pair_correct {binders} : before {args} = after {args} := by\n"
    # simp alone can discharge DCE even when the dead expression contains MUL.
    proof = f"  simp [{UNFOLD}] <;> bv_decide\n"
    with tempfile.TemporaryDirectory() as directory:
        file = Path(directory) / "Pair.lean"

        def lean(body: str) -> subprocess.CompletedProcess:
            file.write_text(definitions + body)
            return run(["lake", "env", "lean", file], ROOT, timeout)

        result = lean(statement + proof + "#print axioms pair_correct\n")
        if result.returncode == 0:
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
        model = dict(re.findall(r"^(\w+) = (\d+)#256$", log, re.MULTILINE))
        vals = [int(model.get(arg, "0")) for arg in a.inputs]
        inputs = " ".join(f"{value}#256" for value in vals)
        lhs, rhs = f"before {inputs}", f"after {inputs}"

        def display(term: str) -> str:
            if a.result_type == "Word":
                return f"#eval IO.println (({term}).toNat)"
            return f"#eval IO.println (EVM.describe ({term}))"

        replay = lean(
            f"example : {lhs} ≠ {rhs} := by decide\n{display(lhs)}\n{display(rhs)}\n"
        )
        if replay.returncode:
            raise Unsupported(
                "Lean could neither prove equivalence nor replay a counterexample\n"
                + log
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
