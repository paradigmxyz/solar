"""Bind constant arguments at every direct entry of a private MIR function."""

from . import mir_state


def direct_calls(body):
    for block, code in enumerate(body["blocks"]):
        operations = [inst["operation"] for inst in code["instructions"]] + [
            code["terminator"]
        ]
        for position, op in enumerate(operations):
            if op["kind"] in {"call", "tail_call"}:
                yield block, position, op


def substitute_arguments(node, values):
    if isinstance(node, list):
        return [substitute_arguments(item, values) for item in node]
    if isinstance(node, dict):
        if node.get("kind") == "arg":
            return values.get(node["index"], node)
        return {key: substitute_arguments(item, values) for key, item in node.items()}
    return node


def bind(model, tables, function, name):
    from .dead_arguments import argument_uses, entry_ids

    old = tables[0][function]
    target = old["id"]
    if not old["params"] or target in entry_ids(model):
        return None
    if any(
        body != tables[1].get(other)
        for other, body in tables[0].items()
        if other != function
    ):
        return None
    sites = [
        (body["id"], block, point, op["args"])
        for body in tables[0].values()
        for block, point, op in direct_calls(body)
        if op["function"] == target
    ]
    if not sites or any(owner == target for owner, *_ in sites):
        return None
    if any([arg["type"] for arg in args] != old["params"] for *_, args in sites):
        return None
    used = argument_uses(old["blocks"]) | argument_uses(tables[1][function]["blocks"])
    values = {
        index: arg
        for index, arg in enumerate(sites[0][3])
        if index in used
        and arg["kind"] == "constant"
        and arg["type"]["kind"] == "word"
        and all(args[index] == arg for *_, args in sites)
    }
    if not values or not any(
        index not in argument_uses(tables[1][function]["blocks"]) for index in values
    ):
        return None
    evidence = {
        "callee": target,
        "arguments": values,
        "direct_sites": [site[:3] for site in sites],
    }
    return values, evidence, certificate(name, sites, values)


def certificate(name, sites, bindings):
    code = ""
    for index, value in bindings.items():
        tag = f"{name}Argument{index}"
        width = value["type"]["bits"]
        number = int(value["value"])
        constant = mir_state.operand(value)
        parameter = mir_state.operand(
            {"kind": "arg", "index": index, "type": value["type"]}
        )
        read = f".scalar ⟨{width},{number}#{width}⟩"
        slots = ",".join(
            f"({owner},{block},{point},{int(args[index]['value'])}#{width})"
            for owner, block, point, args in sites
        )
        code += f"""
def {tag}Sites : List (Nat × Nat × Nat × BitVec {width}) := [{slots}]
theorem {tag}Inventory : {tag}Sites.all (fun site => site.2.2.2 == {number}#{width}) = true :=
  by decide +kernel
#print axioms {tag}Inventory

theorem {tag}Read (frame : MIR.State.Frame)
    (entered : frame.arguments[{index}]? = some ({read})) :
    MIR.State.readValue frame ({parameter}) = MIR.State.readValue frame ({constant}) := by
  simp [MIR.State.readValue, MIR.State.required, entered, Bind.bind, Except.bind]
#print axioms {tag}Read
"""
    return code
