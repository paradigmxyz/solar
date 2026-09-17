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

from . import compare, corpus, display, fandango, suite
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


def import_input(db, path, target, version, *, request=None, quiet=False):
    if request is None:
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
    if target != "*:*" and (not separator or not name or source_path not in sources):
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
    if not quiet:
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


def compare_saved(db, root, args, *, compilation_id=None, expectations=None):
    if expectations is None:
        rules = load_rules(args.expectations)
        expectations = rules, compare.rule_index(rules)
    rules, rule_index = expectations
    if bool(args.left) != bool(args.right):
        raise ValueError("--left and --right must be supplied together")
    selected = args.check or ["abi", "methods"]
    if args.left:
        pairs = [(compilation_id or "files", args.left.resolve(), args.right.resolve())]
    else:
        if args.reference == args.candidate:
            raise ValueError("reference and candidate must differ")
        rows = db.execute(
            """
            SELECT compilation_id, compiler, directory FROM attempts
            WHERE compiler IN (?, ?) AND (? IS NULL OR compilation_id = ?)
            QUALIFY row_number() OVER (
                PARTITION BY compilation_id, compiler ORDER BY started DESC, id DESC
            ) = 1 ORDER BY compilation_id, compiler
        """,
            [args.reference, args.candidate, compilation_id, compilation_id],
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
                            "generator.json",
                            "import.json",
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


def run_engine(root, args, *, directory=None):
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
    if directory is None:
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


def fuzz_campaign(root, args):
    specs = args.compiler or [
        "solc=solc --standard-json",
        "solar=solar --standard-json",
    ]
    names = [corpus.parse_compiler_spec(spec)[0] for spec in specs]
    if len(set(names)) != len(names):
        raise ValueError("provide unique --compiler NAME='COMMAND ARGS' entries")
    reference = args.reference or names[0]
    candidates = args.candidate or [name for name in names if name != reference]
    if not args.generate_only and (
        reference not in names
        or not candidates
        or any(name not in names or name == reference for name in candidates)
    ):
        raise ValueError(
            "select a reference and at least one distinct candidate from --compiler names"
        )
    rules = load_rules(args.expectations)
    expectations = rules, compare.rule_index(rules)
    symbolic_arguments = shlex.split(args.symbolic_args)
    campaign, config, grammar = fandango.prepare(root, args)
    data = campaign / args.version
    db = corpus.connect(data)
    report = {
        "status": "running",
        "config": config,
        "generated": 0,
        "cases": [],
        "reference": reference,
        "candidates": candidates,
        "compilers": specs,
    }
    print(f"Campaign: {campaign}", flush=True)
    try:
        files = fandango.generate(campaign, config, grammar, args.generation_timeout)
        report["generated"] = len(files)
        population = {
            record["name"]: (campaign / "population" / f"{index:08d}.sol").read_text(
                encoding="utf-8"
            )
            for index, record in enumerate(config.get("population", []))
        }
        compilers = (
            []
            if args.generate_only
            else [corpus.compiler_info(spec, data) for spec in specs]
        )
        for number, source in enumerate(files, 1):
            case = {
                "source": str(source),
                "status": "running",
                "comparisons": [],
                "symbolic": [],
            }
            report["cases"].append(case)
            request = {
                "language": "Solidity",
                "sources": {
                    "generated.sol": {"content": source.read_text(encoding="utf-8")}
                },
                "settings": config["settings"],
            }
            input_path = source.with_suffix(".json")
            corpus.write_json(input_path, request)
            identifier = import_input(
                db,
                input_path,
                "generated.sol:" + args.contract,
                args.version,
                request=request,
            )
            case["compilation_id"] = identifier
            print(f"[{number}/{len(files)}] {source.name}", flush=True)
            if args.generate_only:
                case["status"] = "imported"
                continue
            attempts = {}
            code = corpus.run(
                db,
                data,
                args,
                compilation_id=identifier,
                compilers=compilers,
                attempts=attempts,
            )
            case["compile_exit"] = code
            case["attempts"] = {
                name: str(attempts[name]) for name in names if name in attempts
            }
            for name in names:
                if name in attempts:
                    provenance = attempts[name] / "generator.json"
                    if not provenance.exists():
                        corpus.write_json(
                            provenance,
                            {
                                "campaign": str(campaign),
                                "config": config,
                                "grammar": grammar.decode(),
                                "population": population,
                                "source": str(source),
                            },
                        )
            case["status"] = "failed" if code else "passed"
            if code == 130:
                report["status"] = "interrupted"
                return 130
            if not code:
                for candidate in candidates:
                    comparison_args = argparse.Namespace(
                        **{
                            **vars(args),
                            "left": attempts[reference],
                            "right": attempts[candidate],
                            "reference": reference,
                            "candidate": candidate,
                        }
                    )
                    comparison_code = compare_saved(
                        db,
                        data,
                        comparison_args,
                        compilation_id=identifier,
                        expectations=expectations,
                    )
                    comparison_directory = db.execute(
                        "SELECT directory FROM comparisons ORDER BY started DESC LIMIT 1"
                    ).fetchone()[0]
                    case["comparisons"].append(
                        str(Path(comparison_directory) / "report.json")
                    )
                    if comparison_code:
                        case["status"] = "failed"
                        if not args.continue_on_failure:
                            break
                    if args.symbolic_signature:
                        engine_directory = data / "engines/symbolic" / uuid.uuid4().hex
                        engine_args = argparse.Namespace(
                            action="symbolic",
                            engine_timeout=args.engine_timeout,
                            arguments=[
                                *symbolic_arguments,
                                "--source",
                                "generated.sol",
                                "--contract",
                                args.contract,
                                "--signature",
                                args.symbolic_signature,
                                "--solc",
                                args.symbolic_solc,
                                "--solc-attempt",
                                str(attempts[reference]),
                                "--solar-attempt",
                                str(attempts[candidate]),
                            ],
                        )
                        engine_code = run_engine(
                            data, engine_args, directory=engine_directory
                        )
                        engine_report = engine_directory / "result.json"
                        case["symbolic"].append(str(engine_report))
                        if (
                            engine_code
                            or not engine_report.exists()
                            or read_json(engine_report)
                            .get("engine_report", {})
                            .get("status")
                            != "bounded_agreement"
                        ):
                            case["status"] = "failed"
                        if engine_code == 130:
                            report["status"] = "interrupted"
                            return 130
                    if case["status"] == "failed" and not args.continue_on_failure:
                        break
            corpus.write_json(campaign / "report.json", report)
            if case["status"] == "failed" and not args.continue_on_failure:
                break
        failed = sum(case["status"] == "failed" for case in report["cases"])
        report["status"] = (
            "failed" if failed else "generated" if args.generate_only else "passed"
        )
        print(
            f"{report['status'].upper()}: {len(report['cases'])}/{len(files)} cases processed; {failed} failed"
        )
        return int(failed > 0)
    except BaseException as error:
        report.update(
            status="interrupted" if isinstance(error, KeyboardInterrupt) else "error",
            error=str(error),
        )
        raise
    finally:
        corpus.write_json(campaign / "report.json", report)
        db.close()
        print(f"campaign report: {campaign / 'report.json'}")
        print(f"reuse with --dir {campaign} --version {args.version}")


def add_comparison_arguments(parser):
    parser.add_argument("--check", action="append", choices=compare.COMPARATORS)
    parser.add_argument("--policy", choices=("interface", "exact"), default="interface")
    parser.add_argument(
        "--full",
        action="store_true",
        help="Print every difference and complete JSON values",
    )
    parser.add_argument("--expectations", type=Path)
    parser.add_argument("--continue-on-failure", action="store_true")


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
    fuzzer = sub.add_parser(
        "fuzz", help="Generate Solidity with Fandango, then compile and compare"
    )
    fuzzer.add_argument(
        "--grammar",
        type=Path,
        default=Path(__file__).resolve().parents[3]
        / "fuzz/fandango/solidity-source.fan",
    )
    fuzzer.add_argument("--contract", default="FandangoSource")
    fuzzer.add_argument("--seed", type=int, default=1)
    fuzzer.add_argument("--count", type=int, default=16)
    fuzzer.add_argument(
        "--rounds", type=int, default=1, help="Batches with consecutive seeds"
    )
    fuzzer.add_argument(
        "--initial-population", type=Path, help="Recursively snapshot .sol seed files"
    )
    fuzzer.add_argument("--population-size", type=int)
    fuzzer.add_argument("--mutation-rate", type=float)
    fuzzer.add_argument("--crossover-rate", type=float)
    fuzzer.add_argument(
        "--settings",
        type=Path,
        help="JSON settings object; evmVersion defaults to osaka",
    )
    fuzzer.add_argument("--generation-timeout", type=float, default=600)
    fuzzer.add_argument("--generate-only", action="store_true")
    fuzzer.add_argument(
        "--reference", help="Baseline compiler name (default: first compiler)"
    )
    fuzzer.add_argument(
        "--candidate",
        action="append",
        help="Candidate name; repeat to select several (default: all others)",
    )
    fuzzer.add_argument(
        "--symbolic-signature",
        help="Also check this function in every generated source",
    )
    fuzzer.add_argument(
        "--symbolic-solc",
        default="solc",
        help="Solc executable for the symbolic harness",
    )
    fuzzer.add_argument(
        "--symbolic-args",
        default="",
        help="Additional symbolic engine options, shell-quoted",
    )
    fuzzer.add_argument("--engine-timeout", type=float, default=120)
    corpus.add_compiler_arguments(fuzzer)
    add_comparison_arguments(fuzzer)
    fuzzer.set_defaults(limit=None)
    importer = sub.add_parser("import-input", help="Import a local standard-JSON input")
    importer.add_argument("input", type=Path)
    importer.add_argument("--target", required=True, metavar="SOURCE:CONTRACT")
    directory = sub.add_parser(
        "import-directory",
        help="Import Solidity files or upstream solc fixtures recursively",
    )
    directory.add_argument("directory", type=Path)
    directory.add_argument(
        "--verbose",
        action="store_true",
        help="Print every imported and skipped fixture",
    )
    directory.add_argument("--format", choices=["solidity", "solc"], default="solc")
    directory.add_argument(
        "--settings", type=Path, help="Default standard-JSON settings"
    )
    directory.add_argument(
        "--allow-skips",
        action="store_true",
        help="Exit successfully when unsupported cases are reported",
    )
    comparer = sub.add_parser("compare", help="Compare saved compiler outputs")
    comparer.add_argument(
        "--left", type=Path, help="Reference JSON output or attempt directory"
    )
    comparer.add_argument(
        "--right", type=Path, help="Candidate JSON output or attempt directory"
    )
    comparer.add_argument("--reference", default="solc")
    comparer.add_argument("--candidate", default="solar")
    add_comparison_arguments(comparer)
    args, rest = parser.parse_known_args(argv)
    if args.action == "self-test":
        if rest:
            parser.error("unrecognized arguments: " + " ".join(rest))
        tests = unittest.TestSuite(
            unittest.defaultTestLoader.loadTestsFromTestCase(t)
            for t in (compare.Tests, display.Tests, fandango.Tests, suite.Tests, Tests)
        )
        return int(not unittest.TextTestRunner().run(tests).wasSuccessful())
    if args.action in {"sync", "run", "status"}:
        return corpus.main(argv)
    if rest:
        parser.error("unrecognized arguments: " + " ".join(rest))
    if not corpus.re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", args.version):
        parser.error("--version must be a release number such as 0.8.36")
    root = args.dir.expanduser().resolve() / args.version
    if args.action == "fuzz":
        if args.count < 1 or not 0 <= args.seed < 2**32 or not args.contract:
            parser.error(
                "--count must be positive, --seed must fit uint32, and --contract must be nonempty"
            )
        if any(
            not 0 < value < float("inf")
            for value in (args.timeout, args.generation_timeout, args.engine_timeout)
        ):
            parser.error("timeouts must be positive and finite")
        if args.rounds < 1 or args.seed + args.rounds > 2**32:
            parser.error(
                "--rounds must be positive and consecutive seeds must fit uint32"
            )
        if args.population_size is not None and args.population_size < 1:
            parser.error("--population-size must be positive")
        if any(
            value is not None and not 0 <= value <= 1
            for value in (args.mutation_rate, args.crossover_rate)
        ):
            parser.error("mutation and crossover rates must be between 0 and 1")
        failed = False
        for index in range(args.rounds):
            batch = argparse.Namespace(**{**vars(args), "seed": args.seed + index})
            print(f"Round {index + 1}/{args.rounds}; seed {batch.seed}", flush=True)
            code = fuzz_campaign(root, batch)
            if code == 130:
                return code
            failed |= bool(code)
            if code and not args.continue_on_failure:
                break
        return int(failed)
    if args.action in {"symbolic", "runtime"}:
        if not 0 < args.engine_timeout < float("inf"):
            parser.error("--engine-timeout must be positive and finite")
        return run_engine(root, args)
    db = corpus.connect(root)
    try:
        if args.action == "import-input":
            import_input(db, args.input, args.target, args.version)
            return 0
        if args.action == "import-directory":
            return suite.import_directory(db, root, args, import_input)
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
    def test_directory_import_coverage_and_resume(self):
        directory = self.root / "fixtures"
        directory.mkdir()
        (directory / "good.sol").write_text("contract C {}\n// ----\n")
        (directory / "bad.sol").write_text(
            "contract C {\n// ----\n// ParserError 1: expected brace\n"
        )
        (directory / "unsupported.sol").write_text(
            "contract C {}\n// ====\n// SMTEngine: all\n// ----\n"
        )
        args = argparse.Namespace(
            directory=directory,
            format="solc",
            settings=None,
            version=corpus.DEFAULT_VERSION,
            allow_skips=False,
            verbose=False,
        )
        self.assertEqual(
            suite.import_directory(self.db, self.root, args, import_input), 1
        )
        args.allow_skips = True
        self.assertEqual(
            suite.import_directory(self.db, self.root, args, import_input), 0
        )
        self.assertEqual(
            self.db.execute("SELECT count(*) FROM compilations").fetchone(), (1,)
        )
        self.assertEqual(
            self.db.execute("SELECT count(*) FROM input_provenance").fetchone(), (1,)
        )
        report = read_json(next((self.root / "imports").glob("*/report.json")))
        self.assertEqual(report["counts"], {"imported": 1, "skipped": 2})

    def test_rounds_stop_and_continue(self):
        with corpus.patch.object(
            sys.modules[__name__], "fuzz_campaign", side_effect=[1, 0]
        ) as campaign:
            self.assertEqual(main(["fuzz", "--rounds", "2", "--seed", "9"]), 1)
            self.assertEqual(campaign.call_count, 1)
        with corpus.patch.object(
            sys.modules[__name__], "fuzz_campaign", side_effect=[1, 0]
        ) as campaign:
            self.assertEqual(
                main(["fuzz", "--rounds", "2", "--seed", "9", "--continue-on-failure"]),
                1,
            )
            self.assertEqual(
                [call.args[1].seed for call in campaign.call_args_list], [9, 10]
            )

    def test_fuzz_stop_continue_expectations_and_resume(self):
        grammar = self.root / "grammar.fan"
        grammar.write_text("<start> ::= 'contract FandangoSource {}'\n")
        sources = []
        for index in range(2):
            source = self.root / f"{index}.sol"
            source.write_text(f"contract FandangoSource {{ uint x{index}; }}")
            sources.append(source)
        artifact = {
            "contracts": {
                "generated.sol": {
                    "FandangoSource": {
                        "abi": [
                            {
                                "type": "function",
                                "name": "f",
                                "inputs": [],
                                "outputs": [],
                                "stateMutability": "view",
                            }
                        ],
                        "evm": {
                            "bytecode": {"object": ""},
                            "deployedBytecode": {"object": ""},
                            "methodIdentifiers": {"f()": "26121ff0"},
                        },
                    }
                }
            }
        }
        baseline = self.fake("baseline", "print(" + repr(json.dumps(artifact)) + ")")
        artifact["contracts"]["generated.sol"]["FandangoSource"]["abi"][0][
            "stateMutability"
        ] = "pure"
        candidate = self.fake("candidate", "print(" + repr(json.dumps(artifact)) + ")")
        base = [
            "--dir",
            str(self.root / "campaigns"),
            "fuzz",
            "--grammar",
            str(grammar),
            "--count",
            "2",
            "--compiler",
            baseline,
            "--compiler",
            candidate,
        ]
        with corpus.patch.object(fandango, "generate", return_value=sources):
            self.assertEqual(main(base), 1)
            report_path = next(
                (self.root / "campaigns" / corpus.DEFAULT_VERSION / "fuzz").glob(
                    "*/report.json"
                )
            )
            report = read_json(report_path)
            self.assertEqual(len(report["cases"]), 1)
            comparison = read_json(Path(report["cases"][0]["comparisons"][0]))
            bundle = Path(comparison["pairs"][0]["bundle"])
            self.assertEqual(
                read_json(bundle / "left/generator.json")["config"]["seed"], 1
            )
            self.assertEqual(main([*base, "--continue-on-failure"]), 1)
            self.assertEqual(len(read_json(report_path)["cases"]), 2)
            result = comparison["pairs"][0]["results"][0]
            rules = [
                {
                    **{key: result[key] for key in ("comparator", "version", "policy")},
                    "difference": difference,
                    "reason": "test divergence",
                }
                for difference in result["differences"]
            ]
            expected = self.root / "expected.json"
            corpus.write_json(expected, rules)
            self.assertEqual(main([*base, "--expectations", str(expected)]), 0)
            self.assertEqual(read_json(report_path)["status"], "passed")
            another = baseline.replace("baseline=", "another=", 1)
            self.assertEqual(
                main([*base, "--compiler", another, "--expectations", str(expected)]), 0
            )
            self.assertEqual(len(read_json(report_path)["cases"][0]["comparisons"]), 2)

            data = corpus.connect(report_path.parent / corpus.DEFAULT_VERSION)
            try:
                self.assertEqual(
                    data.execute("SELECT count(*) FROM attempts").fetchone(), (6,)
                )
            finally:
                data.close()
            matching_candidate = baseline.replace("baseline=", "candidate=", 1)
            self.assertEqual(main([*base[:-1], matching_candidate]), 0)
            self.assertEqual(main(base), 1)
            selected = Path(read_json(report_path)["cases"][0]["attempts"]["candidate"])
            command = shlex.split(candidate.partition("=")[2])
            command[0] = str(Path(command[0]).resolve())
            self.assertEqual(read_json(selected / "compiler.json")["command"], command)

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
        result = read_json(right / "result.json")
        result["failure"] = "missing bytecode output"
        corpus.write_json(right / "result.json", result)
        self.assertEqual(compare_saved(self.db, self.root, args), 1)
        result["failure"] = None
        corpus.write_json(right / "result.json", result)
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
