"""Snapshot seeded Solidity generation separately from compiler and comparison jobs."""

import hashlib
import json
import sys
import tempfile
import unittest
import uuid
from pathlib import Path
from unittest.mock import patch

from . import corpus

VERSION = "1.1.1"
PYTHON = "3.12"


def prepare(root, args):
    grammar = args.grammar.resolve().read_bytes()
    settings = json.loads(args.settings.read_text()) if args.settings else {}
    if not isinstance(settings, dict):
        raise ValueError("--settings must contain a standard-JSON settings object")
    settings.setdefault("evmVersion", "osaka")
    config = {
        "format_version": 1,
        "fandango_version": VERSION,
        "python": PYTHON,
        "grammar_sha256": hashlib.sha256(grammar).hexdigest(),
        "seed": args.seed,
        "count": args.count,
        "contract": args.contract,
        "settings": settings,
        "solidity_version": args.version,
    }
    campaign = root / "fuzz" / corpus.digest(config)
    return campaign, config, grammar


def generate(campaign, config, grammar, timeout):
    campaign.mkdir(parents=True, exist_ok=True)
    snapshot = campaign / "grammar.fan"
    if snapshot.exists() and snapshot.read_bytes() != grammar:
        raise ValueError(f"grammar snapshot changed: {snapshot}")
    snapshot.write_bytes(grammar)
    corpus.write_json(campaign / "campaign.json", config)
    manifest_path = campaign / "generation.json"
    if manifest_path.exists():
        manifest = json.loads(manifest_path.read_text())
        files = [campaign / item["path"] for item in manifest["sources"]]
        if len(files) != config["count"]:
            raise ValueError("saved generation count does not match campaign")
        for file, record in zip(files, manifest["sources"]):
            if (
                not file.resolve().is_relative_to(campaign.resolve())
                or hashlib.sha256(file.read_bytes()).hexdigest() != record["sha256"]
            ):
                raise ValueError(f"generated source changed: {file}")
        print(f"Reusing {len(files)} generated sources: {manifest_path}", flush=True)
        return files
    directory = campaign / "generation" / uuid.uuid4().hex
    sources = directory / "sources"
    sources.mkdir(parents=True)
    tools = campaign.parents[2] / "fandango-tools"
    environment = {
        "PYTHONHASHSEED": str(config["seed"]),
        "UV_CACHE_DIR": str(tools / "cache"),
        "UV_TOOL_DIR": str(tools / "tools"),
        "UV_TOOL_BIN_DIR": str(tools / "bin"),
        "UV_PYTHON_INSTALL_DIR": str(tools / "python"),
    }
    command = [
        "uv",
        "tool",
        "run",
        "--managed-python",
        "--python",
        PYTHON,
        "--quiet",
        "--from",
        "fandango-fuzzer==" + VERSION,
        "fandango",
        "fuzz",
        "-f",
        str(snapshot),
        "--random-seed",
        str(config["seed"]),
        "-n",
        str(config["count"]),
        "--directory",
        str(sources),
        "--filename-extension",
        ".sol",
        "--progress-bar",
        "off",
    ]
    print(
        f"Generating {config['count']} sources with Fandango {VERSION}, seed {config['seed']}",
        flush=True,
    )
    result = corpus.execute(command, directory, timeout, env=environment)
    result.update(environment=environment, host_python=sys.version)
    corpus.write_json(directory / "result.json", result)
    if result["error"] == "KeyboardInterrupt":
        raise KeyboardInterrupt
    if result["error"] or result["returncode"] != 0:
        raise RuntimeError(
            f"Fandango failed ({result['error'] or result['returncode']}); see {directory / 'stderr.txt'}"
        )
    files = sorted(sources.glob("*.sol"))
    if len(files) != config["count"]:
        raise RuntimeError(
            f"Fandango produced {len(files)}/{config['count']} sources; see {directory}"
        )
    manifest = {
        "result": str((directory / "result.json").relative_to(campaign)),
        "sources": [
            {
                "path": str(file.relative_to(campaign)),
                "sha256": hashlib.sha256(file.read_bytes()).hexdigest(),
            }
            for file in files
        ],
    }
    temporary = campaign / "generation.json.tmp"
    corpus.write_json(temporary, manifest)
    temporary.replace(manifest_path)
    return files


class Tests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="compiler-diff-fandango-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)

    def test_generation_resume_and_tamper(self):
        grammar = b"<start> ::= 'contract C {}'\n"
        config = {"count": 2, "seed": 1}
        campaign = self.root / "0.8.36/fuzz/case"

        def fake_generate(command, directory, timeout, *, env):
            self.assertEqual(env["PYTHONHASHSEED"], "1")
            for index in range(2):
                (directory / "sources" / f"{index}.sol").write_text(
                    f"contract C {{ uint x{index}; }}"
                )
            return {"returncode": 0, "error": None}

        with patch.object(corpus, "execute", side_effect=fake_generate) as execute:
            files = generate(campaign, config, grammar, 20)
            self.assertEqual(generate(campaign, config, grammar, 20), files)
            self.assertEqual(execute.call_count, 1)
            files[0].write_text("changed")
            with self.assertRaisesRegex(ValueError, "generated source changed"):
                generate(campaign, config, grammar, 20)

    def test_incomplete_generation_is_not_importable(self):
        campaign = self.root / "0.8.36/fuzz/case"
        with (
            patch.object(
                corpus, "execute", return_value={"returncode": 0, "error": None}
            ),
            self.assertRaisesRegex(RuntimeError, "0/2 sources"),
        ):
            generate(campaign, {"count": 2, "seed": 1}, b"grammar", 20)
        self.assertFalse((campaign / "generation.json").exists())
