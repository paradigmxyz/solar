"""Import inputs, compile them, and compare saved artifacts without recompiling."""

import argparse
import hashlib
import json
import shlex
import shutil
import subprocess
import sys
import time
import unittest
import uuid
from collections import Counter
from pathlib import Path

import duckdb

from . import compare, corpus, display
from .artifacts import read_attempt


def read_json(path):
    return json.loads(path.read_text(encoding="utf-8"))


def load_rules(path):
    rules = read_json(path) if path else []
    if not isinstance(rules, list):
        raise ValueError("expectations must be an array")
    for rule in rules:
        if (
            not isinstance(rule, dict)
            or set(rule) != {"comparator", "version", "policy", "difference", "reason"}
            or rule["comparator"] not in compare.COMPARATORS
            or rule["policy"] not in ("exact", "interface")
            or type(rule["version"]) is not int
            or not isinstance(rule["reason"], str)
            or not rule["reason"].strip()
            or not isinstance(rule["difference"], dict)
            or set(rule["difference"]) != {"path", "kind", "left", "right"}
            or not isinstance(rule["difference"]["path"], str)
            or rule["difference"]["kind"]
            not in ("value", "missing-left", "missing-right")
        ):
            raise ValueError(
                "invalid expectation: require comparator, version, policy, exact difference and reason"
            )
    return rules


def import_input(db, path, target, version):
    request = read_json(path)
    if not isinstance(request, dict) or request.get("language") != "Solidity":
        raise ValueError("input must be Solidity standard JSON")
    if set(request) - {"language", "sources", "settings"}:
        raise ValueError("unsupported standard JSON input fields")
    sources = request.get("sources")
    settings = request.get("settings", {})
    if not isinstance(settings, dict) or not isinstance(sources, dict) or not sources:
        raise ValueError("input requires settings object and nonempty sources object")
    source_path, separator, name = target.rpartition(":")
    if not separator or not name or source_path not in sources:
        raise ValueError("--target must be SOURCE:CONTRACT in the input")
    for source in sources.values():
        if (
            not isinstance(source, dict)
            or set(source) != {"content"}
            or not isinstance(source["content"], str)
        ):
            raise ValueError(
                "local inputs require inline content only; URL sources are unsupported"
            )
    identifier = "local-" + corpus.digest([request, target])
    empty = db.execute("SELECT count(*) FROM compilations").fetchone()[0] == 0
    db.execute("BEGIN")
    try:
        db.execute(
            "INSERT OR IGNORE INTO compilations VALUES (?, ?, ?, ?, ?, 'null', ?)",
            [
                identifier,
                version,
                name,
                target,
                corpus.json_text(settings),
                corpus.json_text({name: {} for name in sources}),
            ],
        )
        for source_path, source in sources.items():
            content = source["content"]
            source_hash = hashlib.sha256(content.encode()).digest()
            db.execute(
                "INSERT OR IGNORE INTO sources VALUES (?, ?)", [source_hash, content]
            )
            db.execute(
                "INSERT OR IGNORE INTO links VALUES (?, ?, ?)",
                [identifier, source_path, source_hash],
            )
        if empty:
            db.execute("INSERT OR REPLACE INTO state VALUES ('sync_complete', 'true')")
        db.execute("COMMIT")
    except BaseException:
        db.execute("ROLLBACK")
        raise
    print(identifier)
    return identifier


def compare_pair(left, right, selected, policy):
    outputs = []
    inputs = []
    try:
        for path in (left, right):
            if path is None:
                raise ValueError("missing compiler attempt")
            if path.is_dir():
                request, output = read_attempt(path)
                inputs.append(request)
                outputs.append(output)
            else:
                outputs.append(read_json(path))
        if len(inputs) == 2 and json.dumps(inputs[0], sort_keys=True) != json.dumps(
            inputs[1], sort_keys=True
        ):
            raise ValueError("compiler attempts used different inputs")
        return [
            compare.compare_outputs(outputs[0], outputs[1], kind, policy)
            for kind in selected
        ]
    except (ValueError, OSError, TypeError, AttributeError) as error:
        return [
            {
                "comparator": kind,
                "version": compare.VERSION,
                "policy": policy,
                "status": "error",
                "differences": [],
                "error": str(error),
            }
            for kind in selected
        ]


def compare_saved(db, root, args):
    rules = load_rules(args.expectations)
    rule_index = compare.rule_index(rules)
    if bool(args.left) != bool(args.right):
        raise ValueError("--left and --right must be supplied together")
    selected = args.check or ["abi", "methods"]
    if args.left:
        pairs = [("files", args.left.resolve(), args.right.resolve())]
    else:
        if args.reference == args.candidate:
            raise ValueError("reference and candidate must differ")
        rows = db.execute(
            """
            SELECT compilation_id, compiler, directory FROM attempts
            WHERE compiler IN (?, ?)
            QUALIFY row_number() OVER (
                PARTITION BY compilation_id, compiler ORDER BY started DESC, id DESC
            ) = 1 ORDER BY compilation_id, compiler
        """,
            [args.reference, args.candidate],
        ).fetchall()
        by_id = {}
        for identifier, compiler, directory in rows:
            by_id.setdefault(identifier, {})[compiler] = Path(directory)
        pairs = [
            (identifier, paths.get(args.reference), paths.get(args.candidate))
            for identifier, paths in by_id.items()
        ]
    if not pairs:
        raise ValueError("no saved attempts to compare")
    report = {
        "version": compare.VERSION,
        "policy": args.policy,
        "reference": args.reference,
        "candidate": args.candidate,
        "expectations": rules,
        "pairs": [],
        "summary": {},
        "failed": False,
    }
    directory = root / "comparisons" / uuid.uuid4().hex
    directory.mkdir(parents=True)
    matched = set()
    counts = Counter()
    groups = Counter()
    for identifier, left, right in pairs:
        pair = {
            "id": identifier,
            "left": str(left) if left else None,
            "right": str(right) if right else None,
            "results": compare_pair(left, right, selected, args.policy),
        }
        matched.update(compare.expectations(pair, rules, rule_index))
        report["pairs"].append(pair)
        report["failed"] = report["failed"] or bool(pair["failed"])
        for result in pair["results"]:
            counts[result["comparator"] + ":" + result["status"]] += 1
            for diff in result["differences"]:
                groups[result["comparator"] + ":" + diff["path"]] += 1
        if any(r["status"] != "equal" for r in pair["results"]):
            bundle = directory / str(len(report["pairs"]))
            bundle.mkdir()
            copied = []
            for label, source in (("left", left), ("right", right)):
                if source is not None and source.exists():
                    if source.is_dir():
                        destination = bundle / label
                        destination.mkdir()
                        for name in (
                            "input.json",
                            "stdout.txt",
                            "stderr.txt",
                            "compiler.json",
                            "compilation.json",
                            "result.json",
                            "replay.sh",
                        ):
                            if (source / name).is_file():
                                shutil.copyfile(source / name, destination / name)
                    else:
                        destination = bundle / (label + ".json")
                        shutil.copyfile(source, destination)
                    copied.append(destination)
            pair["bundle"] = str(bundle)
            if len(copied) == 2:
                corpus.write_json(bundle / "expectations.json", rules)
                command = [
                    "uv",
                    "run",
                    str(Path(__file__).resolve().parents[3] / "scripts/sourcify.py"),
                    "--dir",
                    str(root.parent),
                    "--version",
                    root.name,
                    "compare",
                    "--left",
                    str(copied[0]),
                    "--right",
                    str(copied[1]),
                    "--reference",
                    args.reference,
                    "--candidate",
                    args.candidate,
                    "--policy",
                    args.policy,
                    "--expectations",
                    str(bundle / "expectations.json"),
                ]
                for check in selected:
                    command.extend(["--check", check])
                if getattr(args, "full", False):
                    command.append("--full")
                replay = bundle / "compare.sh"
                replay.write_text(
                    "#!/bin/sh\nset -eu\nexec " + shlex.join(command) + "\n",
                    encoding="utf-8",
                )
                pair["replay"] = str(replay)
            corpus.write_json(bundle / "comparison.json", pair)
        display.show_pair(pair, getattr(args, "full", False))
        if pair["failed"] and not args.continue_on_failure:
            break
    report["summary"] = dict(counts)
    report["difference_groups"] = dict(groups)
    report["available_pairs"] = len(pairs)
    report["compilations"] = db.execute("SELECT count(*) FROM compilations").fetchone()[
        0
    ]
    report["unused_expectations"] = [i for i in range(len(rules)) if i not in matched]
    corpus.write_json(directory / "report.json", report)
    db.execute(
        "CREATE TABLE IF NOT EXISTS comparisons (id VARCHAR PRIMARY KEY, version INTEGER, directory VARCHAR, failed BOOLEAN, started DOUBLE)"
    )
    db.execute(
        "INSERT INTO comparisons VALUES (?, ?, ?, ?, ?)",
        [
            directory.name,
            compare.VERSION,
            str(directory),
            report["failed"],
            time.time(),
        ],
    )
    failed = sum(bool(pair["failed"]) for pair in report["pairs"])
    print(
        f"\n{'FAIL' if failed else 'PASS'}: {len(report['pairs'])}/{len(pairs)} pairs compared; {failed} failed; corpus contains {report['compilations']} compilations"
    )
    print(
        "Checks: "
        + ", ".join(f"{key}={count}" for key, count in sorted(counts.items()))
    )
    print(f"Policy: {args.policy}; comparator version: {compare.VERSION}")
    print(f"report: {directory / 'report.json'}")
    if report["unused_expectations"]:
        print(f"unused expectations: {report['unused_expectations']}")
    return int(report["failed"])


def run_engine(root, args):
    repository = Path(__file__).resolve().parents[3]
    script = (
        repository
        / {
            "symbolic": "fuzz/fandango/symbolic_differential.py",
            "runtime": "benches/runtime/benchmark.py",
        }[args.action]
    )
    if not script.is_file():
        raise ValueError("execution engines require a repository checkout")
    arguments = args.arguments
    if arguments[:1] == ["--"]:
        arguments = arguments[1:]
    if "--help" in arguments or "-h" in arguments:
        return subprocess.call([sys.executable, str(script), *arguments])
    directory = root / "engines" / args.action / uuid.uuid4().hex
    directory.mkdir(parents=True)
    if args.action == "symbolic":
        arguments = [*arguments, "--output-root", str(directory / "artifacts")]
    else:
        arguments = [
            *arguments,
            "--output",
            str(directory / "output.json"),
            "--artifacts",
            str(directory / "artifacts"),
        ]
    result = corpus.execute(
        [sys.executable, str(script), *arguments], directory, args.engine_timeout
    )
    # Preserve the engine's report and exit semantics, including incomplete runs.
    if args.action == "symbolic":
        try:
            result["engine_report"] = read_json(directory / "stdout.txt")
        except ValueError:
            pass
    result["engine"] = args.action
    corpus.write_json(directory / "result.json", result)
    print((directory / "stdout.txt").read_text(errors="replace"), end="")
    print(
        (directory / "stderr.txt").read_text(errors="replace"), end="", file=sys.stderr
    )
    print(f"engine report: {directory / 'result.json'}")
    if result["error"] == "KeyboardInterrupt":
        return 130
    return (
        result["returncode"]
        if result["returncode"] is not None
        and result["returncode"] >= 0
        and not result["error"]
        else 1
    )


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dir", type=Path, default=Path("/tmp/solar-sourcify"))
    parser.add_argument("--version", default=corpus.DEFAULT_VERSION)
    sub = parser.add_subparsers(dest="action", required=True)
    for name in ("sync", "run", "status", "self-test"):
        sub.add_parser(name, add_help=False)
    for name in ("symbolic", "runtime"):
        engine = sub.add_parser(name, help="Run the existing " + name + " engine")
        engine.add_argument("--engine-timeout", type=float, default=3600)
        engine.add_argument("arguments", nargs=argparse.REMAINDER)
    importer = sub.add_parser("import-input", help="Import a local standard-JSON input")
    importer.add_argument("input", type=Path)
    importer.add_argument("--target", required=True, metavar="SOURCE:CONTRACT")
    comparer = sub.add_parser("compare", help="Compare saved compiler outputs")
    comparer.add_argument(
        "--left", type=Path, help="Reference JSON output or attempt directory"
    )
    comparer.add_argument(
        "--right", type=Path, help="Candidate JSON output or attempt directory"
    )
    comparer.add_argument("--reference", default="solc")
    comparer.add_argument("--candidate", default="solar")
    comparer.add_argument("--check", action="append", choices=compare.COMPARATORS)
    comparer.add_argument(
        "--policy", choices=("interface", "exact"), default="interface"
    )
    comparer.add_argument(
        "--full",
        action="store_true",
        help="Print every difference and complete JSON values",
    )
    comparer.add_argument("--expectations", type=Path)
    comparer.add_argument("--continue-on-failure", action="store_true")
    args, rest = parser.parse_known_args(argv)
    if args.action == "self-test":
        if rest:
            parser.error("unrecognized arguments: " + " ".join(rest))
        suite = unittest.TestSuite(
            unittest.defaultTestLoader.loadTestsFromTestCase(t)
            for t in (compare.Tests, display.Tests, Tests)
        )
        return int(not unittest.TextTestRunner().run(suite).wasSuccessful())
    if args.action in {"sync", "run", "status"}:
        return corpus.main(argv)
    if rest:
        parser.error("unrecognized arguments: " + " ".join(rest))
    if not corpus.re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", args.version):
        parser.error("--version must be a release number such as 0.8.36")
    root = args.dir.expanduser().resolve() / args.version
    if args.action in {"symbolic", "runtime"}:
        if not 0 < args.engine_timeout < float("inf"):
            parser.error("--engine-timeout must be positive and finite")
        return run_engine(root, args)
    db = corpus.connect(root)
    try:
        if args.action == "import-input":
            import_input(db, args.input, args.target, args.version)
            return 0
        return compare_saved(db, root, args)
    finally:
        db.close()


def entrypoint():
    try:
        sys.exit(main())
    except KeyboardInterrupt:
        sys.exit(130)
    except (RuntimeError, ValueError, OSError, duckdb.Error) as error:
        print(f"error: {error}", file=sys.stderr)
        sys.exit(1)


class Tests(corpus.Tests):
    def test_local_import(self):
        path = self.root / "input.json"
        request = {
            "language": "Solidity",
            "sources": {"C.sol": {"content": "contract C {}"}},
            "settings": {},
        }
        corpus.write_json(path, request)
        identifier = import_input(self.db, path, "C.sol:C", corpus.DEFAULT_VERSION)
        self.assertEqual(
            import_input(self.db, path, "C.sol:C", corpus.DEFAULT_VERSION), identifier
        )
        self.assertEqual(
            self.db.execute("SELECT count(*) FROM compilations").fetchone(), (1,)
        )
        corpus.require_sources(self.db)
        self.assertEqual(
            corpus.make_input(self.db, identifier)[0]["sources"], request["sources"]
        )
        request = {
            "language": "Solidity",
            "sources": {"C.sol": {"urls": ["file:///etc/passwd"]}},
        }
        corpus.write_json(path, request)
        with self.assertRaisesRegex(ValueError, "inline content"):
            import_input(self.db, path, "C.sol:C", corpus.DEFAULT_VERSION)

    def test_saved_comparisons_and_bundles(self):
        self.seed()
        output = compare.Tests().output()
        output["contracts"]["../C.sol"] = output["contracts"].pop("C.sol")
        output["contracts"]["../C.sol"]["C"]["evm"].update(
            bytecode={"object": ""}, deployedBytecode={"object": ""}
        )
        specs = [
            self.fake(name, "print(" + repr(json.dumps(output)) + ")")
            for name in ("solc", "solar")
        ]
        self.assertEqual(corpus.run(self.db, self.root, self.arguments(*specs)), 0)
        args = argparse.Namespace(
            left=None,
            right=None,
            reference="solc",
            candidate="solar",
            check=None,
            policy="interface",
            expectations=None,
            continue_on_failure=True,
        )
        self.assertEqual(compare_saved(self.db, self.root, args), 0)
        row = self.db.execute(
            "SELECT directory FROM attempts WHERE compiler = 'solar'"
        ).fetchone()
        right = Path(row[0])
        output["contracts"]["../C.sol"]["C"]["abi"][0]["stateMutability"] = "pure"
        corpus.write_json(right / "stdout.txt", output)
        self.assertEqual(compare_saved(self.db, self.root, args), 1)
        report_path = (
            Path(
                self.db.execute(
                    "SELECT directory FROM comparisons ORDER BY started DESC LIMIT 1"
                ).fetchone()[0]
            )
            / "report.json"
        )
        report = read_json(report_path)
        self.assertTrue(
            (Path(report["pairs"][0]["bundle"]) / "left/input.json").exists()
        )
        self.assertEqual(report["summary"], {"abi:different": 1, "methods:equal": 1})
        corpus.write_json(right / "input.json", {})
        self.assertEqual(
            compare_pair(
                Path(
                    self.db.execute(
                        "SELECT directory FROM attempts WHERE compiler = 'solc'"
                    ).fetchone()[0]
                ),
                right,
                ["abi"],
                "interface",
            )[0]["status"],
            "error",
        )
        self.assertEqual(
            compare_pair(None, right, ["abi"], "interface")[0]["status"], "error"
        )
