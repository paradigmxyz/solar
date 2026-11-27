"""Bind a frame-free constructor's immutable writes to its staging words."""

from . import specialized_arguments
from .common import Unsupported
from .scalar_checks import WORD, literal


def check(paths, solar, replay, version, timeout, function, *, verbose=False):
    from . import mir_cfg, prune

    if not function or any(
        p.suffix != ".mir" or p.stat().st_size > 1024 * 1024 for p in paths
    ):
        return None
    old, new = (prune.declarations(p.read_text())[1].get(function) for p in paths)
    if not old or not new or "creation_only" not in new["text"][0]:
        return None
    if not any(line.startswith("storeimmutable ") for line in old["text"][1:]):
        return None
    try:
        return mir_cfg.check(
            paths,
            solar,
            replay,
            version,
            timeout,
            function,
            lowering="lower-immutables",
        )
    except Unsupported as error:
        if verbose:
            print(f"MIR immutable staging: {error}")
    return None


def reachable(roots, edges):
    seen = set()
    pending = list(roots)
    while pending:
        here = pending.pop()
        if here not in seen:
            seen.add(here)
            pending.extend(edges.get(here, ()))
    return seen


def bind(model, tables, interfaces, function):
    from .dead_arguments import entry_ids

    constructor = model["constructor"]
    if (
        constructor is None
        or tables[0][function]["id"] != constructor
        or any(
            body != tables[1].get(name)
            for name, body in tables[0].items()
            if name != function
        )
    ):
        raise Unsupported("immutable staging needs a single changed constructor")
    edges = {
        body["id"]: [
            op["function"] for _, _, op in specialized_arguments.direct_calls(body)
        ]
        for body in tables[0].values()
    }
    runtime_roots = entry_ids(model) - {constructor, None}
    if constructor in reachable(runtime_roots, edges):
        raise Unsupported("immutable staging constructor is callable at runtime")
    creation = reachable({constructor}, edges)
    for body in tables[0].values():
        for block in body["blocks"]:
            for inst in block["instructions"]:
                op = inst["operation"]
                frame = op["kind"] in {
                    "frame_load",
                    "frame_store",
                    "internal_frame_load",
                    "internal_frame_store",
                } or (op["kind"] == "layout" and op["query"] == "frame_address")
                store = op["kind"] == "layout" and op["query"] == "immutable_store"
                if (frame and body["id"] in creation) or (
                    store and body["id"] != constructor
                ):
                    raise Unsupported("immutable staging needs frame-free construction")

    header, before = interfaces[0]
    new_header, after = interfaces[1]
    expected = [
        line.removesuffix("] {") + ", creation_only] {"
        if line.startswith(f"fn @{function}(") and line.endswith("[constructor] {")
        else line
        for line in before
    ]
    if header != new_header or expected != after or expected == before:
        raise Unsupported(
            "immutable staging changed declarations beyond creation-only marking"
        )
    return runtime_roots, edges


def project(body, runtime_roots, edges, name):
    blocks = []
    stores = set()
    for block in body["blocks"]:
        instructions = []
        for inst in block["instructions"]:
            op = inst["operation"]
            if op["kind"] == "layout" and op["query"] == "immutable_store":
                if (
                    op["width"] != 256
                    or op["id"] > 2041
                    or len(op["args"]) != 1
                    or op["args"][0]["type"] != WORD
                    or op["args"][0]["kind"] == "undefined"
                ):
                    raise Unsupported(
                        "immutable staging requires a defined full-word assignment"
                    )
                slot = 192 + 32 * op["id"]
                stores.add((op["id"], slot))
                inst = {
                    **inst,
                    "operation": {
                        "kind": "opcode",
                        "name": "mstore",
                        "args": [literal(slot, WORD), *op["args"]],
                    },
                }
            instructions.append(inst)
        blocks.append({**block, "instructions": instructions})
    if not stores:
        raise Unsupported("immutable staging has no constructor assignment")

    reach = reachable(runtime_roots, edges)
    pairs = [
        (source, target) for source, targets in edges.items() for target in targets
    ]
    runtime_edges = ",".join(f"({source},{target})" for source, target in pairs)
    closure = sorted(reach)
    roots = sorted(runtime_roots)
    code = f"""
def {name}Runtime : List Nat := {closure}
def {name}Edges : List (Nat × Nat) := [{runtime_edges}]
theorem {name}Roots : ({roots} : List Nat).all {name}Runtime.contains = true := by decide +kernel
theorem {name}Closure :
    {name}Edges.all (fun e => !{name}Runtime.contains e.1 || {name}Runtime.contains e.2) = true :=
  by decide +kernel
theorem {name}Constructor : {name}Runtime.contains {body["id"]} = false := by decide +kernel
#print axioms {name}Roots
#print axioms {name}Closure
#print axioms {name}Constructor
"""
    for index, address in sorted(stores):
        tag = f"{name}Slot{index}"
        code += f"""
theorem {tag}Address : 192 + 32 * {index} = {address} := by decide
theorem {tag} (cfg : MIR.State.Config) (frame : MIR.State.Frame) (word : Word)
    (slot : MIR.State.layoutBinding cfg.layout.immutableSlots {index} = some {address}) :
    MIR.State.layoutOperation cfg frame (.immutableStore {index}) [word] =
      MIR.State.memoryOperation cfg frame .store [{address}#256,word] :=
  MIR.State.immutable_store_as_memory cfg frame {index} {address} word (by decide) slot
#print axioms {tag}Address
#print axioms {tag}
"""
    return {**body, "blocks": blocks}, sorted(stores), code
