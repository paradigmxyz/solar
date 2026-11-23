"""Structural identity of independently exported reachable EVM source graphs.

This route never erases an operation or transfer check to match graphs. It
quantifies shared node handlers and every execution bound. The constructor
binding and Rust CFG normalizer remain trusted; no opcode implementation or
physical gas/layout certificate is claimed. Code-image reads and calls remain
excluded. A failed data-identity proof is unsupported, never a counterexample.
"""

import hashlib
import json
import re
import tempfile
from pathlib import Path

from . import audit, correspondence, native, regions
from .common import Program, Unsupported, literal


def render(model, catalog, version, entry, *, dynamic_exit=False, max_growth=1):
    if not isinstance(model, dict) or not isinstance(model.get("nodes"), list):
        raise Unsupported("invalid source-identity graph")
    rows = model.get("nodes", [])
    if (
        set(model) != {"entry", "nodes"}
        or not 1 <= len(rows) <= 4096
        or any(
            not isinstance(row, dict) or type(row.get("id")) is not int for row in rows
        )
        or [row.get("id") for row in rows] != list(range(len(rows)))
    ):
        raise Unsupported("invalid source-identity graph indices")

    def target(value):
        if type(value) is not int or not 0 <= value < len(rows):
            raise Unsupported("invalid source-identity graph edge")
        return value

    root = target(model["entry"])
    if rows[root].get("label") != int(entry[2:]) or any(
        row.get("label") == int(entry[2:]) for row in rows[:root]
    ):
        raise Unsupported("source-identity graph changed the selected entry")
    nodes = []
    for row in rows:
        if (
            set(row) != {"id", "label", "node"}
            or type(row["label"]) is not int
            or not 0 <= row["label"] < 2**32
        ):
            raise Unsupported("invalid source-identity node label")
        node, label = row["node"], row["label"]
        if not isinstance(node, dict):
            raise Unsupported("invalid source-identity node")
        kind = node.get("kind")
        if kind == "operation" and set(node) == {"kind", "operation", "next"}:
            operation = node["operation"]
            if not isinstance(operation, str):
                raise Unsupported("invalid source-identity instruction")
            name = operation
            if operation.startswith("push "):
                literal(operation[5:])
                name = (
                    "push0"
                    if operation == "push 0"
                    and catalog["push0"]["forks"][version]["available"]
                    else "push1"
                )
            elif match := re.fullmatch(r"(dup|swap) (\d+)", operation):
                name = match[1] + match[2]
            definition = catalog.get(name)
            if (
                definition is None
                or not definition["forks"][version]["available"]
                or definition["observes_code"]
                or definition["sets_returndata"]
                or definition["stack"] is None
                or name in {"jump", "jumpi"}
            ):
                raise Unsupported(
                    "source identity requires code-image/call correspondence"
                )
            nodes.append(
                f".operation {label} {json.dumps(operation)} {target(node['next'])}"
            )
        elif (
            kind == "jump"
            and set(node) == {"kind", "growth", "next"}
            and type(node["growth"]) is int
            and 0 <= node["growth"] <= max_growth
        ):
            nodes.append(f".jump {label} {node['growth']} {target(node['next'])}")
        elif kind == "branch" and set(node) == {"kind", "yes", "no"}:
            nodes.append(f".branch {label} {target(node['yes'])} {target(node['no'])}")
        elif (
            kind == "terminal"
            and set(node) == {"kind", "operation"}
            and (
                node["operation"] in {"stop", "invalid", "return", "revert"}
                or dynamic_exit
                and node["operation"] == "jump"
            )
        ):
            nodes.append(f".terminal {label} {json.dumps(node['operation'])}")
        else:
            raise Unsupported("unsupported source-identity node")
    return f"⟨{root},#[{','.join(nodes)}]⟩"


def parse_pair(paths, solar, version, timeout, entry):
    from .check import ROOT, run

    if not re.fullmatch(r"bb[0-9]{1,10}", entry) or int(entry[2:]) >= 2**32:
        raise Unsupported("source identity requires a valid region label")
    texts, headers = [], []
    for path in paths:
        if path.suffix != ".evmir" or path.stat().st_size > 1024 * 1024:
            raise Unsupported("source identity requires EVM IR files of at most 1 MiB")
        parsed = run(
            [
                solar,
                path.resolve(),
                "-Zevm-ir-pipeline=none",
                "-Zvalidate-ir=true",
                f"--evm-version={version}",
            ],
            ROOT,
            timeout,
        )
        if parsed.returncode:
            raise Unsupported(parsed.stderr)
        text = parsed.stdout
        texts.append(text)
        header = re.split(r"(?m)^bb\d+", text, maxsplit=1)[0]
        headers.append(
            [line for line in header.splitlines() if not line.startswith(";")]
        )
    if headers[0] != headers[1]:
        raise Unsupported("source identity changed shared declarations")
    return texts


def check(paths, solar, replay, version, timeout, entry):
    from .check import PRELUDE, ROOT, build_model, run

    catalog = {row["name"]: row for row in native.catalog(replay, timeout)["evmir"]}
    texts = parse_pair(paths, solar, version, timeout, entry)
    try:
        models = [
            correspondence.export(text, entry, version, replay, timeout)
            for text in texts
        ]
    except Unsupported as error:
        if str(error) != "EVM source graph: escaping label in EVM source graph":
            raise
        from . import continuation_flow

        return continuation_flow.check(texts, replay, catalog, version, timeout, entry)
    dynamic_exit = any(
        row["node"] == {"kind": "terminal", "operation": "jump"}
        for row in models[0]["nodes"]
    )
    if models[0] != models[1] or dynamic_exit:
        from . import cfg_flow, stack_layout

        expressions = [
            render(model, catalog, version, entry, dynamic_exit=True)
            for model in models
        ]
        try:
            return stack_layout.check_data(
                texts, models, expressions, entry, catalog, version, timeout
            )
        except Unsupported:
            return cfg_flow.check(
                texts, models, expressions, entry, catalog, version, timeout
            )
    expressions = [render(model, catalog, version, entry) for model in models]
    code = (
        PRELUDE
        + f"""
def before : EVM.SourceGraph.Program := {expressions[0]}
def after : EVM.SourceGraph.Program := {expressions[1]}
theorem same : before = after := by rfl
theorem contextual {{State Observation : Type}}
 (handlers : EVM.SourceGraph.Handlers State Observation) (fuel : Nat) (state : State) :
 Correspondence.evalGraph (EVM.SourceGraph.graph before handlers) fuel before.entry state =
 Correspondence.evalGraph (EVM.SourceGraph.graph after handlers) fuel after.entry state :=
 EVM.SourceGraph.identity before after same handlers fuel state
#print axioms contextual
"""
    )
    built = build_model(timeout)
    if built.returncode:
        raise Unsupported(built.stdout + built.stderr)
    with tempfile.TemporaryDirectory(prefix="solar-tv-source-identity-") as folder:
        proof = Path(folder) / "Pair.lean"
        proof.write_text(code)
        result = run(["lake", "env", "lean", proof], ROOT, timeout)
    if (
        result.returncode
        or "'contextual' does not depend on any axioms" not in result.stdout
    ):
        raise Unsupported(
            "source-identity proof did not close\n" + result.stdout + result.stderr
        )
    programs = []
    for text, model in zip(texts, models, strict=True):
        selected = {f"bb{row['label']}" for row in model["nodes"]}
        projection = (
            "@module SourceIdentity\n"
            + "\n".join(
                f"{label}:\n" + "\n".join(body)
                for label, body in regions.blocks(text).items()
                if label in selected
            )
            + "\n"
        )
        programs.append(
            Program(
                ("evm-source-identity", entry),
                [],
                "",
                "",
                canonical_ir=projection,
                replay_ir=text,
                selector=entry,
                scope="source-identity",
                evm_version=version,
                import_evidence={
                    "kind": "independent-source-graph-identity",
                    "graph_sha256": hashlib.sha256(
                        json.dumps(model, sort_keys=True).encode()
                    ).hexdigest(),
                    "nodes": len(model["nodes"]),
                    "entry": model["entry"],
                    "boundary": "trusted Rust parser/CFG normalizer and source-data constructor binding; arbitrary shared node handlers; no opcode implementation, physical gas or backend layout certificate",
                },
            )
        )
    return audit.Result(
        "verified", stdout="verified\n", program=programs[0], pair=tuple(programs)
    )
