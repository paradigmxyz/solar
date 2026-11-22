"""Generate a proof of an explicit dispatcher against its implicit ABI routes."""

import re

from . import mir_state, mir_symbolic
from .common import Unsupported

ENTRY_ONLY = {"abi_wrapper", "external", "public", "payable", "pure", "view"}
ROUTING_OPS = {"callvalue", "calldataload", "calldatasize", "lt", "ne", "shr", "or"}
ROUTING_TERMS = {"jump", "branch", "switch", "tail_call", "revert"}


def retained_declaration(body):
    head, *rest = body["text"]
    attrs = re.fullmatch(r"(.*) \[(.*)\] \{", head)
    if attrs:
        keep = [
            word
            for word in attrs[2].split(", ")
            if not (word in ENTRY_ONLY or word.startswith("selector="))
        ]
        head = attrs[1] + (" [" + ", ".join(keep) + "]" if keep else "") + " {"
    return [head, *rest]


def proof(source, target, entry, definitions, version, before, retained):
    def remap(id_):
        return id_ + (id_ >= entry)

    selectors = sorted(source["selectors"], key=lambda row: row["selector"])
    receive, fallback = source["receive_route"], source["fallback_route"]
    wrappers = selectors + [row for row in (receive, fallback) if row is not None]
    if source["library"] or not all(row["wrapper"] for row in wrappers):
        raise Unsupported("routed dispatch requires explicit non-library ABI wrappers")
    if len({row["selector"] for row in selectors}) != len(selectors):
        raise Unsupported("duplicated implicit selector")
    if sorted(target["runtime_entries"]) != sorted(
        remap(row["id"]) for row in wrappers
    ):
        raise Unsupported("dispatcher tail-call targets differ from the source routes")
    for role, route in (("receive", receive), ("fallback", fallback)):
        if target[role] != (remap(route["id"]) if route else None):
            raise Unsupported(f"dispatcher changed the {role} root")
    if [(name, retained_declaration(body)) for name, body in before.items()] != [
        (name, body["text"]) for name, body in retained
    ]:
        raise Unsupported("dispatch materialization changed retained wrapper code")

    dispatcher = target["functions"][entry]
    # The interpreter's cut must only follow a finite, read-only routing prefix.
    # The generated Lean theorem checks the actually imported instructions.
    blocks = dispatcher["blocks"]
    operations = {
        op.get("name", op["kind"])
        for block in blocks
        for i in block["instructions"]
        for op in (i["operation"],)
    }
    if (
        len(blocks) > 64
        or any(
            block["phis"] or block["terminator"]["kind"] not in ROUTING_TERMS
            for block in blocks
        )
        or operations - ROUTING_OPS
    ):
        raise Unsupported("dispatcher exceeds the scalar routing proof domain")
    fuel = sum(2 + len(block["instructions"]) for block in blocks) + 1
    routes = ",".join(f"({row['selector']},{remap(row['id'])})" for row in selectors)
    receiving = f"(some {remap(receive['id'])})" if receive else "none"
    default = (
        f"(some ({remap(fallback['id'])},{str(fallback['payable']).lower()}))"
        if fallback
        else "none"
    )
    rejects_value = str(all(not row["payable"] for row in wrappers)).lower()
    available = sorted(
        definitions[op]["opcode"] for op in operations if op in definitions
    )
    if any(
        not definitions[op]["forks"][version]["available"]
        for op in operations
        if op in definitions
    ):
        raise Unsupported("dispatcher uses an unavailable opcode")
    opcode_hypotheses = "".join(
        f"    (op{opcode} : {opcode} ∈ cfg.available)\n" for opcode in available
    )
    op_simp = "".join(f"op{opcode}, " for opcode in available)
    cuts = [row["id"] for row in target["functions"] if row["id"] != entry]
    table = ",".join(
        mir_state.function_body(
            {**body, "blocks": []} if body["id"] != entry else body, definitions
        )
        for body in target["functions"]
    )
    unfold = mir_symbolic.step_definitions()
    for name in ("expand", "requiredWords", "memoryWords"):
        unfold = unfold.replace(f"Execution.{name}, ", "")
    normalize = f"""simp_all [{unfold}, ↓MIR.RoutedDispatch.reduceRunTo,
      MIR.RoutedDispatch.Agrees, MIR.RoutedDispatch.expand_zero _ bounded,
      MIR.TransferFromDispatch.selector_word, MIR.RoutedDispatch.callvalue,
      MIR.RoutedDispatch.lookup, Nat.mod_eq_of_lt inputBound, BitVec.toNat_ofNat]"""
    short_misses = "\n".join(
        f"""  have s{n} : cfg.environment.input.length < 4 →
      MIR.TransferFromDispatch.selector cfg.environment.input ≠ {row["selector"]} := by
    intro small equal
    have h := MIR.RoutedDispatch.short_selector _ small
    rw [equal] at h
    exact (by decide : (({row["selector"]} : Word).setWidth 8 ≠ 0)) h"""
        for n, row in enumerate(selectors)
        if row["selector"] & 0xFF != 0
    )
    selected = [
        f"({row['selector']} : Word) = MIR.TransferFromDispatch.selector cfg.environment.input"
        for row in selectors
    ]
    split_selectors = "\n".join(
        f"""  all_goals by_cases selected{index} : {condition}
  all_goals
    try (guard_hyp selected{index} : {condition}; solve | {normalize} <;> omega)"""
        for index, condition in enumerate(selected)
    )
    input_cases = []
    if rejects_value == "true" or (fallback and not fallback["payable"]):
        input_cases.append("by_cases zeroValue : cfg.environment.value = 0")
    if receive:
        input_cases.append("by_cases empty : cfg.environment.input = []")
    input_cases.append("by_cases short : cfg.environment.input.length < 4")
    partition = " <;>\n    ".join(input_cases)
    code = f"""
open MIR.RoutedDispatch in
def routes : Routes := ⟨[{routes}], {receiving}, {default}, {rejects_value}⟩
def cuts : List Nat := {cuts}
def routingBodies : MIR.State.Functions := [{table}]
theorem signatures : MIR.RoutedDispatch.cutBodies cuts after = routingBodies := rfl
theorem routing (cfg : MIR.State.Config) (frame : MIR.State.Frame)
{opcode_hypotheses}    (bounded : frame.memory.length ≤ 65536)
    (inputBound : cfg.environment.input.length < 2^256) :
    MIR.RoutedDispatch.Agrees entry frame
      (MIR.RoutedDispatch.select routes cfg.environment.input cfg.environment.value)
      (MIR.CallContext.runTo {cuts} {fuel} cfg routingBodies
        (MIR.RoutedDispatch.initial entry frame)) := by
{short_misses}
  have sizeZero : BitVec.ofNat 256 cfg.environment.input.length = 0 ↔
      cfg.environment.input = [] := by
    rw [← BitVec.toNat_inj]
    simp [BitVec.toNat_ofNat, Nat.mod_eq_of_lt inputBound, List.length_eq_zero_iff]
  {partition}
  all_goals
    simp [{unfold}, ↓MIR.RoutedDispatch.reduceRunTo, MIR.RoutedDispatch.initial,
      MIR.RoutedDispatch.select, MIR.RoutedDispatch.lookup, MIR.RoutedDispatch.Agrees,
      MIR.RoutedDispatch.callvalue,
      MIR.RoutedDispatch.expand_zero _ bounded, MIR.TransferFromDispatch.selector_word,
      BitVec.toNat_ofNat, Nat.mod_eq_of_lt inputBound, List.isEmpty_iff,
      List.length_eq_zero_iff, entry, routes, {op_simp}*]
{split_selectors}
  all_goals {normalize}
  all_goals omega
theorem dispatch (cfg : MIR.State.Config) (frame : MIR.State.Frame)
{opcode_hypotheses}    (bounded : frame.memory.length ≤ 65536)
    (inputBound : cfg.environment.input.length < 2^256) :
    MIR.RoutedDispatch.Agrees entry frame
      (MIR.RoutedDispatch.select routes cfg.environment.input cfg.environment.value)
      (MIR.CallContext.runTo cuts {fuel} cfg after (MIR.RoutedDispatch.initial entry frame)) := by
  rw [(MIR.RoutedDispatch.cut_agreement cuts after).prefix_eq, signatures]
  exact routing cfg frame {" ".join(f"op{opcode}" for opcode in available)} bounded inputBound
#print axioms dispatch
"""
    return code, {
        "work_steps": fuel,
        "selectors": len(selectors),
        "receive": bool(receive),
        "fallback": bool(fallback),
    }
