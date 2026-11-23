"""Contextual certificates for copy erasure in late MIR.

The two original modules are parsed independently. Projection gives scalar
operands their word carriers and retains each effect and control operand.
Lean checks the exact erased graph and a must-agree invariant across its
edges. The unpriced request semantics retains GAS and placement queries.
"""

import json
import re
import tempfile
from dataclasses import dataclass
from pathlib import Path

from . import dead_ssa, mir_state, phi_edges
from .common import Unsupported


@dataclass(frozen=True)
class Action:
    key: int
    reads: tuple = ()
    writes: tuple = ()


@dataclass(frozen=True)
class Copy:
    id: int
    source: tuple


@dataclass
class Block:
    phis: dict
    code: list
    exit: Action
    targets: list


def intern(table, item):
    return table.setdefault(dead_ssa.stable(item), len(table))


def scalar_type(value):
    if value["kind"] == "pointer":
        mir_state.value_type(value)
        return {"kind": "word", "bits": 256, "name": "i256"}
    if value["kind"] != "word":
        raise Unsupported("carrier proof requires scalar MIR")
    mir_state.width(value["bits"])
    return value


def source_copy(inst):
    """The schema exports inttoptr as word and ptrtoint as low_bits."""
    operation = inst["operation"]
    if operation["kind"] not in {"word", "low_bits"} or len(operation["args"]) != 1:
        return None
    operand = operation["args"][0]
    source, target = operand["type"], inst["type"]
    if (
        inst["result"] is not None
        and source.get("bits") == target.get("bits") == 256
        and (source["kind"] == "pointer" or target["kind"] == "pointer")
        and operation.get("bits", 256) == 256
    ):
        scalar_type(source)
        scalar_type(target)
        return operand
    return None


def tail_sites(before, after, left, right):
    """Find call/terminal pairs with a retained, straight-line revert callee."""
    sites = {}
    for index, (a, b) in enumerate(zip(before["blocks"], after["blocks"])):
        term = b["terminator"]
        if term["kind"] != "tail_call" or a["terminator"]["kind"] not in {
            "invalid",
            "stop",
        }:
            continue
        if not a["instructions"]:
            raise Unsupported("abort tail requires a final direct call")
        call = a["instructions"][-1]
        target = term["function"]
        if (
            call["result"] is not None
            or call["operation"]["kind"] != "call"
            or call["operation"]["function"] != target
            or target not in left
            or left[target] != right[target]
        ):
            raise Unsupported("abort tail changed the call binding or its helper")
        sites[index] = target
    return sites


def abort_lemma(helper, definitions):
    blocks = helper["blocks"]
    if len(blocks) != 1 or blocks[0]["phis"] or helper["returns"]:
        raise Unsupported("abort tail requires a straight-line helper")
    block = blocks[0]
    term = block["terminator"]
    if term["kind"] != "revert":
        raise Unsupported("abort tail helper must revert")
    stores = []
    for inst in block["instructions"]:
        op = inst["operation"]
        if (
            op["kind"] != "opcode"
            or op["name"] != "mstore"
            or inst["result"] is not None
        ):
            raise Unsupported("abort tail helper has an unsupported instruction")
        a, b = map(mir_state.operand, op["args"])
        stores.append(f"⟨{a},{b}⟩")
    offset, size = map(mir_state.operand, term["args"])
    params = "[" + ",".join(map(mir_state.value_type, helper["params"])) + "]"
    index = helper["id"]
    name = json.dumps(helper["return_buffer"])
    return f"""
def abort{index} : MIR.AbortTail.Helper := ⟨[{",".join(stores)}],{offset},{size}⟩
def original{index} : Nat × MIR.State.Function := {mir_state.function_body(helper, definitions)}
theorem abort_shape{index} : original{index}.2 = abort{index}.body {params} {name} := rfl
theorem abort_entry{index} (cfg : MIR.State.Config) (functions : MIR.State.Functions)
    (binding : MIR.State.function functions {index} = .ok original{index}.2)
    (fuel : Nat) (stack : List MIR.State.Continuation) (owner : Option Nat)
    (frame : MIR.State.Frame) :
    MIR.State.run (fuel+1) cfg functions
      ⟨{{function := {index}, returnOwner := owner}},stack,frame⟩ =
      MIR.AbortTail.execute abort{index} cfg fuel 0 frame :=
  MIR.AbortTail.at_entry abort{index} cfg functions {index} {params} {name}
    (abort_shape{index} ▸ binding) fuel stack owner frame
theorem abort_never_returns{index} (cfg : MIR.State.Config) (fuel : Nat)
    (frame : MIR.State.Frame) (values : List EvmRules.Word) :
    ∀ result, MIR.AbortTail.execute abort{index} cfg fuel 0 frame = .ok result →
      result.completion ≠ .returned values :=
  MIR.AbortTail.execute_never_returns abort{index} cfg fuel 0 frame values
#print axioms abort_entry{index}
#print axioms abort_never_returns{index}
"""


def align(before, after, tail_rewrites, *, erase_both=False):
    if len(before["blocks"]) != len(after["blocks"]):
        raise Unsupported("carrier proof requires unchanged block identities")
    renaming = {}
    used = set()

    def bind(old, new):
        if old is None or new is None:
            if old != new:
                raise Unsupported("carrier proof changed a result binder")
        elif new in renaming or old in used:
            raise Unsupported("carrier SSA renaming is not injective")
        else:
            renaming[new] = old
            used.add(old)

    for index, (left, right) in enumerate(
        zip(before["blocks"], after["blocks"], strict=True)
    ):
        retained = [inst for inst in left["instructions"] if source_copy(inst) is None]
        targets = (
            [inst for inst in right["instructions"] if source_copy(inst) is None]
            if erase_both
            else right["instructions"]
        )
        if index in tail_rewrites:
            retained.pop()
        if len(retained) != len(targets) or len(left["phis"]) != len(right["phis"]):
            raise Unsupported("carrier proof changed the number of retained actions")
        for old, new in [
            *zip(left["phis"], right["phis"], strict=True),
            *zip(retained, targets, strict=True),
        ]:
            bind(old["result"], new["result"])
    if erase_both:
        # Assign the target's erased results fresh names so that retained
        # locals share the same names in both checked erasures.
        next_id = 1 + max(used, default=0)
        for block in after["blocks"]:
            for inst in block["instructions"]:
                if source_copy(inst) is not None:
                    bind(next_id, inst["result"])
                    next_id += 1
    return renaming


def project(body, symbols, renaming, tail_rewrites, *, retain_target_copies=False):
    def local(index):
        if renaming is None:
            return index
        if index not in renaming:
            raise Unsupported("carrier proof has an unbound target register")
        return renaming[index]

    def read(value):
        if value["kind"] == "local":
            return ("reg", local(value["index"]))
        if value["kind"] not in {"arg", "constant"}:
            raise Unsupported("carrier proof requires defined scalar operands")
        constant = {**value, "type": scalar_type(value["type"])}
        return ("fixed", intern(symbols, ["operand", constant]))

    def action(payload, writes=()):
        reads = []

        def visit(value):
            if isinstance(value, list):
                return [visit(item) for item in value]
            if not isinstance(value, dict):
                return value
            if value.get("kind") in {"pointer", "word", "product", "slice"} and (
                "bits" in value or "fields" in value or "location" in value
            ):
                return scalar_type(value)
            if "type" in value and value.get("kind") in {
                "local",
                "arg",
                "constant",
                "undefined",
            }:
                reads.append(read(value))
                return ["operand", scalar_type(value["type"])]
            return {key: visit(item) for key, item in sorted(value.items())}

        return Action(intern(symbols, ["action", visit(payload)]), tuple(reads), writes)

    blocks = []
    for index, block in enumerate(body["blocks"]):
        phis = {}
        if group := block["phis"]:
            incoming = [dict(phi["incoming"]) for phi in group]
            if any(
                arms.keys() != incoming[0].keys() or len(arms) != len(phi["incoming"])
                for arms, phi in zip(incoming, group, strict=True)
            ):
                raise Unsupported("inconsistent predecessor group in carrier proof")
            for pred in sorted(incoming[0]):
                phis[pred] = action(
                    {"phis": [arms[pred] for arms in incoming]},
                    tuple(local(phi["result"]) for phi in group),
                )
        code = []
        instructions = block["instructions"]
        term = block["terminator"]
        if index in tail_rewrites:
            # The original continuation is STOP or INVALID. The checked abort
            # body cannot reach it. Keep the invocation and its arguments.
            term = {**instructions[-1]["operation"], "kind": "tail_call"}
            instructions = instructions[:-1]
        for inst in instructions:
            if (renaming is None or retain_target_copies) and (
                operand := source_copy(inst)
            ) is not None:
                code.append(Copy(local(inst["result"]), read(operand)))
                continue
            result = inst["result"]
            code.append(
                action(
                    {"operation": inst["operation"], "type": inst["type"]},
                    () if result is None else (local(result),),
                )
            )
        blocks.append(Block(phis, code, action(term), phi_edges.targets(term)))
    return blocks


def aliases_and_live(blocks):
    aliases = {
        step.id: step.source
        for block in blocks
        for step in block.code
        if isinstance(step, Copy)
    }
    for dest, source in aliases.items():
        seen = {dest}
        while source[0] == "reg" and source[1] in aliases:
            if source[1] in seen:
                raise Unsupported("cyclic carrier copy definitions")
            seen.add(source[1])
            source = aliases[source[1]]
        aliases[dest] = source

    def execute(code, active):
        active = set(active)
        for step in code:
            if isinstance(step, Copy):
                active.add(step.id)
            else:
                written = {("reg", reg) for reg in step.writes}
                active = {i for i in active if aliases[i] not in written}
        return active

    live = [set(aliases) for _ in blocks]
    live[0].clear()
    # Intersect all incoming path states. Unknown or unreachable predecessors
    # remain conservative at the proof boundary. Lean checks the result.
    while True:
        changed = False
        for index, block in enumerate(blocks):
            entries = block.phis.values() if block.phis else [Action(0)]
            out = set.intersection(
                *(execute([phi, *block.code], live[index]) for phi in entries)
            )
            for target in block.targets:
                narrowed = live[target] & out
                if narrowed != live[target]:
                    live[target] = narrowed
                    changed = True
        if not changed:
            return aliases, [sorted(active) for active in live]


def render_input(source):
    return f".{source[0]} {source[1]}"


def render_action(action):
    reads = "[" + ",".join(map(render_input, action.reads)) + "]"
    return f"⟨{action.key},{reads},{list(action.writes)}⟩"


def render_block(block):
    phis = (
        "["
        + ",".join(f"({pred},{render_action(phi)})" for pred, phi in block.phis.items())
        + "]"
    )
    code = (
        "["
        + ",".join(
            f".copy {step.id} ({render_input(step.source)})"
            if isinstance(step, Copy)
            else f".action {render_action(step)}"
            for step in block.code
        )
        + "]"
    )
    return f"⟨{phis},{code},{render_action(block.exit)},{block.targets}⟩"


def paired_proof(index, old, new):
    renaming = align(old, new, {}, erase_both=True)
    symbols = {}
    graphs = (
        project(old, symbols, None, {}),
        project(new, symbols, renaming, {}, retain_target_copies=True),
    )
    names = ("before", "after")
    count = 0
    code = f"\nnamespace CarrierPair{index}\nopen MIR.CarrierCopies\n"
    for name, graph in zip(names, graphs, strict=True):
        aliases, live = aliases_and_live(graph)
        count += len(aliases)
        mapping = ",".join(
            f"({reg},{render_input(source)})" for reg, source in aliases.items()
        )
        code += f"""
def {name}Copies : Copies := [{mapping}]
def {name}Live : List (List Nat) := {live}
def {name} : List Block := [{",".join(map(render_block, graph))}]
"""
    if count == 0:
        raise Unsupported("no full-word carrier can be erased")
    code += f"""
theorem preserved {{Value State Terminal : Type}} (handlers : Handlers Value State Terminal)
    (fixed regs : Registers Value) (fuel : Nat) (state : State) :
    run handlers fixed before fuel 0 none regs state =
      run handlers fixed after fuel 0 none regs state :=
  paired_entry handlers fixed before after beforeCopies afterCopies beforeLive afterLive
    (by decide +kernel) (by decide +kernel) (by decide +kernel) fuel regs state
end CarrierPair{index}
#print axioms CarrierPair{index}.preserved
"""
    return code, count


def check(left, right, changed, definitions, timeout):
    from .check import PRELUDE, ROOT, build_model, run

    proof = (
        PRELUDE
        + """
open MIR.CarrierCopies
set_option maxRecDepth 16384
set_option maxHeartbeats 2000000
#print axioms MIR.PhaseShaping.pointer_carrier
#print axioms MIR.PhaseShaping.full_word_casts
"""
    )
    evidence = {}
    helpers = set()
    for index in sorted(changed):
        old, new = left[index], right[index]
        tail_rewrites = tail_sites(old, new, left, right)
        for helper in sorted(set(tail_rewrites.values()) - helpers):
            proof += abort_lemma(left[helper], definitions)
            helpers.add(helper)
        names = {}
        renaming = align(old, new, tail_rewrites)
        before = project(old, names, None, tail_rewrites)
        after = project(new, names, renaming, set())
        aliases, live = aliases_and_live(before)
        if not aliases and not tail_rewrites:
            raise Unsupported("no carrier or abort rewrite in changed body")
        mapping = (
            "["
            + ",".join(
                f"({reg},{render_input(source)})" for reg, source in aliases.items()
            )
            + "]"
        )
        proof += f"""
def copies{index} : Copies := {mapping}
def live{index} : List (List Nat) := {live}
def before{index} : List Block := [{",".join(map(render_block, before))}]
def after{index} : List Block := [{",".join(map(render_block, after))}]
theorem body{index} {{Value State Terminal : Type}} (handlers : Handlers Value State Terminal)
    (fixed regs : Registers Value) (fuel : Nat) (state : State) :
    run handlers fixed before{index} fuel 0 none regs state =
    run handlers fixed after{index} fuel 0 none regs state :=
  from_entry handlers fixed copies{index} live{index} before{index} after{index}
    (by decide) (by decide) fuel regs state
#print axioms body{index}
"""
        evidence[old["return_buffer"]] = {
            "kind": "carrier-graph",
            "copies": len(aliases),
            "blocks": len(before),
            "abort_tails": len(tail_rewrites),
        }
    if not evidence:
        raise Unsupported("no carrier graph changed")
    built = build_model(timeout)
    if built.returncode:
        raise Unsupported(built.stdout + built.stderr)
    with tempfile.TemporaryDirectory(prefix="solar-tv-carrier-graph-") as folder:
        path = Path(folder) / "Proof.lean"
        path.write_text(proof)
        result = run(["lake", "env", "lean", path], ROOT, timeout)
    theorems = (
        r"(?:(?:body|abort_entry|abort_never_returns)[0-9]+"
        r"|SolarTV\.MIR\.PhaseShaping\.(?:pointer_carrier|full_word_casts))"
    )
    axioms = re.findall(rf"'{theorems}' depends on axioms: \[(.*?)\]", result.stdout)
    no_axioms = re.findall(
        rf"'{theorems}' does not depend on any axioms", result.stdout
    )
    if (
        result.returncode
        or len(axioms) + len(no_axioms) != len(evidence) + 2 * len(helpers) + 2
        or any(
            set(a.split(", ")) - {"propext", "Quot.sound", "Classical.choice"}
            for a in axioms
        )
    ):
        raise Unsupported(
            "carrier graph proof did not close\n" + result.stdout + result.stderr
        )
    return evidence
