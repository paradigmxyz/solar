"""Versioned audit records and independent status totals."""

import argparse
import hashlib
import json
import os
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

from .common import Program

ROOT = Path(__file__).resolve().parents[1]
STATUSES = (
    "verified",
    "counterexample",
    "unsupported",
    "timeout",
    "inconclusive",
    "error",
)


@dataclass
class Result:
    status: str
    stdout: str = ""
    stderr: str = ""
    program: Program | None = None
    witness: dict = field(default_factory=dict)
    pair: tuple[Program, Program] | None = None

    @property
    def exit_code(self) -> int:
        return (
            0
            if self.status == "verified"
            else 1
            if self.status == "counterexample"
            else 2
        )


def semantics() -> dict:
    digest = hashlib.sha256()
    paths = [
        *ROOT.glob("SolarTV/**/*.lean"),
        ROOT / "SolarTV.lean",
        *ROOT.glob("validator/*.py"),
        ROOT / "lean-toolchain",
    ]
    for path in sorted(paths):
        digest.update(
            path.relative_to(ROOT).as_posix().encode()
            + b"\0"
            + path.read_bytes()
            + b"\0"
        )
    dependency = ROOT.parent / "scripts/evm-rules/lean"
    for path in sorted(dependency.glob("EvmRules/**/*.lean")):
        digest.update(
            path.relative_to(dependency).as_posix().encode()
            + b"\0"
            + path.read_bytes()
            + b"\0"
        )
    return {
        "version": "solar-tv-v2",
        "sha256": digest.hexdigest(),
        "lean": (ROOT / "lean-toolchain").read_text().strip(),
    }


def record(
    result: Result,
    *,
    layer: str,
    target: str,
    policy: str | None,
    before: Path,
    after: Path,
    model: dict | None = None,
) -> dict:
    program = result.program
    assumptions = [
        "compiler-validated IR",
        "defined inputs",
        "trusted parser and importer",
    ]
    if program and program.world:
        fixture = program.world
        assumptions.extend(
            [
                "compiled full entry; trusted compiler backend lowering",
                f"explicit initial world sha256={fixture['fixture_sha256']}; omitted accounts absent and slots zero",
                "only listed symbolic storage cells and entry stack words are quantified",
                f"entry gas budget {fixture['gas']}; root value zero; origin {fixture['origin']}; gas price {fixture['gas_price']}; fresh private memory",
                "explicit block context; omitted history hashes zero; transaction admission/authorization processing excluded",
                f"model work limit {fixture['fuel']}; 64 KiB memory limit; unsupported paths prevent verification",
                "nested bytecode, constructors and active precompiles execute; fork-specific deferred SELFDESTRUCT",
                "scenario-entry proof; no arbitrary-world or whole-pass claim",
                "persistent effects only"
                if policy is None
                else "exact effects, remaining gas and refund"
                if policy == "exact"
                else "same effects/refund and no less remaining gas unless original runs out of gas",
            ]
        )
    else:
        if layer == "mir":
            assumptions.append("acyclic scalar MIR; no implicit backend frame setup")
        else:
            assumptions.append(
                "one opcode window; explicit valid label transfers; no PC/CODESIZE"
            )
            if program:
                assumptions.append(f"entry stack height {program.interface[1]}")
        if program and program.scope:
            assumptions.append(
                "region-local proof; no whole-pass or interprocedural claim"
            )
        if (program and program.memory) or (
            result.pair and any(side.memory for side in result.pair)
        ):
            assumptions.append(
                "constant accesses inside 256-byte memory prefix; representable aligned MSIZE"
            )
        assumptions.append(
            "sufficient gas"
            if policy is None
            else "exact remaining gas and outcomes"
            if policy == "exact"
            else "resource refinement; original OOG unconstrained; gas-insensitive continuation"
        )
    sources = []
    for i, path in enumerate((before, after)):
        sources.append(
            {
                "path": str(path.resolve()),
                "canonical_sha256": hashlib.sha256(
                    result.pair[i].canonical_ir.encode()
                ).hexdigest()
                if result.pair
                else None,
                "module_sha256": hashlib.sha256(
                    (result.pair[i].replay_ir or result.pair[i].canonical_ir).encode()
                ).hexdigest()
                if result.pair
                else None,
            }
        )
    return {
        "version": 1,
        "status": result.status,
        "layer": layer,
        "semantics": model or semantics(),
        "target": target,
        "gas_policy": policy
        or ("fixture-effects" if program and program.world else "sufficient"),
        "assumptions": assumptions,
        "inputs": sources,
        "evidence": "kernel-proof"
        if result.status == "verified"
        else "kernel-and-rust-replay"
        if result.status == "counterexample"
        else None,
        **(
            {
                "world": {
                    "fixture_sha256": program.world["fixture_sha256"],
                    "fixture": {
                        key: value
                        for key, value in program.world.items()
                        if key
                        not in {
                            "configuration",
                            "image_sha256",
                            "compiler",
                            "path",
                            "fixture_sha256",
                        }
                    },
                    "backend_images": [
                        side.world["image_sha256"]
                        for side in result.pair
                        if side.world is not None
                    ]
                    if result.pair
                    else [],
                    "opcode_config_sha256": hashlib.sha256(
                        program.world["configuration"].encode()
                    ).hexdigest(),
                    "native_samples": 1 if result.status == "verified" else 0,
                }
            }
            if program and program.world
            else {}
        ),
        "witness": result.witness,
        "diagnostic": result.stderr,
    }


def context(record: dict) -> str:
    model = record["semantics"]
    return (
        f"semantics: {model['version']} sha256={model['sha256']} ({model['lean']})\n"
        f"target: {record['target']}; gas policy: {record['gas_policy']}\n"
        "assumptions: " + "; ".join(record["assumptions"]) + "\n"
    )


def write(path: Path, record: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="w", encoding="utf-8", dir=path.parent, delete=False
        ) as file:
            temporary = Path(file.name)
            json.dump(record, file, indent=2)
            file.write("\n")
        os.replace(temporary, path)
    finally:
        if temporary:
            temporary.unlink(missing_ok=True)


def summarize(arguments: list[str]) -> int:
    parser = argparse.ArgumentParser(description="Summarize solar-tv audit records")
    parser.add_argument("directory", type=Path)
    args = parser.parse_args(arguments)
    totals = dict.fromkeys(STATUSES, 0)
    if not (args.directory / "audits").is_dir():
        parser.exit(2, "no audit directory found\n")
    try:
        for path in sorted((args.directory / "audits").glob("*.json")):
            data = json.loads(path.read_text())
            if (
                not isinstance(data, dict)
                or data.get("version") != 1
                or not isinstance(data.get("status"), str)
                or data["status"] not in totals
            ):
                raise ValueError(f"invalid audit record: {path}")
            totals[data["status"]] += 1
    except (OSError, ValueError, AttributeError) as error:
        parser.exit(2, f"cannot summarize audit: {error}\n")
    print(json.dumps(totals, indent=2))
    return 0
