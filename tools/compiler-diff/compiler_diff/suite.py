"""Import directory fixtures without treating unsupported tests as passing comparisons."""

import copy
import hashlib
import json
import posixpath
import re
import tempfile
import unittest
import uuid
from collections import Counter
from pathlib import Path

from . import corpus

EVM_VERSIONS = [
    "homestead",
    "tangerineWhistle",
    "spuriousDragon",
    "byzantium",
    "constantinople",
    "petersburg",
    "istanbul",
    "berlin",
    "london",
    "paris",
    "shanghai",
    "cancun",
    "prague",
    "osaka",
]
TOKENS = re.compile(
    r"""//[^\n]*|/\*.*?\*/|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|[A-Za-z_$][\w$]*|;""",
    re.DOTALL,
)


def imports(content):
    active = False
    for match in TOKENS.finditer(content):
        token = match.group()
        if token.startswith(("//", "/*")):
            continue
        if token == "import":
            active = True
        elif token == ";":
            active = False
        elif active and token.startswith(('"', "'")):
            if "\\" in token:
                raise ValueError("escaped import paths are unsupported")
            yield token[1:-1]


def fixture(path, root, format, *, text=None):
    if text is None:
        text = path.read_text(encoding="utf-8")
    sources = {}
    metadata = {"settings": {}, "expectations": "", "external_sources": {}}
    name = path.name
    lines = []
    phase = "source"
    opened = False

    def flush():
        nonlocal opened
        if lines or opened:
            if name in sources:
                raise ValueError(f"duplicate source unit: {name}")
            sources[name] = {"content": "".join(lines)}
            lines.clear()
            opened = False

    for line in text.splitlines(keepends=True):
        if format == "solc":
            if line.startswith("// ----"):
                flush()
                phase = "expectations"
                continue
            if phase == "expectations":
                metadata["expectations"] += line
                continue
            if line.startswith("// ===="):
                flush()
                phase = "settings"
                continue
            if phase == "settings":
                match = re.fullmatch(r"//\s*([^:]+):\s*(.*?)\s*", line)
                if not match:
                    raise ValueError("malformed test settings")
                key, value = match.groups()
                if key in metadata["settings"]:
                    raise ValueError(f"duplicate test setting: {key}")
                metadata["settings"][key] = value
                continue
            match = re.fullmatch(
                r"==== (Source|ExternalSource):\s*(.*?)\s*====\s*", line
            )
            if match:
                kind, value = match.groups()
                if kind == "Source":
                    flush()
                    name = "test.sol" if value == "////" else value
                    opened = True
                    if not name or name in sources:
                        raise ValueError(f"invalid or duplicate source unit: {name}")
                else:
                    alias, separator, target = value.partition("=")
                    target = target if separator else alias
                    external = (path.parent / target.strip()).resolve()
                    if Path(
                        target.strip()
                    ).is_absolute() or not external.is_relative_to(root):
                        raise ValueError(
                            f"external source escapes import directory: {target}"
                        )
                    alias = alias.strip()
                    if not alias or alias in sources:
                        raise ValueError(
                            f"invalid or duplicate external source: {alias}"
                        )
                    sources[alias] = {"content": external.read_text(encoding="utf-8")}
                    metadata["external_sources"][alias] = target.strip()
                continue
            if line.startswith("===="):
                raise ValueError("unsupported source delimiter")
        lines.append(line)
    flush()
    if not sources:
        raise ValueError("test contains no sources")
    pending = list(sources)
    while pending:
        source_name = pending.pop()
        for imported in imports(sources[source_name]["content"]):
            resolved = (
                posixpath.normpath(
                    posixpath.join(posixpath.dirname(source_name), imported)
                )
                if imported in (".", "..") or imported.startswith(("./", "../"))
                else imported
            )
            if resolved in sources:
                continue
            external = (path.parent / resolved).resolve()
            if Path(resolved).is_absolute() or not external.is_relative_to(root):
                raise ValueError(f"import escapes import directory: {resolved}")
            if not external.is_file():
                raise ValueError(f"unresolved import: {resolved}")
            sources[resolved] = {"content": external.read_text(encoding="utf-8")}
            pending.append(resolved)
    return sources, metadata


def settings_variants(metadata, defaults):
    settings = copy.deepcopy(defaults)
    settings.setdefault("evmVersion", "osaka")
    variants = [settings]
    for key, value in metadata["settings"].items():
        if key == "EVMVersion":
            match = re.fullmatch(r"(>=|<=|>|<|=)?\s*(\w+)", value)
            if not match or match[2] not in EVM_VERSIONS:
                raise ValueError(f"unsupported EVMVersion: {value}")
            operation, version = match.groups()
            if operation in (None, "="):
                settings["evmVersion"] = version
            else:
                current = settings["evmVersion"]
                if current not in EVM_VERSIONS:
                    raise ValueError(f"unknown EVM version: {current}")
                left, right = EVM_VERSIONS.index(current), EVM_VERSIONS.index(version)
                accepted = {
                    ">=": left >= right,
                    "<=": left <= right,
                    ">": left > right,
                    "<": left < right,
                }[operation]
                if not accepted:
                    raise ValueError(f"EVMVersion {value} excludes {current}")
        elif key == "compileViaYul" and value in ("true", "false", "also"):
            settings["viaIR"] = value == "true"
        elif key == "revertStrings" and value in (
            "default",
            "strip",
            "debug",
            "verboseDebug",
        ):
            settings.setdefault("debug", {})["revertStrings"] = value
        else:
            raise ValueError(f"unsupported test setting: {key}: {value}")
    if metadata["settings"].get("compileViaYul") == "also":
        variants.append({**copy.deepcopy(settings), "viaIR": True})
    return variants


def import_directory(db, root, args, import_input):
    origin = args.directory.expanduser().resolve()
    if not origin.is_dir():
        raise ValueError("input directory does not exist")
    paths = sorted(origin.rglob("*.sol"))
    if not paths:
        raise ValueError("input directory contains no .sol files")
    defaults = json.loads(args.settings.read_text()) if args.settings else {}
    if not isinstance(defaults, dict):
        raise ValueError("--settings must contain a JSON object")
    if "remappings" in defaults:
        raise ValueError("directory imports do not yet support settings remappings")
    directory = root / "imports" / uuid.uuid4().hex
    directory.mkdir(parents=True)
    report = {"directory": str(origin), "format": args.format, "cases": []}
    try:
        for index, path in enumerate(paths):
            case = {
                "path": path.relative_to(origin).as_posix(),
                "status": "skipped",
                "compilations": [],
            }
            report["cases"].append(case)
            try:
                if not path.resolve().is_relative_to(origin):
                    raise ValueError("source escapes import directory")
                original = path.read_bytes()
                artifact = directory / f"{index:08d}"
                artifact.mkdir()
                (artifact / "fixture.sol").write_bytes(original)
                case.update(
                    artifact=str(artifact), sha256=hashlib.sha256(original).hexdigest()
                )
                sources, metadata = fixture(
                    path, origin, args.format, text=original.decode("utf-8")
                )
                case.update(metadata)
                corpus.write_json(artifact / "fixture.json", metadata)
                if re.search(
                    r"^//\s*\w*Error(?:\s|:)", metadata["expectations"], re.MULTILINE
                ):
                    raise ValueError(
                        "intentional compiler-error test; artifact comparison is inapplicable"
                    )
                for variant, settings in enumerate(
                    settings_variants(metadata, defaults)
                ):
                    request = {
                        "language": "Solidity",
                        "sources": sources,
                        "settings": settings,
                    }
                    input_path = artifact / f"input-{variant}.json"
                    corpus.write_json(input_path, request)
                    identifier = import_input(
                        db, input_path, "*:*", args.version, request=request, quiet=True
                    )
                    case["compilations"].append(identifier)
                    db.execute(
                        "INSERT OR REPLACE INTO input_provenance VALUES (?, ?, ?)",
                        [
                            identifier,
                            str(path),
                            corpus.json_text(
                                {
                                    "path": str(path),
                                    "fixture": original.decode("utf-8"),
                                    "metadata": metadata,
                                    "import_report": str(directory / "report.json"),
                                }
                            ),
                        ],
                    )
                case["status"] = "imported"
            except (ValueError, OSError) as error:
                case["reason"] = str(error)
            if args.verbose:
                print(
                    f"[{case['status'].upper()}] {case['path']}"
                    + (f": {case['reason']}" if "reason" in case else ""),
                    flush=True,
                )
            elif (index + 1) % 100 == 0:
                print(f"Importing {index + 1}/{len(paths)} files", flush=True)
        report["counts"] = dict(Counter(case["status"] for case in report["cases"]))
        report["unique_compilations"] = len(
            {
                identifier
                for case in report["cases"]
                for identifier in case["compilations"]
            }
        )
        report["skip_reasons"] = dict(
            Counter(
                case["reason"]
                for case in report["cases"]
                if case["status"] == "skipped"
            )
        )
        skipped = report["counts"].get("skipped", 0)
        print(
            f"Imported {report['counts'].get('imported', 0)}/{len(paths)} files; {skipped} skipped; {report['unique_compilations']} unique compilations"
        )
        for reason, count in Counter(report["skip_reasons"]).most_common(10):
            print(f"  {count} skipped: {reason}")
        return int(
            not report["unique_compilations"] or (skipped > 0 and not args.allow_skips)
        )
    finally:
        corpus.write_json(directory / "report.json", report)
        print(f"Import report: {directory / 'report.json'}", flush=True)


class Tests(unittest.TestCase):
    def test_sources_settings_and_imports(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "lib.sol").write_text("library L {}\n")
            path = root / "case.sol"
            path.write_text(
                '==== ExternalSource: lib.sol ====\n==== Source: a.sol ====\nimport "lib.sol"; contract A {}\n==== Source: b.sol ====\nimport "./a.sol"; contract B is A {}\n// ====\n// compileViaYul: also\n// EVMVersion: >=cancun\n// ----\n// Warning 123: message\n'
            )
            sources, metadata = fixture(path, root, "solc")
            self.assertEqual(set(sources), {"lib.sol", "a.sol", "b.sol"})
            self.assertEqual(
                sources["b.sol"]["content"], 'import "./a.sol"; contract B is A {}\n'
            )
            self.assertEqual(
                settings_variants(metadata, {}),
                [
                    {"evmVersion": "osaka", "viaIR": False},
                    {"evmVersion": "osaka", "viaIR": True},
                ],
            )
            path.write_text(
                '==== Source: empty.sol ====\n==== Source: C.sol ====\nimport "empty.sol"; contract C {}\n'
            )
            sources, _ = fixture(path, root, "solc")
            self.assertEqual(sources["empty.sol"], {"content": ""})
            path.write_text(
                "==== Source: empty.sol ====\n==== Source: empty.sol ====\n"
            )
            with self.assertRaisesRegex(ValueError, "duplicate source"):
                fixture(path, root, "solc")
            path.write_text(
                '==== Source: .hidden.sol ====\ncontract Hidden {}\n==== Source: dir/C.sol ====\nimport ".hidden.sol"; contract C {}\n'
            )
            sources, _ = fixture(path, root, "solc")
            self.assertEqual(set(sources), {".hidden.sol", "dir/C.sol"})
            path.write_text('import "../outside.sol"; contract C {}')
            with self.assertRaisesRegex(ValueError, "escapes"):
                fixture(path, root, "solidity")

    def test_settings_are_not_silently_dropped(self):
        with self.assertRaisesRegex(ValueError, "unsupported test setting"):
            settings_variants({"settings": {"SMTEngine": "all"}}, {})
        with self.assertRaisesRegex(ValueError, "excludes"):
            settings_variants({"settings": {"EVMVersion": "<cancun"}}, {})
        self.assertEqual(
            list(
                imports(
                    'string constant S = "import fake"; /* import "bad"; */ import {A} from "a.sol";'
                )
            ),
            ["a.sol"],
        )
