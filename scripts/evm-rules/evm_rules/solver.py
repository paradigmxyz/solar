"""Strict cvc5 process protocol shared by verification and artifact replay.

Only a successful process returning exactly UNSAT proves a query. Try internal
bitblasting, default bitblasting, then integer bitvector solving. Retry only
timeouts and UNKNOWN; never retry SAT or protocol errors.
"""

import hashlib
import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path


class QueryCache:
    """Reuse only UNSAT for identical queries and solver identities.

    The directory comes from the CLI through the environment so spawned bit
    workers use the same cache. Entries are atomic and contain their full key;
    malformed or incomplete entries are misses. Cached results are trusted
    solver answers, not independently checked proof certificates.
    """

    def __init__(self, query, solver):
        directory = os.environ.get("SOLAR_PROOF_CACHE")
        self.path = None
        self.entry = {
            "schema": 1,
            "solver": solver,
            "query_sha256": hashlib.sha256(query.encode()).hexdigest(),
            "status": "unsat",
        }
        if directory:
            key = hashlib.sha256(
                json.dumps(self.entry, sort_keys=True).encode()
            ).hexdigest()
            self.path = Path(directory) / key[:2] / f"{key}.json"

    def hit(self):
        if self.path is None:
            return False
        try:
            return json.loads(self.path.read_text()) == self.entry
        except OSError, ValueError:
            return False

    def save(self):
        if self.path is None:
            return
        self.path.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.NamedTemporaryFile(
            mode="w", dir=self.path.parent, delete=False
        ) as file:
            temporary = Path(file.name)
            try:
                file.write(json.dumps(self.entry, sort_keys=True) + "\n")
                file.close()
                temporary.replace(self.path)
            finally:
                temporary.unlink(missing_ok=True)


def solve_query(data, solver, timeout_ms):
    result = {}
    attempts = []
    for strategy in (["--bv-solver=bitblast-internal"], [], ["--solve-bv-as-int=sum"]):
        try:
            process = subprocess.run(
                [solver, "--lang", "smt2", f"--tlimit={timeout_ms}", *strategy],
                check=False,
                input=data,
                capture_output=True,
                timeout=timeout_ms / 1000 + 1,
            )
            stdout = process.stdout.decode(errors="replace").strip()
            stderr = process.stderr.decode(errors="replace").strip()
            status = (
                stdout
                if process.returncode == 0 and stdout in ("unsat", "sat", "unknown")
                else "error"
            )
            if (
                status == "error"
                and stdout in ("", "unknown")
                and "interrupted by timeout" in stderr
            ):
                status = "timeout"
            attempt = {
                "status": status,
                "flags": strategy,
                "returncode": process.returncode,
                "stdout": stdout[:4096],
                "stderr": stderr[:4096],
            }
        except subprocess.TimeoutExpired:
            attempt = {
                "status": "timeout",
                "flags": strategy,
                "reason": "solver process exceeded its time limit",
            }
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
            version = subprocess.run(
                [executable, "--version"],
                check=False,
                capture_output=True,
                text=True,
                timeout=5,
            )
        except subprocess.TimeoutExpired as error:
            raise ValueError("cvc5 version probe exceeded its time limit") from error
        if version.returncode != 0 or "cvc5" not in version.stdout.lower():
            raise ValueError("the fallback solver must identify itself as cvc5")
        self.metadata = {
            "name": "cvc5",
            "version": version.stdout.strip(),
            "executable": executable,
            "timeout_ms_per_strategy": timeout_ms,
            "executable_sha256": hashlib.sha256(
                Path(executable).read_bytes()
            ).hexdigest(),
        }

    def solve(self, query):
        data = query.encode()
        cache = QueryCache(
            query,
            {
                key: self.metadata[key]
                for key in ("name", "version", "executable_sha256")
            },
        )
        if cache.hit():
            return {
                "status": "unsat",
                "cache_hit": True,
                "solver": self.metadata,
                "query_sha256": hashlib.sha256(data).hexdigest(),
            }
        try:
            result = solve_query(
                data,
                self.metadata["executable"],
                self.metadata["timeout_ms_per_strategy"],
            )
        except OSError as error:
            result = {"status": "error", "reason": str(error)}
        if result["status"] == "unsat":
            cache.save()
        return {
            **result,
            "solver": self.metadata,
            "query_sha256": hashlib.sha256(data).hexdigest(),
        }
