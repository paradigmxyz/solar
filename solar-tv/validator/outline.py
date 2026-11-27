"""Check a shared EVM prefix extracted through a stack-carried return."""

import hashlib
import json
import re
import tempfile
from pathlib import Path

from . import (
    audit,
    cfg_flow,
    entry_stack,
    evm_effects,
    evm_schedule,
    native,
    prefix,
    regions,
    source_identity,
)
from .common import Program, Unsupported, literal

LABEL_PUSH = re.compile(r"push (bb\d+)")
ROTATE = ["swap 1", "swap 2", "swap 3"]


def windows(texts, entry, version, replay, timeout):
    """Bind each call, return and resumed conditional via the independent export."""
    old, new = map(regions.blocks, texts)
    labels = list(old)
    if list(new)[: len(labels)] != labels or entry not in old:
        raise Unsupported("outline changed the original block order")
    changed = [label for label in old if old[label] != new[label]]
    if entry not in changed:
        raise Unsupported("selected region is not an outline site")
    added = set(new) - old.keys()
    for label in changed:
        if len(new[label]) < 2 or not new[label][-1].startswith("jump bb"):
            raise Unsupported("outline site needs an explicit helper transfer")
    helper_names = {new[label][-1][5:] for label in changed}
    if len(helper_names) != 1 or not helper_names <= added:
        raise Unsupported("outline sites must share one fresh helper")
    (helper,) = helper_names
    continuation_names = [LABEL_PUSH.fullmatch(new[label][-2]) for label in changed]
    if not all(continuation_names):
        raise Unsupported("outline site needs a literal continuation")
    continuations = [match[1] for match in continuation_names]
    if set(continuations) | {helper} != added or len(set(continuations)) != len(
        changed
    ):
        raise Unsupported("outline targets are not distinct fresh blocks")
    if helper in continuations:
        raise Unsupported("outline continuation is the helper")
    # Fresh targets must be private to these specific transfers. The helper
    # and resumed suffixes must not fall through into new block order.
    for label, body in new.items():
        original = body[:-2] if label in changed else body
        if added.intersection(re.findall(r"\bbb\d+\b", " ".join(original))):
            raise Unsupported("outline added another reference to a fresh block")
    if not new[labels[-1]] or not re.fullmatch(
        r"(?:jump(?: bb\d+)?|stop|return|revert|invalid)", new[labels[-1]][-1]
    ):
        raise Unsupported("original fallthrough enters the fresh block list")

    def export(side, label):
        return prefix.export(texts[side], label, version, replay, timeout)

    outlined = export(1, helper)
    instructions = outlined["instructions"]
    if (
        outlined["terminal"] != "Op(86)"
        or len(instructions) < 5
        or instructions[-3:] != ROTATE
        or not LABEL_PUSH.fullmatch(instructions[-4])
    ):
        raise Unsupported("outline helper does not rotate its return over a branch")
    body, guard = instructions[:-4], instructions[-4]
    if guard[5:] not in old or old[guard[5:]] != new[guard[5:]]:
        raise Unsupported("outline changed the guard target")
    sites = {}
    for label, continuation in zip(changed, continuations, strict=True):
        before, call, resume = (
            export(0, label),
            export(1, label),
            export(1, continuation),
        )
        common = call["instructions"][:-1]
        suffix = resume["instructions"]
        if (
            call.get("jump_target") != int(helper[2:])
            or call["instructions"][-1:] != [f"push {continuation}"]
            or suffix[:1] != ["jumpi"]
            or before["instructions"] != common + body + [guard] + suffix
            or before.get("jump_target") is None
            or before.get("jump_target") != resume.get("jump_target")
            or before["terminal"] != resume["terminal"]
        ):
            raise Unsupported("outline changed a prefix, branch or resumed suffix")
        sites[label] = (common, before, call, resume)
    return body, guard, helper, outlined, sites


def steps(body, ops):
    """Propose each active stack explicitly and check each opcode once."""
    proof = ["  unfold before after"]
    labels = ["continuation"]
    slots = []

    def stack(words):
        return "(" + " :: ".join([*(f"({word})" for word in words), "stack"]) + ")"

    def scalar(op, old, new):
        proof.append(
            f"  rw [EVM.StackSchedule.expand_scalar (op := ({op})) "
            f"(stack := {stack(old)}) (next := {stack(new)}) "
            "(EVM.StackSchedule.step_fits rfl (by (try simp) <;> omega) (by (try simp) <;> omega))]"
        )

    scalar(".push continuation", [], labels)
    proof.append(
        f"  rw [EVM.StackSchedule.expand_reserve (stack := {stack(labels)}) "
        "(by (try simp) <;> omega)]"
    )
    for index, (line, op) in enumerate(zip(body, ops, strict=True)):
        if match := re.fullmatch(r"\.effect (\d+) (\d+) ([01])", op):
            opcode, inputs, outputs = map(int, match.groups())
            for bottom in ([], labels):
                proof.append(
                    f"  rw [EVM.StackSchedule.expand_effect (opcode := {opcode}) "
                    f"(inputs := {inputs}) (outputs := {outputs}) (stack := {stack(slots + bottom)}) "
                    "(by simp) (by (try simp) <;> omega) (by (try simp) <;> omega)]"
                )
            reply = f"reply{index}"
            outputs_word = str(bool(outputs)).lower()
            proof.extend(
                [
                    "  refine ⟨rfl, ?_⟩",
                    f"  intro {reply}",
                    (
                        f"  simp only [show (({outputs} : Nat) == 1) = {outputs_word} by decide, "
                        "Bool.false_eq_true, ite_false, ite_true, List.drop_succ_cons, List.drop_zero]"
                    ),
                ]
            )
            slots = ([reply] if outputs else []) + slots[inputs:]
            continue

        previous = slots.copy()
        if line.startswith("push "):
            slots.insert(0, literal(line[5:]))
        elif line.startswith("swap "):
            depth = int(line[5:])
            slots[0], slots[depth] = slots[depth], slots[0]
        elif line.startswith("dup "):
            slots.insert(0, slots[int(line[4:]) - 1])
        elif line in {"not", "iszero", "add", "and", "shr"}:
            inputs = 1 if line in {"not", "iszero"} else 2
            operands = " ".join(f"({value})" for value in slots[:inputs])
            slots = [f"Evm.{line} {operands}", *slots[inputs:]]
        else:
            raise Unsupported("outline uses an unsupported scalar operation")
        scalar(op[9:-1], previous, slots)
        scalar(op[9:-1], previous + labels, slots + labels)

    if len(slots) != 2:
        raise Unsupported("outline body did not produce two words and a branch label")
    scalar(".push branch", slots, ["branch", *slots])
    scalar(".push branch", slots + labels, ["branch", *slots, *labels])
    ready = ["branch", *slots, *labels]
    for depth in range(1, 4):
        previous = ready.copy()
        ready[0], ready[depth] = ready[depth], ready[0]
        scalar(f".swap {depth - 1}", previous, ready)
    proof.append(
        f"  rw [EVM.StackSchedule.expand_nil (stack := {stack(ready)}) (by (try simp) <;> omega),\n"
        f"      EVM.StackSchedule.expand_nil (stack := {stack(['branch', *slots])}) (by (try simp) <;> omega)]\n"
        "  simp [EVM.StackSchedule.Refines, returned]"
    )
    return "\n".join(proof)


def certificate(body, ops, cap, delta, version):
    before = ",".join([*ops, ".scalar (.push branch)"])
    after = ",".join(
        [
            ".scalar (.push continuation)",
            ".reserve 1",
            *ops,
            ".scalar (.push branch)",
            ".scalar (.swap 0)",
            ".scalar (.swap 1)",
            ".scalar (.swap 2)",
        ]
    )
    definitions = f"""
set_option maxHeartbeats 3500000
open EvmRules

def before (branch : Word) : List EVM.Effects.Op := [{before}]
def after (branch continuation : Word) : List EVM.Effects.Op := [{after}]

def returned (continuation : Word) : EVM.Effects.Exit → EVM.StackSchedule.Trace
  | .continuation (target :: stack) =>
    if target = continuation then .done (.live (.continuation stack)) else .fault
  | _ => .fault

private theorem matched (ctx : EVM.Context) (branch continuation : Word)
    (stack : EVM.Stack) (fits : stack.length ≤ {cap}) :
    EVM.StackSchedule.Refines
      (EVM.StackSchedule.expand ctx .{version} (returned continuation) (after branch continuation) stack)
      (EVM.StackSchedule.expand ctx .{version} (fun exit => .done (.live exit)) (before branch) stack) := by
"""
    return (
        definitions
        + steps(body, ops)
        + f"""

theorem checkedOutline {{World : Type}} (handler : EVM.StackSchedule.Handler World)
    (ctx : EVM.Context) (branch continuation : Word) (stack : EVM.Stack)
    (pc height : Nat)
    (reached : EVM.StackReachability.Reaches SourceFrame.graph SourceFrame.signatures pc height)
    (prefixBound : stack.length ≤ height + {delta})
    (world : World) (result : EVM.StackSchedule.Exit × World)
    (ran : EVM.StackSchedule.execute handler
      (EVM.StackSchedule.project (fun exit => .done (.live exit))
        (EVM.Effects.expand ctx .{version} (before branch) stack 0)) world = some result) :
    EVM.StackSchedule.execute handler
      (EVM.StackSchedule.project (returned continuation)
        (EVM.Effects.expand ctx .{version} (after branch continuation) stack 0)) world = some result := by
  have room := SourceFrame.entryFits reached
  rw [EVM.StackSchedule.project_expand] at ran ⊢
  exact EVM.StackSchedule.effects handler _ _ (matched ctx branch continuation stack (by omega)) world result ran
#print axioms checkedOutline
"""
    )


def check(paths, solar, replay, version, timeout, entry):
    from .check import PRELUDE, build_model
    from .entry_proof import check_axioms, success

    texts = source_identity.parse_pair(paths, solar, version, timeout, entry)
    body, guard, helper, outlined, sites = windows(
        texts, entry, version, replay, timeout
    )
    catalog = {op["name"]: op for op in native.catalog(replay, timeout)["evmir"]}
    ops, need, peak = evm_effects.instructions(body, catalog, version, composed=True)
    if need or len(ops) > 64:
        raise Unsupported("outline body must use only values it produces")
    peak = max(peak + 1, 4)
    caller = entry_stack.propose(texts[0], entry, replay, catalog, version, timeout)
    source_cap = max(caller.bounds)
    cap = max(
        source_cap,
        *(
            evm_schedule.height_after_prefix(source_cap, site[0], catalog, version)
            for site in sites.values()
        ),
    )
    if cap + peak > 1024:
        raise Unsupported("entry stack lacks room for the outline return")
    bound = entry_stack.certificate(
        "SourceFrame", caller, catalog, source_cap, chunked=True
    )
    definitions = certificate(body, ops, cap, cap - source_cap, version)
    success(build_model(timeout), "outline proof model")
    with tempfile.TemporaryDirectory(prefix="solar-tv-outline-") as folder:
        proved = cfg_flow.check_stages(
            Path(folder),
            PRELUDE
            + "noncomputable section\nset_option maxRecDepth 65536\nset_option maxHeartbeats 3500000\n",
            [bound, definitions],
            timeout,
        )
    axioms = [
        line
        for line in success(proved, "outline certificate").splitlines()
        if "'checkedOutline'" in line
    ]
    check_axioms(axioms, 1)
    evidence = {
        "kind": "kernel-checked-outlined-prefix",
        "checked_labels": sorted(sites),
        "helper": helper,
        "guard": guard[5:],
        "source_stack_bound": entry_stack.evidence(caller),
        "cut_stack_cap": cap,
        "source_sha256": hashlib.sha256(
            json.dumps([outlined, sites], sort_keys=True).encode()
        ).hexdigest(),
    }
    pair = tuple(
        Program(
            ("evm-outlined-prefix", "any"),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            selector=entry,
            scope="outlined-prefix",
            gas="unpriced-shared-interactions",
            evm_version=version,
            import_evidence=evidence,
        )
        for text in texts
    )
    return audit.Result("verified", stdout="verified\n", program=pair[0], pair=pair)
