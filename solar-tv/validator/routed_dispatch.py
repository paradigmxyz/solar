"""Generate a proof of an explicit dispatcher against its implicit ABI routes."""

import re

from . import dispatch_guard_hoist, mir_state, mir_symbolic
from .common import Unsupported

ENTRY_ONLY = {"abi_wrapper", "external", "public", "payable", "pure", "view"}
ROUTING_OPS = {
    "callvalue",
    "calldataload",
    "calldatasize",
    "lt",
    "ne",
    "shr",
    "or",
    "word",
}
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


def proof(
    source, target, entry, definitions, version, before, retained, *, debug_reverts
):
    def remap(id_):
        return id_ + (id_ >= entry)

    selectors = sorted(source["selectors"], key=lambda row: row["selector"])
    receive, fallback = source["receive_route"], source["fallback_route"]
    wrappers = selectors + [row for row in (receive, fallback) if row is not None]
    if not all(row["wrapper"] for row in wrappers):
        raise Unsupported("routed dispatch requires explicit ABI wrappers")
    library = source["library"]
    if library and (receive or fallback):
        raise Unsupported("library runtime dispatch requires selector-only routes")
    if library and any(
        row.get("mutability") not in {"payable", "nonpayable", "view", "pure"}
        for row in selectors
    ):
        raise Unsupported("library runtime dispatch needs source route mutability")
    guarded = [
        remap(row["id"])
        for row in selectors
        if library and row["mutability"] in {"payable", "nonpayable"}
    ]
    immutable = source.get("library_immutable") if guarded else None
    if guarded and (immutable is None or target.get("library_immutable") != immutable):
        raise Unsupported(
            "library dispatch needs the retained deployment-address immutable"
        )
    if len({row["selector"] for row in selectors}) != len(selectors):
        raise Unsupported("duplicated implicit selector")
    if sorted(target["runtime_entries"]) != sorted(
        remap(row["id"]) for row in wrappers
    ):
        raise Unsupported("dispatcher tail-call targets differ from the source routes")
    for role, route in (("receive", receive), ("fallback", fallback)):
        if target[role] != (remap(route["id"]) if route else None):
            raise Unsupported(f"dispatcher changed the {role} root")
    declarations = {name: retained_declaration(body) for name, body in before.items()}
    retained_text = {name: body["text"] for name, body in retained}
    minimum, guards = None, []
    if list(declarations.items()) != list(retained_text.items()):
        minimum, guards = dispatch_guard_hoist.guards(
            source, entry, selectors, declarations, retained_text
        )
    if debug_reverts and minimum is not None:
        raise Unsupported(
            "debug dispatch requires wrappers whose guard blocks are unchanged"
        )

    dispatcher = target["functions"][entry]
    # The interpreter's cut must only follow a finite, read-only routing prefix.
    # The generated Lean theorem checks the actually imported instructions.
    blocks = dispatcher["blocks"]
    instructions = [i["operation"] for block in blocks for i in block["instructions"]]
    operations = {op.get("name", op["kind"]) for op in instructions}
    allowed = ROUTING_OPS
    if debug_reverts:
        allowed = allowed | {"mstore"}
    if guarded:
        allowed = allowed | {"address", "eq"}
        if all(
            op
            == {
                "kind": "layout",
                "query": "immutable_read",
                "id": immutable,
                "width": 160,
                "signed": False,
                "args": [],
            }
            for op in instructions
            if op["kind"] == "layout"
        ):
            allowed = allowed | {"layout"}
    if (
        len(blocks) > 64
        or any(
            block["phis"] or block["terminator"]["kind"] not in ROUTING_TERMS
            for block in blocks
        )
        or operations - allowed
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
    rejects_value = str(
        not library and bool(wrappers) and all(not row["payable"] for row in wrappers)
    ).lower()
    if minimum is not None:
        operations.update(("calldatasize", "lt"))
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
    opcode_arguments = " ".join(f"op{opcode}" for opcode in available)
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
    original = (
        "MIR.RoutedDispatch.select routes cfg.environment.input cfg.environment.value"
    )
    relation = "MIR.RoutedDispatch.Agrees"
    selected_member = "MIR.RoutedDispatch.selector_target _ _ _ _ id selected"
    library_hypotheses = library_arguments = library_simp = ""
    library_denied = "(fun _ => false)"
    if guarded:
        library_hypotheses = f"""    (deployment : World.Address)
    (binding : MIR.LibraryDispatch.Binding cfg {immutable} deployment)
"""
        library_arguments = "deployment binding "
        original = (
            f"MIR.LibraryDispatch.select routes.selectors {guarded} deployment "
            "cfg.environment.address cfg.environment.input"
        )
        selected_member = "MIR.LibraryDispatch.selector_target _ _ _ _ _ id selected"
        library_denied = f"(fun id => deployment == cfg.environment.address && ({guarded} : List Nat).contains id)"
        library_simp = (
            "MIR.LibraryDispatch.select, MIR.LibraryDispatch.address, "
            f"MIR.LibraryDispatch.immutable _ _ {immutable} deployment binding, Option.filter, "
        )
    diagnostics_simp = ""
    if debug_reverts:
        relation = "MIR.DispatchDiagnostics.Agrees"
        original = (
            "MIR.DispatchDiagnostics.select routes cfg.environment.input cfg.environment.value "
            + library_denied
        )
        diagnostics_simp = (
            "MIR.DispatchDiagnostics.select, MIR.DispatchDiagnostics.Agrees, "
            "MIR.DispatchDiagnostics.expected, MIR.DispatchDiagnostics.Failure.message, "
            "MIR.DispatchDiagnostics.Failure.size, MIR.DispatchDiagnostics.messageBytes, "
            "MIR.DispatchDiagnostics.encodeMemory, "
            "MIR.DispatchDiagnostics.writeWord, MIR.DispatchDiagnostics.expand_small, "
            "MIR.DispatchDiagnostics.grown_bounded, Execution.writeBytes_length, "
        )
    normalize = f"""simp_all [{unfold}, ↓MIR.RoutedDispatch.reduceRunTo,
      MIR.RoutedDispatch.Agrees, MIR.RoutedDispatch.expand_zero _ bounded,
      MIR.TransferFromDispatch.selector_word, MIR.RoutedDispatch.callvalue,
      MIR.RoutedDispatch.lookup, Nat.mod_eq_of_lt inputBound, BitVec.toNat_ofNat, {library_simp}{diagnostics_simp}]"""
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
    if guarded:
        input_cases.append("by_cases direct : deployment = cfg.environment.address")
    if rejects_value == "true" or (fallback and not fallback["payable"]):
        input_cases.append("by_cases zeroValue : cfg.environment.value = 0")
    if receive:
        input_cases.append("by_cases empty : cfg.environment.input = []")
    input_cases.append("by_cases short : cfg.environment.input.length < 4")
    expected = original
    prefix_theorem = "dispatch"
    retained_proof = "theorem retained : after = reference := by rfl\n"
    route_simp = ""
    if minimum is not None:
        input_cases.append(
            f"by_cases shortHead : cfg.environment.input.length < {minimum}"
        )
        expected = (
            f"if cfg.environment.input.length < {minimum} then none else {expected}"
        )
        prefix_theorem = "dispatch_prefix"
        retained_proof = dispatch_guard_hoist.retained_proof(minimum, guards)
        route_simp = "BitVec.ult_eq_decide, BitVec.or_eq_zero_iff, "
    partition = " <;>\n    ".join(input_cases)
    code = f"""
open MIR.RoutedDispatch in
def routes : Routes := ⟨[{routes}], {receiving}, {default}, {rejects_value}⟩
{retained_proof}
def cuts : List Nat := {cuts}
def routingBodies : MIR.State.Functions := [{table}]
theorem signatures : MIR.RoutedDispatch.cutBodies cuts after = routingBodies := rfl
theorem routing (cfg : MIR.State.Config) (frame : MIR.State.Frame)
{library_hypotheses}{opcode_hypotheses}    (bounded : frame.memory.length ≤ 65536)
    (inputBound : cfg.environment.input.length < 2^256) :
    {relation} entry frame
      ({expected})
      (MIR.CallContext.runTo {cuts} {fuel} cfg routingBodies
        (MIR.RoutedDispatch.initial entry frame)) := by
{short_misses}
  have sizeZero : BitVec.ofNat 256 cfg.environment.input.length = 0 ↔
      cfg.environment.input = [] := by
    rw [← BitVec.toNat_inj]
    simp [BitVec.toNat_ofNat, Nat.mod_eq_of_lt inputBound, List.length_eq_zero_iff]
  {partition}
  all_goals
    try omega
  all_goals
    simp [{unfold}, ↓MIR.RoutedDispatch.reduceRunTo, MIR.RoutedDispatch.initial,
      MIR.RoutedDispatch.select, MIR.RoutedDispatch.lookup, MIR.RoutedDispatch.Agrees,
      MIR.RoutedDispatch.callvalue,
      MIR.RoutedDispatch.expand_zero _ bounded, MIR.TransferFromDispatch.selector_word,
      BitVec.toNat_ofNat, Nat.mod_eq_of_lt inputBound, List.isEmpty_iff,
      List.length_eq_zero_iff, entry, routes, {library_simp}{route_simp}{diagnostics_simp}{op_simp}*]
{split_selectors}
  all_goals {normalize}
  all_goals omega
theorem {prefix_theorem} (cfg : MIR.State.Config) (frame : MIR.State.Frame)
{library_hypotheses}{opcode_hypotheses}    (bounded : frame.memory.length ≤ 65536)
    (inputBound : cfg.environment.input.length < 2^256) :
    {relation} entry frame
      ({expected})
      (MIR.CallContext.runTo cuts {fuel} cfg after (MIR.RoutedDispatch.initial entry frame)) := by
  rw [(MIR.RoutedDispatch.cut_agreement cuts after).prefix_eq, signatures]
  exact routing cfg frame {library_arguments}{opcode_arguments} bounded inputBound
"""
    evidence = {
        "work_steps": fuel,
        "selectors": len(selectors),
        "receive": bool(receive),
        "fallback": bool(fallback),
    }
    if library:
        evidence["library_guarded_routes"] = len(guarded)
    if debug_reverts:
        evidence["debug_reverts"] = True
    if minimum is not None:
        code += f"""
theorem dispatch (cfg : MIR.State.Config) (frame : MIR.State.Frame)
{library_hypotheses}{opcode_hypotheses}    (bounded : frame.memory.length ≤ 65536)
    (inputBound : cfg.environment.input.length < 2^256) :
    MIR.DispatchGuardHoist.Agrees cfg reference after entry frame
      ({original})
      (MIR.CallContext.runTo cuts {fuel} cfg after (MIR.RoutedDispatch.initial entry frame)) :=
  MIR.DispatchGuardHoist.certificate cfg reference after entry {minimum} frame
    ({original}) _ (dispatch_prefix cfg frame {library_arguments}{opcode_arguments} bounded inputBound)
    (fun id selected => wrappers id ({selected_member})) op54 op16 inputBound (by decide) bounded
"""
        evidence["minimum_calldata"] = minimum
    return code + "#print axioms dispatch\n", evidence
