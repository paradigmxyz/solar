"""Maintain a local Sourcify corpus and resumable standard-JSON compiler runs.

Usage (data stays under --dir/--version, default /tmp/solar-sourcify/0.8.36):
    uv run scripts/sourcify.py sync
    uv run scripts/sourcify.py --version 0.8.34 sync
    uv run scripts/sourcify.py run
    uv run scripts/sourcify.py run --continue-on-failure
    uv run scripts/sourcify.py run --retry-failures --compiler 'solar=/path/to/solar --standard-json'
    uv run scripts/sourcify.py --dir /data/sourcify status
    uv run scripts/sourcify.py self-test

`sync` selects every compilation for --version (default 0.8.36) in the daily
public v2 export, across all chains, deduplicated by Sourcify compilation ID
rather than deployment.
It reads remote Parquet columns with DuckDB, retaining only matching records and
sources locally. The first scan can take a long time and transfer substantial data.
Each shard commits separately; unchanged shards are skipped on resume. New selected
compilations invalidate source-mapping scans, and new mappings invalidate source
scans, including older shards that may contain shared sources. The export is not
an atomic snapshot: sync again to pick up data published during the previous scan.
See https://docs.sourcify.dev/docs/repository/download-dataset/.

`run` is offline and requires a complete sync. It defaults to `solc` and `solar`
on PATH; install solc matching --version and build the desired compiler revision
first. Repeat --compiler NAME='COMMAND ARGS' to replace these defaults with any compilers that
accept standard JSON on stdin and return standard JSON on stdout, or wrappers
with that interface. Commands run without a shell. Use absolute paths for wrapper
arguments. Original compiler settings are preserved except outputSelection, which
requests ABI and creation/runtime bytecode to force code generation. This checks
compilation success, not bytecode equality or runtime equivalence.

The DuckDB database tracks imports and attempts. Successful jobs are skipped;
cached failures still stop the run unless --continue-on-failure is set. Use
--retry-failures to retry them. A changed executable, command, input, timeout, or
--tag creates new jobs. Use --tag when a wrapper's dependencies or environment
change without changing the wrapper itself. Stricter output-validation revisions
also start new jobs; older attempt directories are retained. --limit bounds new attempts, not corpus size or cached failures. Any failure makes the run exit 1, including in continue mode.

Every attempt has a directory containing the complete input (inline source texts,
never extracted to untrusted source paths), original compilation record, compiler
path/hash/version, invocation, timing, return code, stdout, stderr, and replay.sh.
Run `sh /path/to/attempt/replay.sh` to replay with the recorded executable.
Failures live under failures/; successes under runs/. Interrupted attempts remain
available and are retried next time. If a compiler executable changes during a
batch, the run stops even with --continue-on-failure; any affected attempt is
marked interrupted so it cannot be reused as a success or failure. Restart the
run after rebuilding the compiler. Environment variables are inherited but are
not dumped, to avoid saving credentials. Compiler processes and temporary files
run inside the attempt directory. Timeouts terminate the whole process group.
The script requires POSIX; DuckDB locks out concurrent writers to the same corpus.
Each version has its own database, checkpoints, sources and run directories.
Pass the same --version before sync, run and status. Sources are deduplicated
within each version; separate versions do not share source storage. Existing
unversioned corpora are left untouched.
Copy --dir for a backup; /tmp is disposable. No compiler binaries are downloaded.

Only httpfs (installed under --dir), database/spill files, logs and attempt data
are managed here. uv's own cache is controlled by UV_CACHE_DIR; set it before
invocation if it also needs to live under your chosen directory.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
import platform
import re
import shlex
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest
import urllib.error
import urllib.parse
import urllib.request
import uuid
import xml.etree.ElementTree as ET
from pathlib import Path
from unittest.mock import patch

import duckdb

from .artifacts import contract_outputs
from .display import show_attempt

EXPORT = "https://export.sourcify.dev"
DEFAULT_VERSION = "0.8.36"
# Bump when stricter output checks invalidate cached results.
VALIDATION_VERSION = 1
OUTPUTS = {
    "*": {
        "*": [
            "abi",
            "evm.bytecode.object",
            "evm.deployedBytecode.object",
            "evm.methodIdentifiers",
            "evm.deployedBytecode.immutableReferences",
            "evm.deployedBytecode.linkReferences",
            "userdoc",
            "devdoc",
        ]
    }
}


def json_text(value):
    return json.dumps(value, sort_keys=True, indent=2, ensure_ascii=False) + "\n"


def digest(value):
    return hashlib.sha256(json_text(value).encode()).hexdigest()


def write_json(path, value):
    path.write_text(json_text(value), encoding="utf-8")


def connect(root):
    root.mkdir(parents=True, exist_ok=True)
    db = duckdb.connect(str(root / "corpus.duckdb"))
    db.execute("SET temp_directory = ?", [str(root / "spill")])
    db.execute("SET memory_limit = '1GB'")
    db.execute("SET threads = 4")
    db.execute("""
        CREATE TABLE IF NOT EXISTS compilations (
            id VARCHAR PRIMARY KEY, version VARCHAR, name VARCHAR,
            fully_qualified_name VARCHAR, compiler_settings VARCHAR,
            additional_input VARCHAR, source_paths VARCHAR
        );
        CREATE TABLE IF NOT EXISTS links (
            compilation_id VARCHAR, path VARCHAR, source_hash BLOB,
            PRIMARY KEY (compilation_id, path)
        );
        CREATE TABLE IF NOT EXISTS sources (source_hash BLOB PRIMARY KEY, content VARCHAR);
        CREATE TABLE IF NOT EXISTS shards (
            key VARCHAR PRIMARY KEY, etag VARCHAR, selection VARCHAR
        );
        CREATE TABLE IF NOT EXISTS input_provenance (
            compilation_id VARCHAR, origin VARCHAR, metadata VARCHAR,
            PRIMARY KEY (compilation_id, origin)
        );
        CREATE TABLE IF NOT EXISTS state (key VARCHAR PRIMARY KEY, value VARCHAR);
        CREATE TABLE IF NOT EXISTS attempts (
            id VARCHAR PRIMARY KEY, job VARCHAR, compilation_id VARCHAR,
            compiler VARCHAR, status VARCHAR, directory VARCHAR, started DOUBLE
        );
        CREATE INDEX IF NOT EXISTS attempts_job ON attempts(job);
    """)
    return db


def list_shards(table, base=EXPORT):
    marker = ""
    while True:
        query = urllib.parse.urlencode(
            {"prefix": f"v2/{table}/", "marker": marker, "max-keys": 1000}
        )
        for attempt in range(4):
            try:
                with urllib.request.urlopen(f"{base}/?{query}", timeout=60) as response:
                    root = ET.fromstring(response.read())
                break
            except urllib.error.URLError, TimeoutError:
                if attempt == 3:
                    raise
                time.sleep(2**attempt)
        entries = root.findall("{*}Contents")
        for entry in entries:
            key = entry.findtext("{*}Key")
            if key and key.endswith(".parquet"):
                yield key, entry.findtext("{*}ETag")
        if root.findtext("{*}IsTruncated") != "true":
            return
        next_marker = root.findtext("{*}NextMarker")
        if not next_marker and entries:
            next_marker = entries[-1].findtext("{*}Key")
        if not next_marker or next_marker == marker:
            raise RuntimeError("export listing did not advance its pagination marker")
        marker = next_marker


def selection_hash(db, query):
    hasher = hashlib.sha256()
    reader = db.cursor().execute(query)
    try:
        while rows := reader.fetchmany(10000):
            for row in rows:
                hasher.update(json_text(row).encode())
    finally:
        reader.close()
    return hasher.hexdigest()


def import_shard(db, table, location, key, etag, selection):
    previous = db.execute(
        "SELECT etag, selection FROM shards WHERE key = ?", [key]
    ).fetchone()
    if previous == (etag, selection):
        return False
    queries = {
        "compiled_contracts": """
            INSERT OR REPLACE INTO compilations
            SELECT id, version, name, fully_qualified_name, compiler_settings, additional_input,
                json_extract(compilation_artifacts, '$.sources')
            FROM read_parquet(?)
            WHERE compiler = 'solc' AND lower(language) = 'solidity'
              AND (version = ? OR starts_with(version, ?))
        """,
        "compiled_contracts_sources": """
            INSERT OR REPLACE INTO links
            SELECT compilation_id, path, source_hash FROM read_parquet(?)
            WHERE compilation_id IN (SELECT id FROM compilations)
        """,
        "sources": """
            INSERT OR REPLACE INTO sources
            SELECT source_hash, content FROM read_parquet(?)
            WHERE source_hash IN (SELECT source_hash FROM links)
        """,
    }
    parameters = [location]
    if table == "compiled_contracts":
        parameters.extend([selection, selection + "+"])
    db.execute("BEGIN")
    try:
        db.execute(queries[table], parameters)
        db.execute(
            "INSERT OR REPLACE INTO shards VALUES (?, ?, ?)", [key, etag, selection]
        )
        db.execute("COMMIT")
    except BaseException:
        db.execute("ROLLBACK")
        raise
    return True


def sync(db, root, version):
    db.execute("INSERT OR REPLACE INTO state VALUES ('sync_complete', 'false')")
    db.execute("SET extension_directory = ?", [str(root / "extensions")])
    db.execute("INSTALL httpfs; LOAD httpfs")
    for table in ("compiled_contracts", "compiled_contracts_sources", "sources"):
        selection = version
        if table == "compiled_contracts_sources":
            selection = selection_hash(db, "SELECT id FROM compilations ORDER BY id")
        elif table == "sources":
            selection = selection_hash(
                db, "SELECT DISTINCT hex(source_hash) FROM links ORDER BY 1"
            )
        count = 0
        for key, etag in list_shards(table):
            count += 1
            print(f"{table} shard {count}: {key}", flush=True)
            import_shard(db, table, f"{EXPORT}/{key}", key, etag, selection)
        if not count:
            raise RuntimeError(f"export contains no shards for {table}")
    require_sources(db)
    db.execute("INSERT OR REPLACE INTO state VALUES ('sync_complete', 'true')")
    status(db)


def require_sources(db):
    missing = db.execute("""
        SELECT count(*) FROM compilations c
        WHERE NOT EXISTS (SELECT 1 FROM links l WHERE l.compilation_id = c.id)
           OR c.source_paths IS NULL
           OR list_sort(json_keys(c.source_paths)) IS DISTINCT FROM (
               SELECT list_sort(list(l.path)) FROM links l WHERE l.compilation_id = c.id
           )
           OR EXISTS (
               SELECT 1 FROM links l LEFT JOIN sources s USING (source_hash)
               WHERE l.compilation_id = c.id AND s.content IS NULL
           )
    """).fetchone()[0]
    if missing:
        raise RuntimeError(
            f"{missing} compilations have missing sources; run sync again"
        )


def make_input(db, compilation_id):
    result = db.execute("SELECT * FROM compilations WHERE id = ?", [compilation_id])
    record = dict(zip([column[0] for column in result.description], result.fetchone()))
    settings = json.loads(record["compiler_settings"])
    settings["outputSelection"] = OUTPUTS
    sources = db.execute(
        """
        SELECT path, content FROM links LEFT JOIN sources USING (source_hash)
        WHERE compilation_id = ? ORDER BY path
    """,
        [compilation_id],
    ).fetchall()
    if not sources or any(content is None for _, content in sources):
        raise RuntimeError(f"missing sources for {compilation_id}; run sync again")
    request = {
        "language": "Solidity",
        "sources": {p: {"content": c} for p, c in sources},
        "settings": settings,
    }
    additional = json.loads(record["additional_input"] or "null")
    if additional:
        # Unknown additional inputs must not silently change the replayed compilation.
        raise RuntimeError(
            f"unsupported additional_input for {compilation_id}: {additional}"
        )
    return request, record


def execute(command, directory, timeout, input_path=None, *, env=None):
    started = time.monotonic()
    result = {
        "command": command,
        "returncode": None,
        "error": None,
        "started": time.time(),
    }
    environment = os.environ.copy()
    environment.update(env or {})
    environment.update(
        {"TMPDIR": str(directory), "TMP": str(directory), "TEMP": str(directory)}
    )
    with (
        (directory / "stdout.txt").open("wb") as stdout,
        (directory / "stderr.txt").open("wb") as stderr,
        (input_path or Path(os.devnull)).open("rb") as stdin,
    ):
        try:
            process = subprocess.Popen(
                command,
                stdin=stdin,
                stdout=stdout,
                stderr=stderr,
                cwd=directory,
                env=environment,
                start_new_session=True,
            )
            try:
                process.wait(timeout=timeout)
            except (subprocess.TimeoutExpired, KeyboardInterrupt) as error:
                result["error"] = type(error).__name__
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                process.wait()
            result["returncode"] = process.returncode
        except OSError as error:
            result["error"] = str(error)
    result["seconds"] = time.monotonic() - started
    return result


def executable_state(command):
    path = shutil.which(command)
    if path is None:
        return None
    try:
        stat = Path(path).stat()
    except OSError:
        return None
    return (stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns)


def parse_compiler_spec(spec):
    name, separator, command = spec.partition("=")
    argv = shlex.split(command)
    if not separator or not name or not argv:
        raise ValueError("--compiler requires NAME='COMMAND ARGS'")
    return name, argv


def compiler_info(spec, root):
    name, argv = parse_compiler_spec(spec)
    executable = shutil.which(argv[0])
    info = {
        "name": name,
        "command": argv,
        "sha256": None,
        "file_state": executable_state(argv[0]),
    }
    if executable:
        argv[0] = str(Path(executable).resolve())
        with Path(argv[0]).open("rb") as file:
            info["sha256"] = hashlib.file_digest(file, "sha256").hexdigest()
    with tempfile.TemporaryDirectory(prefix="version-", dir=root) as temporary:
        directory = Path(temporary)
        info["version_result"] = execute([argv[0], "--version"], directory, 10)
        if info["version_result"]["error"] == "KeyboardInterrupt":
            raise KeyboardInterrupt
        info["version_stdout"] = (directory / "stdout.txt").read_text(errors="replace")
        info["version_stderr"] = (directory / "stderr.txt").read_text(errors="replace")
    if executable_state(argv[0]) != info["file_state"]:
        raise RuntimeError(f"compiler {name} changed while identifying it; restart run")
    return info


def output_error(directory, result, target=None):
    if result["error"]:
        return result["error"]
    if result["returncode"] != 0:
        return f"exit status {result['returncode']}"
    try:
        output = json.loads((directory / "stdout.txt").read_text(encoding="utf-8"))
        contracts = contract_outputs(output)
        if target and target != "*:*":
            path, _, name = target.rpartition(":")
            if name not in contracts.get(path, {}):
                return f"missing output for {target}"
        for contracts_in_file in contracts.values():
            if not isinstance(contracts_in_file, dict) or not contracts_in_file:
                return "malformed contract outputs"
            for contract in contracts_in_file.values():
                if not isinstance(contract, dict) or not isinstance(
                    contract.get("abi"), list
                ):
                    return "missing or malformed ABI output"
                for field in ("bytecode", "deployedBytecode"):
                    if not isinstance(contract["evm"][field]["object"], str):
                        return "malformed bytecode output"
    except (ValueError, UnicodeError, KeyError, TypeError) as error:
        return f"invalid or incomplete standard JSON output: {error}"
    return None


def run(db, root, args, *, compilation_id=None, compilers=None, attempts=None):
    if attempts is None:
        attempts = {}
    if db.execute("SELECT value FROM state WHERE key = 'sync_complete'").fetchone() != (
        "true",
    ):
        raise RuntimeError("no complete corpus; run sync first")
    if compilers is None:
        compilers = [
            compiler_info(spec, root)
            for spec in (
                args.compiler
                or ["solc=solc --standard-json", "solar=solar --standard-json"]
            )
        ]
    if len({compiler["name"] for compiler in compilers}) != len(compilers):
        raise ValueError("compiler names must be unique")
    script_hash = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    failures = jobs = 0
    reader = db.cursor().execute(
        "SELECT id FROM compilations WHERE (? IS NULL OR id = ?) ORDER BY id",
        [compilation_id, compilation_id],
    )
    try:
        while row := reader.fetchone():
            compilation_id = row[0]
            request, record = make_input(db, compilation_id)
            request_text = None
            for compiler in compilers:
                if executable_state(compiler["command"][0]) != compiler["file_state"]:
                    raise RuntimeError(
                        f"compiler {compiler['name']} changed; restart run"
                    )
                identity = {
                    key: compiler[key]
                    for key in (
                        "name",
                        "command",
                        "sha256",
                        "version_stdout",
                        "version_stderr",
                    )
                }
                job = digest(
                    [
                        VALIDATION_VERSION,
                        compilation_id,
                        request,
                        identity,
                        args.timeout,
                        args.tag,
                    ]
                )
                previous = db.execute(
                    "SELECT status, directory FROM attempts WHERE job = ? ORDER BY started DESC LIMIT 1",
                    [job],
                ).fetchone()
                if previous and previous[0] == "success":
                    attempts[compiler["name"]] = Path(previous[1])
                    continue
                if previous and previous[0] == "failure" and not args.retry_failures:
                    attempts[compiler["name"]] = Path(previous[1])
                    failures += 1
                    print(f"[FAIL] cached failure: {compilation_id}", flush=True)
                    show_attempt(previous[1], compiler["name"])
                    if not args.continue_on_failure:
                        return 1
                    continue
                if args.limit is not None and jobs >= args.limit:
                    return int(failures > 0)
                jobs += 1
                attempt_id = uuid.uuid4().hex
                directory = root / "runs" / attempt_id
                directory.mkdir(parents=True)
                if request_text is None:
                    request_text = json_text(request)
                (directory / "input.json").write_text(request_text, encoding="utf-8")
                write_json(directory / "compilation.json", record)
                provenance = db.execute(
                    "SELECT metadata FROM input_provenance WHERE compilation_id = ? ORDER BY origin",
                    [compilation_id],
                ).fetchall()
                if provenance:
                    write_json(
                        directory / "import.json",
                        [json.loads(row[0]) for row in provenance],
                    )
                write_json(directory / "compiler.json", compiler)
                replay = '#!/bin/sh\nset -eu\ncd -- "$(dirname -- "$0")"\nexport TMPDIR="$PWD" TMP="$PWD" TEMP="$PWD"\n'
                replay += (
                    shlex.join(compiler["command"])
                    + " < input.json > replay.stdout.txt 2> replay.stderr.txt\n"
                )
                (directory / "replay.sh").write_text(replay, encoding="utf-8")
                db.execute(
                    "INSERT INTO attempts VALUES (?, ?, ?, ?, 'running', ?, ?)",
                    [
                        attempt_id,
                        job,
                        compilation_id,
                        compiler["name"],
                        str(directory),
                        time.time(),
                    ],
                )
                result = execute(
                    compiler["command"],
                    directory,
                    args.timeout,
                    directory / "input.json",
                )
                if (
                    result["error"] != "KeyboardInterrupt"
                    and executable_state(compiler["command"][0])
                    != compiler["file_state"]
                ):
                    result["error"] = "CompilerChanged"
                reason = output_error(directory, result, record["fully_qualified_name"])
                result.update(
                    {
                        "failure": reason,
                        "timeout": args.timeout,
                        "platform": platform.platform(),
                        "python": sys.version,
                        "job": job,
                        "tag": args.tag,
                        "script_sha256": script_hash,
                        "validation_version": VALIDATION_VERSION,
                        "export": EXPORT,
                    }
                )
                write_json(directory / "result.json", result)
                outcome = "success"
                if reason:
                    outcome = (
                        "interrupted"
                        if result["error"] in ("KeyboardInterrupt", "CompilerChanged")
                        else "failure"
                    )
                    failures += 1
                    destination = root / "failures" / attempt_id
                    destination.parent.mkdir(exist_ok=True)
                    directory.rename(destination)
                    directory = destination
                db.execute(
                    "UPDATE attempts SET status = ?, directory = ? WHERE id = ?",
                    [outcome, str(directory), attempt_id],
                )
                attempts[compiler["name"]] = directory
                print(
                    f"{compiler['name']} {compilation_id}: {outcome}: {directory}",
                    flush=True,
                )
                if reason:
                    show_attempt(directory, compiler["name"])
                if result["error"] == "KeyboardInterrupt":
                    return 130
                if result["error"] == "CompilerChanged":
                    return 1
                if reason and not args.continue_on_failure:
                    return 1
    finally:
        reader.close()
    if not jobs:
        print("No pending jobs.")
    return int(failures > 0)


def status(db):
    for table in ("compilations", "links", "sources", "shards"):
        print(f"{table}: {db.execute(f'SELECT count(*) FROM {table}').fetchone()[0]}")
    print(
        "sync:",
        db.execute("SELECT value FROM state WHERE key = 'sync_complete'").fetchone(),
    )
    for compiler, outcome, count in db.execute(
        "SELECT compiler, status, count(*) FROM attempts GROUP BY ALL ORDER BY 1, 2"
    ).fetchall():
        print(f"{compiler}: {outcome}: {count} attempts")


def add_compiler_arguments(runner):
    runner.add_argument("--compiler", action="append", metavar="NAME=COMMAND")
    runner.add_argument("--timeout", type=float, default=120)
    runner.add_argument("--retry-failures", action="store_true")
    runner.add_argument(
        "--tag",
        default="",
        help="Distinguish wrapper dependencies, environment, or experiments",
    )


def main(argv=None):
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--dir", type=Path, default=Path("/tmp/solar-sourcify"))
    parser.add_argument(
        "--version",
        default=DEFAULT_VERSION,
        help="Solidity release to select (default: %(default)s)",
    )
    subparsers = parser.add_subparsers(dest="action", required=True)
    subparsers.add_parser(
        "sync", help="Import/resume compilations and sources for --version"
    )
    subparsers.add_parser("status", help="Show local corpus and attempt counts")
    runner = subparsers.add_parser("run", help="Run compilers against the local corpus")
    add_compiler_arguments(runner)
    runner.add_argument("--continue-on-failure", action="store_true")
    runner.add_argument("--limit", type=int)
    subparsers.add_parser("self-test", help="Run embedded offline regression tests")
    args = parser.parse_args(argv)
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", args.version):
        parser.error("--version must be a release number such as 0.8.36")
    if args.action == "self-test":
        return (
            0
            if unittest.TextTestRunner()
            .run(unittest.defaultTestLoader.loadTestsFromTestCase(Tests))
            .wasSuccessful()
            else 1
        )
    if args.action == "run" and (
        not 0 < args.timeout < float("inf")
        or (args.limit is not None and args.limit < 1)
    ):
        parser.error("--timeout and --limit must be positive and finite")
    root = args.dir.expanduser().resolve() / args.version
    db = connect(root)
    try:
        if args.action == "sync":
            sync(db, root, args.version)
        elif args.action == "status":
            status(db)
        else:
            return run(db, root, args)
    finally:
        db.close()
    return 0


class Tests(unittest.TestCase):
    """Offline corpus and runner regressions."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="sourcify-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.db = connect(self.root)
        self.addCleanup(self.db.close)

    def seed(self):
        self.db.execute(
            "INSERT INTO compilations VALUES ('one', ?, 'C', '../C.sol:C', '{}', 'null', ?)",
            [DEFAULT_VERSION, json_text({"../C.sol": {"id": 0}})],
        )
        self.db.execute("INSERT INTO links VALUES ('one', '../C.sol', 'abc')")
        self.db.execute("INSERT INTO sources VALUES ('abc', 'contract C {}')")
        self.db.execute("INSERT INTO state VALUES ('sync_complete', 'true')")

    def arguments(self, *specs):
        return argparse.Namespace(
            compiler=list(specs),
            timeout=2,
            continue_on_failure=False,
            retry_failures=False,
            limit=None,
            tag="",
        )

    def fake(self, name, body):
        path = self.root / (name + ".py")
        path.write_text(body)
        return f"{name}={shlex.quote(sys.executable)} {shlex.quote(str(path))}"

    def test_stop_continue_resume_retry(self):
        self.seed()
        bad = self.fake(
            "bad", 'print(\'{"errors":[{"severity":"error","message":"bad"}]}\')'
        )
        good = self.fake(
            "good",
            'print(\'{"contracts":{"../C.sol":{"C":{"abi":[],"evm":{"bytecode":{"object":""},"deployedBytecode":{"object":""}}}}}}\')',
        )
        args = self.arguments(bad, good)
        self.assertEqual(run(self.db, self.root, args), 1)
        self.assertEqual(
            self.db.execute("SELECT count(*) FROM attempts").fetchone(), (1,)
        )
        directory = Path(
            self.db.execute("SELECT directory FROM attempts").fetchone()[0]
        )
        self.assertEqual(directory.parent, self.root / "failures")
        self.assertEqual(
            json.loads((directory / "input.json").read_text())["sources"],
            {"../C.sol": {"content": "contract C {}"}},
        )
        self.assertFalse((self.root / "C.sol").exists())
        args.continue_on_failure = True
        args.limit = 1
        self.assertEqual(run(self.db, self.root, args), 1)
        self.assertEqual(
            self.db.execute("SELECT count(*) FROM attempts").fetchone(), (2,)
        )
        self.assertEqual(run(self.db, self.root, args), 1)
        self.assertEqual(
            self.db.execute("SELECT count(*) FROM attempts").fetchone(), (2,)
        )
        args.retry_failures = True
        self.assertEqual(run(self.db, self.root, args), 1)
        self.assertEqual(
            self.db.execute("SELECT count(*) FROM attempts").fetchone(), (3,)
        )

    def test_failures(self):
        for name, body in [
            ("invalid", "print('not json')"),
            ("empty", "print('{}')"),
            ("crash", "raise SystemExit(3)"),
            ("timeout", "import time; time.sleep(10)"),
        ]:
            with self.subTest(name=name):
                directory = self.root / name
                directory.mkdir()
                spec = self.fake(name, body).split("=", 1)[1]
                result = execute(shlex.split(spec), directory, 0.1)
                self.assertIsNotNone(output_error(directory, result))
        directory = self.root / "missing"
        directory.mkdir()
        result = execute([str(self.root / "no-compiler")], directory, 1)
        self.assertIsNotNone(output_error(directory, result))

    def test_abi_output_required(self):
        contract = {
            "evm": {"bytecode": {"object": ""}, "deployedBytecode": {"object": ""}}
        }
        output = {"contracts": {"../C.sol": {"C": contract}}}
        for abi in (None, {}, []):
            with self.subTest(abi=abi):
                if abi is not None:
                    contract["abi"] = abi
                write_json(self.root / "stdout.txt", output)
                result = output_error(
                    self.root, {"error": None, "returncode": 0}, "../C.sol:C"
                )
                self.assertEqual(
                    result, None if abi == [] else "missing or malformed ABI output"
                )

    def mutable_compiler(self):
        path = self.root / "compiler"
        output = {
            "contracts": {
                "../C.sol": {
                    "C": {
                        "abi": [],
                        "evm": {
                            "bytecode": {"object": ""},
                            "deployedBytecode": {"object": ""},
                        },
                    }
                }
            }
        }
        path.write_text(
            "#!/bin/sh\nprintf '%s\\n' " + shlex.quote(json.dumps(output)) + "\n"
        )
        path.chmod(0o755)
        return path

    def test_compiler_rebuild_does_not_cache(self):
        self.seed()
        compiler = self.mutable_compiler()
        original = compiler.read_text()
        args = self.arguments(f"mutable={shlex.quote(str(compiler))}")
        args.continue_on_failure = True
        original_execute = execute

        def rebuilding(*arguments):
            result = original_execute(*arguments)
            if len(arguments) == 4:
                compiler.write_text(original + "# rebuilt\n")
            return result

        with patch(__name__ + ".execute", side_effect=rebuilding):
            self.assertEqual(run(self.db, self.root, args), 1)
        self.assertEqual(
            self.db.execute("SELECT status FROM attempts").fetchall(),
            [("interrupted",)],
        )
        self.assertEqual(run(self.db, self.root, args), 0)
        compiler.write_text(original)
        self.assertEqual(run(self.db, self.root, args), 0)
        self.assertEqual(
            self.db.execute(
                "SELECT status, count(*) FROM attempts GROUP BY status ORDER BY status"
            ).fetchall(),
            [("interrupted", 1), ("success", 2)],
        )

    def test_compiler_changed_before_attempt(self):
        self.seed()
        compiler = self.mutable_compiler()
        original_info = compiler_info

        def rebuilding(*arguments):
            info = original_info(*arguments)
            compiler.write_text(compiler.read_text() + "# rebuilt\n")
            return info

        with (
            patch(__name__ + ".compiler_info", side_effect=rebuilding),
            self.assertRaisesRegex(RuntimeError, "changed; restart run"),
        ):
            run(
                self.db,
                self.root,
                self.arguments(f"mutable={shlex.quote(str(compiler))}"),
            )
        self.assertEqual(
            self.db.execute("SELECT count(*) FROM attempts").fetchone(), (0,)
        )

    def test_exit_during_interrupt(self):
        for interrupted in (
            KeyboardInterrupt(),
            subprocess.TimeoutExpired("compiler", 1),
        ):
            with (
                self.subTest(error=type(interrupted).__name__),
                patch("subprocess.Popen") as spawn,
                patch("os.killpg", side_effect=ProcessLookupError),
            ):
                spawn.return_value.wait.side_effect = [interrupted, 0]
                spawn.return_value.returncode = 0
                result = execute(["compiler"], self.root, 1)
                self.assertEqual(result["error"], type(interrupted).__name__)
                self.assertEqual(result["returncode"], 0)
                self.assertEqual(spawn.return_value.wait.call_count, 2)

    def test_import_resume_and_selection(self):
        fixture = self.root / "compilations.parquet"
        self.db.execute(
            """
            COPY (SELECT 'one' AS id, 'solc' AS compiler, 'solidity' AS language,
                $version AS version, 'C' AS name, 'C.sol:C' AS fully_qualified_name,
                '{}' AS compiler_settings, 'null' AS additional_input,
                '{"sources": {"C.sol": {"id": 0}}}' AS compilation_artifacts
                UNION ALL SELECT 'two', 'solc', 'solidity', '0.8.340', 'C', 'C.sol:C', '{}', 'null', '{}')
            TO $path (FORMAT PARQUET)
        """,
            {"version": "0.8.34+commit.abc", "path": str(fixture)},
        )
        arguments = (
            self.db,
            "compiled_contracts",
            str(fixture),
            "test",
            "etag",
            "0.8.34",
        )
        self.assertTrue(import_shard(*arguments))
        self.assertFalse(import_shard(*arguments))
        self.assertEqual(
            self.db.execute("SELECT id FROM compilations").fetchall(), [("one",)]
        )
        other = connect(self.root / "0.8.340")
        self.addCleanup(other.close)
        self.assertTrue(import_shard(other, *arguments[1:-1], "0.8.340"))
        self.assertEqual(
            other.execute("SELECT id FROM compilations").fetchall(), [("two",)]
        )
        self.assertTrue(import_shard(*arguments[:-2], "new-etag", "0.8.34"))

    def test_cli_version_isolation(self):
        base = self.root / "corpora"
        with patch("sys.stdout", new=io.StringIO()):
            self.assertEqual(main(["--dir", str(base), "status"]), 0)
            self.assertEqual(
                main(["--dir", str(base), "--version", "0.8.34", "status"]), 0
            )
        self.assertTrue((base / "0.8.36" / "corpus.duckdb").is_file())
        self.assertTrue((base / "0.8.34" / "corpus.duckdb").is_file())
        self.assertFalse((base / "corpus.duckdb").exists())
        with patch("sys.stderr", new=io.StringIO()):
            for version in ("../escape", "0.8", "0.8.36+commit.abc", "/tmp/escape"):
                with (
                    self.subTest(version=version),
                    self.assertRaises(SystemExit) as error,
                ):
                    main(["--dir", str(base), "--version", version, "status"])
                self.assertEqual(error.exception.code, 2)
        with patch(__name__ + ".sync") as importer:
            self.assertEqual(
                main(["--dir", str(base), "--version", "0.8.34", "sync"]), 0
            )
            self.assertEqual(importer.call_args.args[1:], (base / "0.8.34", "0.8.34"))

    def test_source_import_and_completeness(self):
        self.seed()
        self.db.execute("DELETE FROM links")
        self.db.execute("DELETE FROM sources")
        with self.assertRaisesRegex(RuntimeError, "missing sources"):
            require_sources(self.db)
        links = self.root / "links.parquet"
        self.db.execute(
            """
            COPY (SELECT 'one' AS compilation_id, '../C.sol' AS path, 'abc'::BLOB AS source_hash
                UNION ALL SELECT 'unselected', 'ignored.sol', 'def'::BLOB)
            TO ? (FORMAT PARQUET)
        """,
            [str(links)],
        )
        import_shard(
            self.db, "compiled_contracts_sources", str(links), "links", "etag", "ids"
        )
        self.assertEqual(self.db.execute("SELECT count(*) FROM links").fetchone(), (1,))
        with self.assertRaisesRegex(RuntimeError, "missing sources"):
            require_sources(self.db)
        sources = self.root / "sources.parquet"
        self.db.execute(
            """
            COPY (SELECT 'abc'::BLOB AS source_hash, 'contract C {}' AS content
                UNION ALL SELECT 'def'::BLOB, 'ignored')
            TO ? (FORMAT PARQUET)
        """,
            [str(sources)],
        )
        import_shard(self.db, "sources", str(sources), "sources", "etag", "hashes")
        self.assertEqual(
            self.db.execute("SELECT count(*) FROM sources").fetchone(), (1,)
        )
        require_sources(self.db)
        self.db.execute(
            "UPDATE compilations SET source_paths = ?",
            [json_text({"../C.sol": {}, "missing.sol": {}})],
        )
        with self.assertRaisesRegex(RuntimeError, "missing sources"):
            require_sources(self.db)
        with self.assertRaises(duckdb.Error):
            import_shard(
                self.db,
                "sources",
                str(self.root / "missing.parquet"),
                "failed",
                "etag",
                "hashes",
            )
        self.assertEqual(
            self.db.execute(
                "SELECT count(*) FROM shards WHERE key = 'failed'"
            ).fetchone(),
            (0,),
        )

    def test_listing_pagination(self):
        pages = [
            b'<ListBucketResult xmlns="urn:s3"><IsTruncated>true</IsTruncated><Contents><Key>v2/sources/one.parquet</Key><ETag>first</ETag></Contents></ListBucketResult>',
            b'<ListBucketResult xmlns="urn:s3"><IsTruncated>false</IsTruncated><Contents><Key>v2/sources/two.parquet</Key><ETag>second</ETag></Contents></ListBucketResult>',
        ]
        with patch(
            "urllib.request.urlopen", side_effect=[io.BytesIO(page) for page in pages]
        ) as request:
            self.assertEqual(
                list(list_shards("sources")),
                [
                    ("v2/sources/one.parquet", "first"),
                    ("v2/sources/two.parquet", "second"),
                ],
            )
            query = urllib.parse.parse_qs(
                urllib.parse.urlsplit(request.call_args.args[0]).query
            )
            self.assertEqual(query["marker"], ["v2/sources/one.parquet"])

    def test_interrupted_retry_and_changed_job(self):
        self.seed()
        good = self.fake(
            "good",
            'print(\'{"contracts":{"../C.sol":{"C":{"abi":[],"evm":{"bytecode":{"object":""},"deployedBytecode":{"object":""}}}}}}\')',
        )
        args = self.arguments(good)
        self.assertEqual(run(self.db, self.root, args), 0)
        self.db.execute("UPDATE attempts SET status = 'running'")
        self.assertEqual(run(self.db, self.root, args), 0)
        args.tag = "changed-dependencies"
        self.assertEqual(run(self.db, self.root, args), 0)
        self.assertEqual(
            self.db.execute("SELECT count(*) FROM attempts").fetchone(), (3,)
        )
        self.assertEqual(
            self.db.execute("SELECT count(DISTINCT job) FROM attempts").fetchone(), (2,)
        )
