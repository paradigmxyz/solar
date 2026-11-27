"""Check covered entry closures when MIR inlining removes private functions.

Reparse both function tables independently. Expand changed acyclic calls as
ordered effects and reuse a closed unchanged set when a call can stay opaque.
The checked scope is unpriced logical execution with shared effect handlers.
"""

import copy
import hashlib
import json
import tempfile
from pathlib import Path

from . import audit, dead_ssa, entry_proof, mir_cfg, mir_effects, mir_state, type_prune
from .common import Program, Unsupported

HEADER = """open SolarTV EvmRules Execution
set_option autoImplicit false
set_option maxRecDepth 16384
set_option maxHeartbeats 6000000
set_option Elab.async false
set_option linter.unusedVariables false
set_option linter.unusedSimpArgs false
"""

ENTRY_FIELDS = ("constructor", "dispatch_entry", "fallback", "receive")


def call_sites(body):
    """Enumerate both call forms without interpreting numeric identities."""
    for block in body["blocks"]:
        for inst in block["instructions"]:
            op = inst["operation"]
            if op["kind"] == "call":
                yield op
        term = block["terminator"]
        if term["kind"] == "tail_call":
            yield term


def module(model, declarations):
    names = {body["id"]: body["return_buffer"] for body in model["functions"]}
    if len(names) != len(model["functions"]) or len(set(names.values())) != len(names):
        raise Unsupported("inline proof requires unique typed function names and IDs")
    if set(names.values()) != declarations.keys():
        raise Unsupported(
            "inline proof has conflicting typed and canonical function tables"
        )

    def route(record):
        return {**record, "id": names[record["id"]]} if record is not None else None

    metadata = {key: value for key, value in model.items() if key != "functions"}
    entries = {names[key] for key in model["runtime_entries"]}
    metadata["runtime_entries"] = sorted(entries)
    for field in ENTRY_FIELDS:
        index = model[field]
        if index is not None:
            metadata[field] = names[index]
            entries.add(names[index])
    for field in ("receive_route", "fallback_route"):
        metadata[field] = route(model[field])
    metadata["selectors"] = sorted(
        map(route, model["selectors"]), key=lambda r: r["selector"]
    )
    if not entries or entries != {
        name for name, info in declarations.items() if info["root"]
    }:
        raise Unsupported("inline proof requires all canonical entry bindings")

    functions = {}
    for original in model["functions"]:
        body = copy.deepcopy(original)
        name = names[body.pop("id")]
        targets = set()
        for site in call_sites(body):
            site["function"] = names[site["function"]]
            targets.add(site["function"])
        if targets != set(declarations[name]["targets"]):
            raise Unsupported(
                "inline proof cannot bind a non-direct function reference"
            )
        functions[name] = body
    return functions, entries, dead_ssa.stable(metadata)


def unchanged_closure(before, after):
    """Take the greatest common set closed over named direct calls."""
    common = {name for name, body in after.items() if before.get(name) == body}
    while True:
        closed = {
            name
            for name in common
            if all(site["function"] in common for site in call_sites(after[name]))
        }
        if closed == common:
            return closed
        common = closed


def scalar_call(body):
    return (
        all(
            ty.get("kind") == "word"
            or ty.get("kind") == "pointer"
            and ty.get("location") == "memory"
            for ty in body["params"] + body["returns"]
        )
        and len(body["returns"]) <= 1
    )


def plan_entry(functions, name, ids, cuts):
    pending = [name]
    visited = set()
    closure = []
    cuts_used = []
    while pending:
        current = pending.pop()
        if current in visited:
            continue
        visited.add(current)
        body = copy.deepcopy(functions[current])
        body["id"] = ids[current]
        if current in cuts:
            body["blocks"] = []
            cuts_used.append(ids[current])
        else:
            for site in call_sites(body):
                target = site["function"]
                pending.append(target)
                site["function"] = ids[target]
        closure.append(body)
    return {"entry_id": ids[name], "functions": closure}, cuts_used


def entry_source(name, signature, trees, obligations):
    binders = " ".join(f"(arg{i} : BitVec {bits})" for i, bits in enumerate(signature))
    values = " ".join(f"arg{i}" for i in range(len(signature)))
    source = f"""import SolarTV
namespace {name}
{HEADER}
def before (environment : World.Environment) {binders} : MIR.Effects.Tree :=
{trees[0]}

def after (environment : World.Environment) {binders} : MIR.Effects.Tree :=
{trees[1]}

def Obligation : Prop := ∀ (environment : World.Environment) {binders},
  before environment {values} = after environment {values}

theorem checked : Obligation := by
  intro environment {values}
  simp [before,after,MIR.compare,MIR.eval,MIR.cast,MIR.shift,Word.shift,
    Word.binary,MIR.unary,Word.unary,MIR.select,MIR.wordBinary,MIR.ternary,
    MIR.LowerAlloc.freePointerRequest,MIR.LowerAlloc.constructorRequest,
    BitVec.ofNatLT_eq_ofNat,BitVec.ult,BitVec.xor_comm,MIR.Effects.abiReturn]

theorem contextual (environment : World.Environment) {binders}
    {{State Outcome : Type}}
    (handler : MIR.Effects.Request → State → Except Outcome (Word × State))
    (finish : MIR.Effects.Boundary → State → Outcome) (state : State) :
    MIR.Effects.interpret handler finish (before environment {values}) state =
      MIR.Effects.interpret handler finish (after environment {values}) state :=
  MIR.Effects.contextual _ _ (checked environment {values}) handler finish state

#print axioms contextual
"""
    return source + "\n".join(obligations) + f"\nend {name}\n"


def closure_source(tables, roots, stable, bindings, proofs):
    modules = "".join(f"import {proof}\n" for _, proof in proofs)
    code = modules + HEADER
    bodies = {}

    def program(name, values):
        nonlocal code
        entries = []
        for key, body in sorted(values.items()):
            shape = dead_ssa.stable(body)
            targets = sorted({site["function"] for site in call_sites(body)})
            record = f"⟨[{json.dumps(shape, ensure_ascii=False)}],{json.dumps(targets)},{str(key in roots).lower()}⟩"
            if record not in bodies:
                symbol = f"body{len(bodies)}"
                bodies[record] = symbol
                code += f"\ndef {symbol} : MIR.FunctionDCE.Body := {record}\n"
            entries.append(f"({json.dumps(key)},{bodies[record]})")
        code += f"\ndef {name} : MIR.FunctionDCE.Program := [{','.join(entries)}]\n"

    before, after = tables
    program("oldTable", before)
    program("newTable", after)
    program("unchangedTable", {name: after[name] for name in stable})
    pairs = ",".join(
        f"({json.dumps(name)},{proof}.Obligation)" for name, proof in proofs
    )
    code += f"""
def changed : List (String × Prop) := [{pairs}]
def entries : List String := {json.dumps(sorted(roots))}
def oldBindings : String := {json.dumps(bindings[0])}
def newBindings : String := {json.dumps(bindings[1])}

theorem entry_coverage : oldBindings = newBindings ∧
    (oldTable.filter fun item => item.2.root).map Prod.fst = entries ∧
    (newTable.filter fun item => item.2.root).map Prod.fst = entries ∧
    entries.all (fun name => (changed.map Prod.fst).contains name ||
      (unchangedTable.map Prod.fst).contains name) = true := by
  refine ⟨rfl, rfl, rfl, ?_⟩
  decide +kernel

theorem checked_entries : ∀ item ∈ changed, item.2 := by
  intro item member
  simp only [changed,List.mem_cons,List.not_mem_nil,or_false] at member
"""
    if len(proofs) > 1:
        cases = " | ".join("rfl" for _ in proofs)
        code += f"  rcases member with {cases}\n"
        code += "".join(f"  · exact {proof}.checked\n" for _, proof in proofs)
    else:
        code += f"  subst item\n  exact {proofs[0][1]}.checked\n"
    names = json.dumps(sorted(stable))
    code += f"""
def unchangedNames : List String := {names}

theorem same_unchanged (name : String) (present : name ∈ unchangedNames) :
    MIR.FunctionDCE.lookup oldTable name = MIR.FunctionDCE.lookup unchangedTable name ∧
    MIR.FunctionDCE.lookup newTable name = MIR.FunctionDCE.lookup unchangedTable name := by
  simp only [unchangedNames,List.mem_cons,List.not_mem_nil,or_false] at present
"""
    if stable:
        if len(stable) > 1:
            cases = " | ".join("rfl" for _ in stable)
            code += f"  rcases present with {cases}\n"
        else:
            code += "  subst name\n"
        code += "  all_goals exact ⟨rfl,rfl⟩\n"
    return (
        code
        + """
theorem common_closed :
    MIR.FunctionDCE.Closed unchangedTable (unchangedTable.map Prod.fst) := by
  have checked : MIR.FunctionDCE.closed unchangedTable = true := by decide +kernel
  intro name _ body found target present
  have entry := MIR.FunctionDCE.lookup_mem unchangedTable name body found
  simp only [MIR.FunctionDCE.closed,List.all_eq_true] at checked
  exact of_decide_eq_true (checked (name,body) entry target present)

theorem closed_under_lookup (table : MIR.FunctionDCE.Program)
    (same : ∀ name ∈ unchangedTable.map Prod.fst,
      MIR.FunctionDCE.lookup table name = MIR.FunctionDCE.lookup unchangedTable name) :
    MIR.FunctionDCE.Closed table (unchangedTable.map Prod.fst) := by
  intro name present body found target member
  rw [same name present] at found
  exact common_closed name present body found target member

theorem checked_unchanged {State Saved Observation : Type}
    (step : (body : MIR.FunctionDCE.Body) → State → MIR.FunctionDCE.Action body.targets State Saved Observation)
    (resume : Saved → State → State) (finish : State → Observation)
    (fuel : Nat) (machine : MIR.FunctionDCE.Machine State Saved)
    (valid : MIR.FunctionDCE.Valid (unchangedTable.map Prod.fst) machine) :
    MIR.FunctionDCE.run oldTable step resume finish fuel machine =
      MIR.FunctionDCE.run newTable step resume finish fuel machine := by
  have left : ∀ name ∈ unchangedTable.map Prod.fst,
      MIR.FunctionDCE.lookup oldTable name = MIR.FunctionDCE.lookup unchangedTable name :=
    fun name member => (same_unchanged name member).1
  have right : ∀ name ∈ unchangedTable.map Prod.fst,
      MIR.FunctionDCE.lookup newTable name = MIR.FunctionDCE.lookup unchangedTable name :=
    fun name member => (same_unchanged name member).2
  exact (MIR.FunctionDCE.preserve _ _ _ left (closed_under_lookup _ left)
    step resume finish fuel machine valid).trans
    (MIR.FunctionDCE.preserve _ _ _ right (closed_under_lookup _ right)
      step resume finish fuel machine valid).symm

#print axioms entry_coverage
#print axioms checked_entries
#print axioms checked_unchanged
"""
    )


def prepare(texts, declarations, replay, version, timeout):
    from . import native

    exported = [type_prune.export(text, replay, timeout) for text in texts]
    try:
        tables, roots, bindings = zip(
            *(
                module(model, decls)
                for model, decls in zip(exported, declarations, strict=True)
            ),
            strict=True,
        )
    except Unsupported:
        raise
    except (KeyError, TypeError, ValueError) as error:
        raise Unsupported("invalid typed inline module") from error
    before, after = tables
    if roots[0] != roots[1] or bindings[0] != bindings[1]:
        raise Unsupported("inline proof changed a module entry binding")
    for name in roots[0]:
        if (
            before[name]["params"] != after[name]["params"]
            or before[name]["returns"] != after[name]["returns"]
            or declarations[0][name]["text"][0] != declarations[1][name]["text"][0]
        ):
            raise Unsupported("inline proof changed a public signature")

    stable = unchanged_closure(before, after)
    cuts = {name for name in stable if scalar_call(after[name])}
    ids = {
        name: index for index, name in enumerate(sorted(before.keys() | after.keys()))
    }
    definitions = {row["name"]: row for row in native.catalog(replay, timeout)["evmir"]}
    entries = []
    for index, name in enumerate(sorted(roots[0] - stable)):
        plans = [plan_entry(table, name, ids, cuts) for table in tables]
        trees = [
            mir_effects.render(
                model,
                definitions,
                abi_fork=version,
                call_cuts=call_cuts,
                inline_effects=True,
            )
            for model, call_cuts in plans
        ]
        body = after[name]
        signature = [
            sum(int(mir_state.width(leaf["bits"])) for leaf in mir_state.leaf_types(ty))
            for ty in body["params"]
        ]
        prefix = f"InlineEntry{index}"
        products = []
        for side, (model, call_cuts) in enumerate(plans):
            for fn in model["functions"]:
                if fn["id"] in call_cuts:
                    continue
                nodes = []
                for block in fn["blocks"]:
                    nodes.append(("phi", block["phis"], []))
                    nodes.extend(("action", inst, []) for inst in block["instructions"])
                    nodes.append(("terminal", block["terminator"], []))
                obligation = mir_cfg.product_certificate(
                    f"product{side}Function{fn['id']}", nodes
                )
                if obligation:
                    products.append(obligation)
        entries.append(
            (
                name,
                prefix,
                entry_source(prefix, signature, trees, products),
                1 + len(products),
            )
        )
    if not entries:
        raise Unsupported("inline proof requires a changed reachable body")
    checks = [(name, proof) for name, proof, _, _ in entries]
    return entries, closure_source(tables, roots[0], stable, bindings, checks), stable


def check(texts, declarations, replay, version, timeout):
    from .check import build_model

    entries, closure, stable = prepare(texts, declarations, replay, version, timeout)
    entry_proof.success(build_model(timeout), "MIR inline model")
    with tempfile.TemporaryDirectory(prefix="solar-tv-mir-inline-") as folder:
        root = Path(folder)
        for name, proof, source, axioms in entries:
            output = entry_proof.lean(root, proof, source, timeout, module=True)
            entry_proof.check_axioms(
                entry_proof.success(output, f"MIR inline entry {name}").splitlines(),
                axioms,
            )
        output = entry_proof.lean(root, "InlineModule", closure, timeout)
        entry_proof.check_axioms(
            entry_proof.success(output, "MIR inline module").splitlines(), 3
        )
    certificate = hashlib.sha256(
        ("".join(source for _, _, source, _ in entries) + closure).encode()
    ).hexdigest()
    programs = tuple(
        Program(
            ("mir-inline-module",),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            evm_version=version,
            scope="module-inline",
            import_evidence={
                "kind": "rooted-inline-effects",
                "changed_entries": [name for name, _, _, _ in entries],
                "unchanged_closed_functions": sorted(stable),
                "source_sha256": hashlib.sha256(text.encode()).hexdigest(),
                "certificate_sha256": certificate,
                "boundary": "trusted typed source renderer; exact shared effects and matching public roots; physical gas and placement separate",
            },
        )
        for text in texts
    )
    return audit.Result(
        "verified", stdout="verified\n", program=programs[0], pair=programs
    )
