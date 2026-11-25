"""Propose a CFG with stack-carried block addresses resolved by provenance.

The Rust export retains each label push and its actual destination. Lean
checks the proposed graph and stack shapes. This is an unpriced structural
proof with shared, position-independent world handlers. Code observers and
external messages remain outside the fragment.
"""

import hashlib
import json
import re

from . import (
    branch_layout,
    cfg_flow,
    code_relocations,
    correspondence,
    native,
    stack_layout,
)
from .common import Unsupported, literal
from .source_identity import INTERACTION_OPS, parse_pair


def edges_text(node):
    match node:
        case {"kind": "branch", "yes": yes, "no": no}:
            return f".branch {yes} {no}"
        case {"next": target}:
            return f".next {target}"
        case _:
            return ".terminal"


def instruction_role(
    operation, catalog, version, *, interactions=False, from_entry=False
):
    if operation.startswith("push "):
        literal(operation[5:])
        return ".ordinary 0 1", (0, 1)
    if from_entry and (
        stack_layout.immutable_push(operation) or code_relocations.operation(operation)
    ):
        return ".ordinary 0 1", (0, 1)
    if operation == "pop":
        return ".pop", ("pop", 0)
    if match := re.fullmatch(r"(dup|swap) (\d{1,2})", operation):
        kind, index = match[1], int(match[2])
        # The ordinary DUP1..16 and SWAP1..16 encodings are fork-independent.
        if 1 <= index <= 16:
            depth = index - 1 if kind == "dup" else index
            return f".{kind} {depth}", (kind, depth)
    definition = catalog.get(operation)
    if (
        definition is None
        or not definition["forks"][version]["available"]
        or (
            (definition["observes_code"] or definition["sets_returndata"])
            and not (interactions and operation in INTERACTION_OPS)
        )
        or definition["stack"] is None
        or operation in {"jump", "jumpi"}
    ):
        raise Unsupported(f"continuation proof excludes `{operation}`")
    inputs, outputs = definition["stack"]
    return f".ordinary {inputs} {outputs}", (inputs, outputs)


def source_rows(
    model, catalog, version, *, interactions, from_entry, indexed_literals=False
):
    source = []
    roles = {}
    for row in model["nodes"]:
        node = row["node"]
        match node:
            case {"kind": "operation", "operation": operation, "next": target}:
                op, roles[row["id"]] = instruction_role(
                    operation,
                    catalog,
                    version,
                    interactions=interactions,
                    from_entry=from_entry,
                )
                source.append(f".operation {json.dumps(operation)} ({op}) {target}")
            case {"kind": "push_label", "target": target, "next": successor}:
                source.append(f".pushLabel {target} {successor}")
            case {"kind": "jump", "growth": growth, "next": target}:
                source.append(f".jump {growth} {target}")
            case {
                "kind": "indexed_literal",
                "growth": growth,
                "index": selector,
                "targets": targets,
            } if indexed_literals:
                source.append(f".indexedLiteral {growth} {selector}#256 {targets}")
            case {"kind": "branch", "yes": yes, "no": no}:
                source.append(f".branch {yes} {no}")
            case {"kind": "terminal", "operation": "jump"}:
                source.append(".dynamicJump")
            case {"kind": "branch_address", "next": target}:
                source.append(f".branchAddress {target}")
            case {"kind": "branch_after_address", "no": otherwise}:
                source.append(f".branchAfterAddress {otherwise}")
            case {"kind": "terminal", "operation": operation} if operation in {
                "stop",
                "invalid",
                "return",
                "revert",
            }:
                inputs = catalog[operation]["stack"][0]
                roles[row["id"]] = (inputs, 0)
                source.append(f".terminal {json.dumps(operation)} {inputs}")
            case _:
                raise Unsupported("unmodeled terminal or edge in continuation proof")
    return source, roles


def project(
    model,
    catalog,
    version,
    *,
    interactions=False,
    from_entry=False,
    mask=(),
    indexed_literals=False,
):
    source, roles = source_rows(
        model,
        catalog,
        version,
        interactions=interactions,
        from_entry=from_entry,
        indexed_literals=indexed_literals,
    )

    points = []
    indices = {}
    output = []
    returns = 0
    branches = 0
    tables = []
    handoffs = 0

    limit = 16384 if from_entry else 2048

    def index(pc, shape):
        if len(shape) > 1024:
            raise Unsupported("continuation provenance exceeds the stack limit")
        key = pc, shape
        if key not in indices:
            if len(points) >= limit:
                raise Unsupported(f"continuation provenance exceeds {limit} states")
            indices[key] = len(points)
            points.append(key)
        return indices[key]

    def check_inputs(shape, count):
        if any(tag is not None for tag in shape[:count]):
            raise Unsupported("continuation label escapes into an observable operand")

    source_size = len(model["nodes"])

    def branch_target(shape):
        if not shape or shape[0] is None or shape[0] >= source_size:
            raise Unsupported("dynamic branch has no resolved local stack label")
        check_inputs(shape[1:], 1)
        return shape[0]

    index(
        model["entry"],
        tuple(source_size + i if tagged else None for i, tagged in enumerate(mask)),
    )
    for pc, shape in points:
        node = dict(model["nodes"][pc]["node"])
        match node["kind"]:
            case "operation":
                kind, size = roles[pc]
                match kind:
                    case "dup":
                        moved = (shape[size] if size < len(shape) else None, *shape)
                    case "swap":
                        padded = list(shape) + [None] * max(0, size + 1 - len(shape))
                        padded[0], padded[size] = padded[size], padded[0]
                        moved = tuple(padded)
                    case "pop":
                        moved = shape[1:]
                    case _:
                        check_inputs(shape, kind)
                        moved = (None,) * size + shape[kind:]
                end = len(moved)
                while end and moved[end - 1] is None:
                    end -= 1
                node["next"] = index(node["next"], moved[:end])
            case "push_label":
                node = {
                    "kind": "operation",
                    "operation": "push 0",
                    "next": index(node["next"], (node["target"], *shape)),
                }
            case "jump":
                node["next"] = index(node["next"], shape)
            case "indexed_literal":
                target = node["targets"][node["index"]]
                node = {
                    "kind": "jump",
                    "growth": 1 + node["growth"],
                    "next": index(target, shape),
                }
                tables.append(len(output))
            case "branch":
                check_inputs(shape, 1)
                node["yes"] = index(node["yes"], shape[1:])
                node["no"] = index(node["no"], shape[1:])
            case "branch_address":
                branch_target(shape)
                node = {
                    "kind": "operation",
                    "operation": "pop",
                    "next": index(node["next"], shape),
                }
            case "branch_after_address":
                node = {
                    "kind": "branch",
                    "yes": index(branch_target(shape), shape[2:]),
                    "no": index(node["no"], shape[2:]),
                }
                branches += 1
            case "terminal":
                if node["operation"] == "jump":
                    if not shape or shape[0] is None:
                        raise Unsupported("dynamic return has no resolved stack label")
                    if shape[0] >= source_size:
                        node["operation"] = f"resume {shape[0] - source_size}"
                        handoffs += 1
                    else:
                        node = {
                            "kind": "operation",
                            "operation": "pop",
                            "next": index(shape[0], shape[1:]),
                        }
                        returns += 1
                else:
                    check_inputs(shape, roles[pc][0])
        output.append({"id": len(output), "label": 0, "node": node})

    point_terms = [f"⟨{pc},{shape_term(shape)}⟩" for pc, shape in points]
    projected = {"entry": 0, "nodes": output}
    return projected, {
        "entry": model["entry"],
        "source": source,
        "points": point_terms,
        "point_data": points,
        "edges": [edges_text(row["node"]) for row in output],
        "label_pushes": {
            i
            for i, (pc, _) in enumerate(points)
            if model["nodes"][pc]["node"]["kind"] == "push_label"
        },
        "block_labels": {model["nodes"][pc]["label"] for pc, _ in points},
        "indexed_tables": tables,
        "evidence": {
            "source_sha256": hashlib.sha256(
                json.dumps(model, sort_keys=True).encode()
            ).hexdigest(),
            "provenance_states": len(points),
            "resolved_returns": returns,
            **({"resolved_branches": branches} if branches else {}),
            **({"resolved_literal_tables": len(tables)} if tables else {}),
            **({"inherited_returns": handoffs} if mask else {}),
        },
    }


def shape_term(shape):
    return (
        "[" + ",".join("none" if tag is None else f"some {tag}" for tag in shape) + "]"
    )


def check(texts, replay, catalog, version, timeout, entry, *, interactions=False):
    models = [
        correspondence.export(text, entry, version, replay, timeout, labels=True)
        for text in texts
    ]
    projected, certificates = zip(
        *(
            project(model, catalog, version, interactions=interactions)
            for model in models
        ),
        strict=True,
    )
    if interactions:
        for model, witness in zip(projected, certificates, strict=True):
            requests = {
                row["node"].get("operation") for row in model["nodes"]
            } & INTERACTION_OPS
            if not requests:
                raise Unsupported("interaction trace requires a reachable interaction")
            witness["evidence"]["interactions"] = sorted(requests)
    elif not any(
        c["evidence"]["resolved_returns"] or c["evidence"].get("resolved_branches")
        for c in certificates
    ):
        raise Unsupported("continuation proof needs a resolved dynamic jump")
    return cfg_flow.check(
        texts,
        projected,
        entry,
        catalog,
        version,
        timeout,
        continuations=tuple(zip(models, certificates, strict=True)),
        interactions=interactions,
    )


def check_entry(
    paths, solar, replay, version, timeout, *, region=None, interactions=True
):
    from . import entry_stack

    texts = parse_pair(paths, solar, version, timeout, region)
    catalog = {row["name"]: row for row in native.catalog(replay, timeout)["evmir"]}
    callers = None
    mask = ()
    if region is not None:
        callers = tuple(
            entry_stack.propose(text, region, replay, catalog, version, timeout)
            for text in texts
        )
        mask = callers[0].mask()
        if mask != callers[1].mask():
            raise Unsupported(
                "the two physical entries disagree on the region's return slots"
            )
    sources = [
        correspondence.export(text, region, version, replay, timeout, labels=True)
        for text in texts
    ]
    code_layout = code_relocations.bind(
        sources, texts, solar, version, timeout, catalog
    )
    normalized, sites = branch_layout.propose(*sources)
    projected, witnesses = zip(
        *(
            project(
                source,
                catalog,
                version,
                interactions=interactions,
                from_entry=True,
                mask=mask,
                indexed_literals=region is None,
            )
            for source in normalized
        ),
        strict=True,
    )
    for raw, model, witness, comparisons in zip(
        sources, projected, witnesses, sites, strict=True
    ):
        witness["evidence"]["interactions"] = sorted(
            {row["node"].get("operation") for row in model["nodes"]} & INTERACTION_OPS
        )
        if comparisons:
            original, _ = source_rows(
                raw,
                catalog,
                version,
                interactions=interactions,
                from_entry=True,
                indexed_literals=region is None,
            )
            witness["comparison"] = original, comparisons
            witness["evidence"]["checked_branch_tests"] = len(comparisons)
            witness["evidence"]["original_graph_sha256"] = hashlib.sha256(
                json.dumps(raw, sort_keys=True).encode()
            ).hexdigest()
    entry = sources[0]["nodes"][sources[0]["entry"]]["label"]
    return cfg_flow.check(
        texts,
        projected,
        region or f"bb{entry}",
        catalog,
        version,
        timeout,
        continuations=tuple(zip(sources, witnesses, strict=True)),
        interactions=interactions,
        from_entry=region is None,
        code_layout=code_layout,
        callers=callers,
        mask=mask,
    )
