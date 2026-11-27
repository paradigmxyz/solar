"""Check merged guards between otherwise unchanged typed SSA actions."""

import subprocess
import sys
from collections import defaultdict

from . import abi_effects, dead_ssa, mir_effects, mir_schedule, native, prune
from .common import Unsupported, literal


def without_transfers(body):
    return {
        **body,
        "blocks": [
            {**block, "terminator": {"kind": "unreachable"}} for block in body["blocks"]
        ],
    }


def successors(term):
    match term["kind"]:
        case "jump":
            return [term["target"]]
        case "branch":
            return [term["then"], term["else"]]
        case "switch":
            return [term["default"], *(block for _, block in term["cases"])]
    return []


def immutable_read(inst, definitions, fork):
    op = inst["operation"]
    if op["kind"] == "layout" and op["query"] in {"heap_floor", "constructor"}:
        return (
            not op["args"]
            and inst["result"] is not None
            and inst["type"]["bits"] == 256
        )
    if op["kind"] != "opcode" or op["name"] != "callvalue":
        return mir_schedule.state_independent(inst, definitions, fork)
    row = definitions["callvalue"]
    return (
        row["forks"][fork]["available"]
        and inst["result"] is not None
        and row["stack"] == [len(op["args"]), 1]
    )


def align(before, after):
    # The target can contain additional guard computations. Propose an
    # injective mapping for the common actions first, then restore the actual
    # branches in that same register numbering.
    _, added, mapping = dead_ssa.align(
        without_transfers(after), without_transfers(before)
    )
    return dead_ssa.rename_locals(before, mapping)["blocks"], set(added)


def guard(term, reject):
    if term["kind"] != "branch" or successors(term).count(reject) != 1:
        raise Unsupported("abort merge requires a distinct rejection edge")
    return term["args"][0], term["then"] == reject


def chains(before, after, added, definitions, fork):
    old_parents, new_parents = defaultdict(set), defaultdict(set)
    changes, bypasses = set(), {}
    for i, (old, new) in enumerate(zip(before, after, strict=True)):
        a, b = old["terminator"], new["terminator"]
        for target in successors(a):
            old_parents[target].add(i)
        for target in successors(b):
            new_parents[target].add(i)
        if a == b:
            if any(inst["result"] in added for inst in new["instructions"]):
                raise Unsupported("new abort guard instruction outside a changed block")
            continue
        changes.add(i)
        if a["kind"] == "branch" and b["kind"] == "jump":
            if successors(a).count(b["target"]) != 1:
                raise Unsupported("abort merge changes the continuing target")
            bypasses[i] = b["target"]
    covered, regions = set(), []
    for start in sorted(set(bypasses) - set(bypasses.values())):
        path, source_guards = [], []
        i = start
        reject = next(
            target
            for target in successors(before[i]["terminator"])
            if target != bypasses[i]
        )
        while True:
            if i in covered or i not in changes:
                raise Unsupported("overlapping or incomplete abort guard chain")
            covered.add(i)
            path.append(i)
            source_guards.append(guard(before[i]["terminator"], reject))
            if i not in bypasses:
                break
            next_ = bypasses[i]
            if (
                next_ == 0
                or old_parents[next_] != {i}
                or new_parents[next_] != {i}
                or before[next_]["phis"]
            ):
                raise Unsupported("abort guard chain has another entry or a phi")
            i = next_
        last = path[-1]
        target_guard = guard(after[last]["terminator"], reject)
        if set(successors(before[last]["terminator"])) != set(
            successors(after[last]["terminator"])
        ):
            raise Unsupported("abort guard chain changed its final destinations")
        abort = before[reject]
        if (
            reject in path
            or abort != after[reject]
            or abort["phis"]
            or abort["terminator"]["kind"]
            not in {"revert", "revert_return_data", "invalid", "stop", "tail_call"}
        ):
            raise Unsupported("abort guard chain needs the same terminal rejection")
        speculative = [
            inst for index in path[1:] for inst in before[index]["instructions"]
        ]
        if any(not immutable_read(inst, definitions, fork) for inst in speculative):
            raise Unsupported("abort guard chain speculates a non-total instruction")
        dead_ssa.reads(speculative)
        if set(dead_ssa.reads(abort, reject_undefined=False)) & {
            inst["result"] for inst in speculative
        }:
            raise Unsupported("abort continuation can read speculative definitions")
        extra = {
            inst["result"]: inst
            for index in path
            for inst in after[index]["instructions"]
            if inst["result"] in added
        }
        if any(
            not mir_schedule.state_independent(inst, definitions, fork)
            for inst in extra.values()
        ):
            raise Unsupported(
                "new guard computation must be total and state-independent"
            )
        new_reads = dead_ssa.reads([*extra.values(), target_guard[0]])
        if set(new_reads) & (added - extra.keys()):
            raise Unsupported("merged guard reads a new local outside its own chain")
        regions.append((path, source_guards, target_guard, extra))
    if not covered or covered != changes:
        raise Unsupported("changed MIR transfers are not disjoint abort guard chains")
    return regions


def candidate(paths, function):
    if any(
        path.suffix != ".mir" or path.stat().st_size > 1024 * 1024 for path in paths
    ):
        return False
    counts = []
    for path in paths:
        _, entries = prune.declarations(path.read_text())
        if function not in entries:
            return False
        counts.append(
            sum(
                line.lstrip().startswith("jumpi ") for line in entries[function]["text"]
            )
        )
    return counts[0] > counts[1]


def guard_equation(name, source, target, original, extra, definitions):
    """Compare the guard wiring with arbitrary values for shared producers."""
    variables = {}
    producers = original | extra

    def value(operand):
        width = mir_effects.scalar_width(operand["type"])
        kind = operand["kind"]
        if kind == "constant":
            return literal(operand["value"], width), width
        if kind == "undefined":
            raise Unsupported("undefined operand in merged guard")
        id_ = operand.get("index") if kind == "local" else None
        inst = producers.get(id_)
        if inst:
            op = inst["operation"]
            if (
                id_ in extra
                or op["kind"] in {"ne", "word"}
                or op.get("name") in {"or", "and", "eq", "xor"}
            ):
                args = [value(arg) for arg in op["args"]]
                expression = mir_effects.pure(op, args, width, definitions)
                if expression is None:
                    raise Unsupported("unmodeled operation in merged guard")
                return f"({expression})", width
        key = dead_ssa.stable(operand)
        return variables.setdefault(key, (f"word{len(variables)}", width))

    def rejection(guard):
        operand, rejects_on_true = guard
        expression, width = value(operand)
        if width != 1:
            raise Unsupported("merged guard requires a one-bit condition")
        comparison = "=" if rejects_on_true else "≠"
        return f"decide ({expression} {comparison} 1#1)"

    early = "[" + ", ".join(map(rejection, source)) + "]"
    late = rejection(target)
    binders = " ".join(f"({arg} : BitVec {bits})" for arg, bits in variables.values())
    args = " ".join(arg for arg, _ in variables.values())
    return f"""
private theorem {name}Guard {binders} :
    ({early} : List Bool).any id = {late} := by
  apply Bool.eq_iff_iff.mpr
  simp [List.any_cons, MIR.eval, MIR.WidthTestFacts.cmp_ne,
    MIR.WidthTestFacts.cmp_eq, MIR.WidthTestFacts.flag_or, MIR.WidthTestFacts.flag_and,
    BitVec.or_eq_zero_iff, MIR.WidthTestFacts.widen_zero, MIR.WidthTestFacts.flag_zero,
    Classical.not_and_iff_not_or_not, not_or, or_comm, or_left_comm, or_assoc,
    and_comm, and_left_comm, and_assoc]
theorem {name} {{Frame Observation : Type}} {binders}
    (stages : List (MIR.MergeAborts.Stage Frame)) (frame : Frame)
    (inputs : (MIR.MergeAborts.collect stages frame).2 = {early})
    (observe : Option Frame → Observation) :
    observe (MIR.MergeAborts.early stages frame) =
      observe (MIR.MergeAborts.late stages frame ({late})) :=
  MIR.MergeAborts.context stages frame ({late}) (by simpa only [inputs] using {name}Guard {args}) observe
#print axioms {name}
"""


def proof(old, new, definitions, fork, prelude):
    left, right = [
        {body["id"]: body for body in model["functions"]} for model in (old, new)
    ]
    if old["entry_id"] != new["entry_id"] or left.keys() != right.keys():
        raise Unsupported("abort merge changed the reachable call graph")
    code = (
        prelude
        + """
#print axioms MIR.Schedule.calldata_load
#print axioms MIR.Schedule.calldata_size
#print axioms MIR.Schedule.address
#print axioms MIR.MergeAborts.callvalue
#print axioms MIR.MergeAborts.heap_floor
#print axioms MIR.MergeAborts.constructor_range
"""
    )
    counts, placements = {}, set()
    for id_, body in left.items():
        changed = right[id_]
        if body == changed:
            continue
        before, added = align(body, changed)
        after = changed["blocks"]
        groups = chains(before, after, added, definitions, fork)
        counts[id_] = len(groups)
        for path, source, target, extra in groups:
            common = {
                inst["result"]: inst
                for index in path
                for inst in before[index]["instructions"]
                if inst["result"] is not None
            }
            placements.update(
                inst["operation"]["query"]
                for index in path[1:]
                for inst in before[index]["instructions"]
                if inst["operation"]["kind"] == "layout"
            )
            code += guard_equation(
                f"abortBody{id_}Block{path[0]}",
                source,
                target,
                common,
                extra,
                definitions,
            )
    if not counts:
        raise Unsupported("no changed abort guard chains")
    return code, counts, sorted(placements)


def check(paths, solar, replay, version, timeout, function, *, verbose=False):
    from .check import PRELUDE

    try:
        if not candidate(paths, function):
            return None
        texts, _, modules = abi_effects.typed_pair(
            paths, solar, replay, version, timeout
        )
        left, right = [
            {key: value for key, value in module.items() if key != "functions"}
            for module in modules
        ]
        if left != right:
            raise Unsupported("abort merge changed module entry bindings")
        old, new = [
            abi_effects.selected_body(module, function, tail_calls=True)[0]
            for module in modules
        ]
        definitions = {
            row["name"]: row for row in native.catalog(replay, timeout)["evmir"]
        }
        code, counts, layouts = proof(old, new, definitions, version, PRELUDE)
        return abi_effects.certificate(
            code,
            texts,
            function,
            version,
            timeout,
            {
                "kind": "kernel-checked-mir-abort-merge",
                "guard_chains": counts,
                "speculative_layout_queries": layouts,
            },
            scope="mir-abort-merge",
        )
    except (Unsupported, OSError, subprocess.TimeoutExpired) as error:
        if verbose:
            print(f"MIR abort merge: {error}", file=sys.stderr)
        return None
