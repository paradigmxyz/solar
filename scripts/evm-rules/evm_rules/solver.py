"""Strict cvc5 process protocol shared by verification and artifact replay.

Only a successful process returning exactly UNSAT proves a query. Try internal
bitblasting, default bitblasting, then integer bitvector solving. Retry only
timeouts and UNKNOWN; never retry SAT or protocol errors.
"""

import hashlib
from pathlib import Path
import shutil
import subprocess


def solve_query(data, solver, timeout_ms):
    result = {}
    attempts = []
    for strategy in (["--bv-solver=bitblast-internal"], [], ["--solve-bv-as-int=sum"]):
        try:
            process = subprocess.run(
                [solver, "--lang", "smt2", f"--tlimit={timeout_ms}", *strategy],
                input=data, capture_output=True, timeout=timeout_ms / 1000 + 1,
            )
            stdout = process.stdout.decode(errors="replace").strip()
            stderr = process.stderr.decode(errors="replace").strip()
            status = stdout if process.returncode == 0 and stdout in ("unsat", "sat", "unknown") else "error"
            if status == "error" and stdout in ("", "unknown") and "interrupted by timeout" in stderr:
                status = "timeout"
            attempt = dict(status=status, flags=strategy, returncode=process.returncode,
                           stdout=stdout[:4096], stderr=stderr[:4096])
        except subprocess.TimeoutExpired:
            attempt = dict(status="timeout", flags=strategy,
                           reason="solver process exceeded its time limit")
        attempts.append(attempt)
        result.update(attempt)
        # Never hide SAT or a parse/process error by trying another strategy.
        if attempt["status"] not in ("timeout", "unknown"):
            break
    result["attempts"] = attempts
    return result


class Cvc5:
    """An explicitly selected fallback with recorded executable and version."""

    def __init__(self, solver="cvc5", timeout_ms=5000):
        if timeout_ms <= 0:
            raise ValueError("solver timeout must be positive")
        executable = shutil.which(str(solver))
        if executable is None:
            raise ValueError(f"solver executable not found: {solver}")
        executable = str(Path(executable).resolve())
        try:
            version = subprocess.run([executable, "--version"], capture_output=True, text=True, timeout=5)
        except subprocess.TimeoutExpired as error:
            raise ValueError("cvc5 version probe exceeded its time limit") from error
        if version.returncode != 0 or "cvc5" not in version.stdout.lower():
            raise ValueError("the fallback solver must identify itself as cvc5")
        self.metadata = {"name": "cvc5", "version": version.stdout.strip(),
                         "executable": executable, "timeout_ms_per_strategy": timeout_ms}

    def solve(self, query):
        data = query.encode()
        try:
            result = solve_query(data, self.metadata["executable"], self.metadata["timeout_ms_per_strategy"])
        except OSError as error:
            result = {"status": "error", "reason": str(error)}
        return {**result, "solver": self.metadata, "query_sha256": hashlib.sha256(data).hexdigest()}
