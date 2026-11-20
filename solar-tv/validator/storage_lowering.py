"""Typed MIR schema for the proved storage-byte write and clear loops."""

from . import type_prune
from .builtin_lowering import (
    body_blocks,
    functions,
    instruction,
    normalize,
    operation,
    require,
)

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

    def shape(function, call=None):
        graph = normalize({**function, "blocks": body_blocks(function)})
        del graph["id"], graph["return_buffer"]
        for block in graph["blocks"]:
            for inst in block["instructions"]:
                op = inst["operation"]
                if op["kind"] == "call":
                    require(
                        op["function"] == call, "unexpected storage clear call target"
                    )
                    op["function"] = 0
        return graph

    require(
        shape(after, clear["id"]) == shape(expected["write"], expected["clear"]["id"])
        and shape(clear) == shape(expected["clear"]),
        "storage writer or clear helper differs from the proved SSA expansion",
    )
    return [helper]
