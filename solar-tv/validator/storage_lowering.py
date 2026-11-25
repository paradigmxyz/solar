"""Typed MIR schemas and contextual linking for storage-byte lowering."""

import hashlib
import json

from . import audit, dead_ssa, mir_state, native, type_prune
from .builtin_lowering import (
    body_blocks,
    functions,
    instruction,
    normalize,
    operation,
    prove,
    require,
)
from .common import Program

HELPER = "clear_storage_words"
HELPER_HEADER = f"fn @{HELPER}(arg0: i256, arg1: i256, arg2: i256) {{"

# This is the input to the LowerStorageBytes proof's SSA interpretation.
# The discarded zext is total and unused. Block IDs, local IDs and the clear
# helper name are immaterial. Every executable operand and phi edge is checked.
EXPANSION = """@module StoreBytes

fn @write(arg0: i256, arg1: ptr<memory>) {
  bb0:
    v0 = sload arg0
    v1 = and v0, 1
    v2 = eq v1, 1
    v3 = sub 0, v1
    v4 = or v3, 127
    v5 = shr v0, 1
    v6 = and v5, v4
    v7 = lt v6, 32
    v8 = eq v2, v7
    jumpi v8, bb1, bb2
  bb1:
    mstore 32, 34
    mstore 0, 0x4e487b71
    revert 28, 36
  bb2:
    v9 = ptradd arg1, 32
    v10 = mload arg1
    v11 = make_memory_slice v9, v10
    v12 = extract_value slice<memory>, v11, 1
    v13 = add v6, 31
    v14 = div v13, 32
    v15 = add v12, 31
    v16 = lt v15, v12
    jumpi v16, bb3, bb4
  bb3:
    mstore 32, 65
    mstore 0, 0x4e487b71
    revert 28, 36
  bb4:
    v17 = div v15, 32
    v18 = lt v12, 32
    v19 = select v18, 0, v17
    v20 = lt v12, v6
    v21 = and v2, v20
    v22 = zext i1 v21 to i256
    jumpi v21, bb5, bb6
  bb5:
    icall @clear, arg0, v19, v14
    jump bb6
  bb6:
    jumpi v18, bb7, bb8
  bb7:
    v23 = extract_value slice<memory>, v11, 0
    v24 = mload v23
    v25 = sub 32, v12
    v26 = mul v25, 8
    v27 = shl 1, v26
    v28 = sub v27, 1
    v29 = not v28
    v30 = and v24, v29
    v31 = mul v12, 2
    v32 = or v30, v31
    sstore arg0, v32
    jump bb9
  bb8:
    v33 = shl v12, 1
    v34 = or v33, 1
    sstore arg0, v34
    v35 = storage_array_data_slot arg0
    v36 = div v12, 32
    jump bb10
  bb9:
    ret
  bb10:
    v37 = phi [bb8: 0], [bb11: v45]
    v38 = lt v37, v36
    jumpi v38, bb11, bb12
  bb11:
    v39 = mul v37, 32
    v40 = extract_value slice<memory>, v11, 0
    v41 = ptrtoint ptr<memory> v40 to i256
    v42 = add v41, v39
    v43 = mload v42
    v44 = add v35, v37
    sstore v44, v43
    v45 = add v37, 1
    jump bb10
  bb12:
    v46 = and v12, 31
    v47 = eq v46, 0
    jumpi v47, bb9, bb13
  bb13:
    v48 = mul v36, 32
    v49 = extract_value slice<memory>, v11, 0
    v50 = ptrtoint ptr<memory> v49 to i256
    v51 = add v50, v48
    v52 = mload v51
    v53 = sub 32, v46
    v54 = mul v53, 8
    v55 = shl 1, v54
    v56 = sub v55, 1
    v57 = not v56
    v58 = and v52, v57
    v59 = add v35, v36
    sstore v59, v58
    jump bb9
}

fn @clear(arg0: i256, arg1: i256, arg2: i256) {
  bb0:
    v0 = storage_array_data_slot arg0
    v1 = add v0, arg1
    jump bb1
  bb1:
    v2 = phi [bb0: arg1], [bb2: v5]
    v3 = phi [bb0: v1], [bb2: v6]
    v4 = lt v2, arg2
    jumpi v4, bb2, bb3
  bb2:
    sstore v3, 0
    v5 = add v2, 1
    v6 = add v3, 1
    jump bb1
  bb3:
    ret
}
"""


LOAD_EXPANSION = """@module LoadBytes

fn @load(arg0: i256) -> ptr<memory> {
  bb0:
    v1 = sload arg0
    v2 = and v1, 1
    v3 = eq v2, 1
    v4 = sub 0, v2
    v5 = or v4, 127
    v6 = shr v1, 1
    v7 = and v6, v5
    v8 = lt v7, 32
    v9 = eq v3, v8
    jumpi v9, bb1, bb2
  bb1:
    mstore 32, 34
    mstore 0, 0x4e487b71
    revert 28, 36
  bb2:
    v10 = add v7, 31
    v11 = div v10, 32
    v12 = add v7, 31
    v13 = and v12, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0
    v14 = add v13, 32
    v15 = lt v14, v7
    jumpi v15, bb3, bb4
  bb3:
    mstore 32, 65
    mstore 0, 0x4e487b71
    revert 28, 36
  bb4:
    v16 = alloc checked v14
    mstore v16, v7
    jumpi v3, bb6, bb5
  bb5:
    v17 = and v1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff00
    v18 = ptradd v16, 32
    mstore v18, v17
    jump bb7
  bb6:
    v19 = storage_array_data_slot arg0
    jump bb8
  bb7:
    ret v16
  bb8:
    v20 = phi [bb6: 0], [bb9: v27]
    v21 = lt v20, v11
    jumpi v21, bb9, bb10
  bb9:
    v22 = add v19, v20
    v23 = sload v22
    v24 = mul v20, 32
    v25 = ptradd v16, v24
    v26 = ptradd v25, 32
    mstore v26, v23
    v27 = add v20, 1
    jump bb8
  bb10:
    jump bb7
}
"""

LOAD_SOURCE = """@module LoadBytes

fn @load(arg0: i256) -> ptr<memory> {
  bb0:
    v0 = load_storage_bytes arg0
    ret v0
}
"""


def shape(function, call=None):
    graph = normalize({**function, "blocks": body_blocks(function)})
    del graph["id"], graph["return_buffer"]
    for block in graph["blocks"]:
        for inst in block["instructions"]:
            op = inst["operation"]
            if op["kind"] == "call":
                require(op["function"] == call, "unexpected storage clear call target")
                op["function"] = 0
    return graph


def check_loader(before, after, refs, executable, timeout):
    require(not any(refs), "storage-byte loader must be closed")
    source = functions(type_prune.export(LOAD_SOURCE, executable, timeout))["load"]
    expanded = functions(type_prune.export(LOAD_EXPANSION, executable, timeout))["load"]
    require(
        shape(before) == shape(source) and shape(after) == shape(expanded),
        "storage loader differs from the proved SSA expansion",
    )


def check_source(before, after, body, refs, executable, timeout):
    require(not refs[0] and len(refs[1]) == 1, "expected a single storage clear helper")
    helper = refs[1][0]
    require(
        helper != after["return_buffer"] and helper in body,
        "missing storage clear helper",
    )
    clear = body[helper]
    expected = functions(type_prune.export(EXPANSION, executable, timeout))

    args = [
        {"kind": "arg", "type": ty, "index": i}
        for i, ty in enumerate(expected["write"]["params"])
    ]
    require(
        before["params"] == expected["write"]["params"]
        and before["returns"] == []
        and before["blocks"]
        == [
            {
                "instructions": [instruction(operation("store_storage_bytes", args))],
                "phis": [],
                "terminator": operation("return"),
            }
        ],
        "expected a single semantic storage-byte write",
    )

    require(
        shape(after, clear["id"]) == shape(expected["write"], expected["clear"]["id"])
        and shape(clear) == shape(expected["clear"]),
        "storage writer or clear helper differs from the proved SSA expansion",
    )
    return [helper]


def helper_addition(before, after):
    return (
        before.keys() <= after.keys()
        and after.keys() - before.keys() == {HELPER}
        and after[HELPER]["text"][0] == HELPER_HEADER
        and not after[HELPER]["root"]
        and not after[HELPER]["targets"]
    )


def check_module(texts, declarations, executable, version, timeout):
    models = [type_prune.export(text, executable, timeout) for text in texts]
    tables = list(map(functions, models))
    before, after = tables
    require(
        all(
            table.keys() == decls.keys()
            and len({body["id"] for body in table.values()}) == len(table)
            for table, decls in zip(tables, declarations, strict=True)
        )
        and {k: v for k, v in models[0].items() if k != "functions"}
        == {k: v for k, v in models[1].items() if k != "functions"},
        "storage-byte module changed shared entry bindings or function identities",
    )
    checked = {}
    for name in before:
        old, new = before[name], after[name]
        old_decl, new_decl = (decls[name] for decls in declarations)
        if old == new and old_decl == new_decl:
            continue
        require(
            old_decl["text"][0] == new_decl["text"][0] and old["id"] == new["id"],
            "storage-byte module changed a retained interface",
        )
        refs = [old_decl["targets"], new_decl["targets"]]
        ops = [
            inst["operation"]["kind"]
            for block in old["blocks"]
            for inst in block["instructions"]
        ]
        if ops == ["load_storage_bytes"] and "load" not in checked:
            check_loader(old, new, refs, executable, timeout)
            checked["load"] = name
        elif ops == ["store_storage_bytes"] and "store" not in checked:
            helpers = check_source(old, new, after, refs, executable, timeout)
            require(helpers == [HELPER], "unexpected storage-byte module helper")
            checked["store"] = name
        else:
            require(False, "storage-byte module has another executable change")
    require(checked.keys() == {"load", "store"}, "expected both storage-byte bodies")

    definitions = {
        row["name"]: row for row in native.catalog(executable, timeout)["evmir"]
    }
    replaced = set(checked.values())
    code = "set_option maxRecDepth 16384\n"
    for side, table, decls in zip(
        ("before", "after"), tables, declarations, strict=True
    ):
        retained = [name for name in table if name != HELPER]
        context = [
            line
            for name in retained
            for line in (
                decls[name]["text"][:1] if name in replaced else decls[name]["text"]
            )
        ]
        typed = ",\n".join(
            mir_state.function_body(
                {**table[name], "blocks": []} if name in replaced else table[name],
                definitions,
            )
            for name in retained
        )
        code += (
            f"def {side}Context : MIR.TypeDCE.Program := "
            f"⟨{json.dumps(context, ensure_ascii=False)},[{typed}]⟩\n"
        )
    prove(code + MODULE_PROOF, timeout)
    pair = tuple(
        Program(
            ("storage-byte-module",),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            scope="builtin-lowering",
            evm_version=version,
            import_evidence={
                "kind": "contextual-storage-bytes-module",
                "source_sha256": hashlib.sha256(
                    dead_ssa.stable(model).encode()
                ).hexdigest(),
                "functions": list(before),
                "replaced": checked,
                "proved_helpers": [HELPER],
                "boundary": (
                    "resolved retained caller table with exact load, store and clear "
                    "shapes; contextual linkage through equal unpriced effects at the "
                    "two replaced bodies"
                ),
            },
        )
        for text, model in zip(texts, models, strict=True)
    )
    return audit.Result("verified", stdout="verified\n", program=pair[0], pair=pair)


MODULE_PROOF = """
open SolarTV.MIR.State EvmRules

theorem sameContext : beforeContext = afterContext := by rfl

theorem contextual {Observation : Type} (cfg : Config)
    (run : MIR.TypeDCE.Program →
      (∀ frame : Frame, frame.memory.length ≤ 65536 → Word → Step) →
      (Frame → Word → Word → Step) → Observation) :
    run beforeContext (fun frame _ slot => loadStorageBytes cfg frame slot)
      (fun frame slot object => storeStorageBytes cfg frame slot object none) =
    run afterContext (fun frame _ slot => MIR.LowerStorageLoad.lowered cfg frame slot)
      (MIR.LowerStorageBytes.lowered cfg) := by
  have loads : (fun (frame : Frame) (_ : frame.memory.length ≤ 65536) slot =>
      loadStorageBytes cfg frame slot) =
      (fun frame _ slot => MIR.LowerStorageLoad.lowered cfg frame slot) := by
    funext frame bounded slot
    exact (MIR.LowerStorageLoad.load cfg frame slot bounded).symm
  have stores : (fun frame slot object => storeStorageBytes cfg frame slot object none) =
      MIR.LowerStorageBytes.lowered cfg := by
    funext frame slot object
    exact (MIR.LowerStorageBytes.store cfg frame slot object).symm
  rw [sameContext, loads, stores]

#print axioms contextual
"""
