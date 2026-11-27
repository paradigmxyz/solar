"""Compose a dynamic word-array ABI decoder with a checked cyclic MIR body."""

from copy import deepcopy

from . import mir_cfg, mir_effects, phi_edges
from .common import Unsupported

WORD_ARRAY = {
    "block_copy": False,
    "element": {"kind": "word", "rule": {"bits": 0, "function": False, "kind": "any"}},
    "kind": "array",
    "length": None,
    "location": 0,
}


def array_entry(header, body):
    params, returns = body["params"], body["returns"]
    return (
        "abi_params=[array<_, bytes32>]" in header
        and "abi_returns=[memory_array<word>]" in header
        and "abi_return_params=[array<_, bytes32>]" in header
        and len(params) == len(returns) == 1
        and params[0].get("kind") == "slice"
        and params[0].get("location") == "calldata"
        and returns[0].get("kind") == "pointer"
        and returns[0].get("location") == "memory"
    )


def restrict(function, group):
    graph = deepcopy(function)
    indices = {block: index for index, block in enumerate(group)}
    graph["blocks"] = [graph["blocks"][index] for index in group]
    try:
        for block in graph["blocks"]:
            for phi in block["phis"]:
                phi["incoming"] = [
                    [indices[pred], value] for pred, value in phi["incoming"]
                ]
            term = block["terminator"]
            if term["kind"] == "jump":
                term["target"] = indices[term["target"]]
            elif term["kind"] == "branch":
                for leg in ("then", "else"):
                    term[leg] = indices[term[leg]]
            elif term["kind"] not in {"return", "revert"}:
                raise Unsupported("unmodeled word-array entry exit")
    except KeyError as error:
        raise Unsupported("word-array entry split has a crossing edge") from error
    return graph


def split(before, wrapper):
    blocks = wrapper["blocks"]
    splits = [
        (index, block, inst)
        for index, block in enumerate(blocks)
        for inst in block["instructions"]
        if inst["operation"]["kind"] == "make_slice"
        and inst["type"] == before["params"][0]
    ]
    if len(splits) != 1:
        raise Unsupported("word-array ABI proof needs one decoded slice")
    index, block, decoded = splits[0]
    term = block["terminator"]
    if block["instructions"][-1] != decoded or term["kind"] != "jump":
        raise Unsupported("word-array ABI proof needs a direct decoder-to-body edge")
    entry = term["target"]
    visited = set()
    pending = [entry]
    while pending:
        position = pending.pop()
        if position not in visited:
            visited.add(position)
            pending.extend(phi_edges.targets(blocks[position]["terminator"]))
    if 0 in visited or index in visited or blocks[entry]["phis"]:
        raise Unsupported("word-array ABI proof needs a detached body entry")
    body = restrict(wrapper, [entry, *sorted(visited - {entry})])
    body["params"], body["returns"] = before["params"], before["returns"]
    definitions = {
        inst["result"]
        for body_block in body["blocks"]
        for inst in body_block["phis"] + body_block["instructions"]
        if inst["result"] is not None
    }

    def bind(value):
        if isinstance(value, list):
            return [bind(item) for item in value]
        if isinstance(value, dict):
            if value.get("kind") == "local":
                if (
                    value["index"] == decoded["result"]
                    and value["type"] == decoded["type"]
                ):
                    return {**value, "kind": "arg", "index": 0}
                if value["index"] not in definitions:
                    raise Unsupported("word-array body uses another decoder local")
            return {key: bind(item) for key, item in value.items()}
        return value

    body["blocks"] = bind(body["blocks"])
    prefix = deepcopy(wrapper)
    values = decoded["operation"]["args"]
    prefix["returns"] = [value["type"] for value in values]
    prefix["blocks"][index]["instructions"].pop()
    prefix["blocks"][index]["terminator"] = {"kind": "return", "args": values}
    decoder = restrict(prefix, [i for i in range(len(blocks)) if i not in visited])
    return decoder, body


def code(before, target, opcodes, fork, prelude, local_guard):
    wrapper = next(
        body for body in target["functions"] if body["id"] == target["entry_id"]
    )
    decoder, body = split(before, wrapper)
    if any(
        inst["operation"]["kind"] == "call"
        for function in (before, body)
        for block in function["blocks"]
        for inst in block["instructions"]
    ):
        raise Unsupported("word-array ABI loop proof requires call-free bodies")
    old, new = map(mir_cfg.import_graph, (before, body))
    points, edges, live, limit, word_rewrites, increment_rewrites = mir_cfg.pairing(
        old, new
    )
    if word_rewrites or increment_rewrites:
        raise Unsupported("word-array ABI loop proof requires preserved body actions")
    witnesses = [
        mir_cfg.render_point(point, successors, locals_)
        for point, successors, locals_ in zip(points, edges, live, strict=True)
    ]
    target_decoder = mir_effects.render(
        {"entry_id": decoder["id"], "functions": [decoder]}, opcodes, abi_fork=fork
    )
    guard = (
        "if environment.value ≠ 0#256 then MIR.Effects.abiDecodeFailure else\n  "
        if local_guard
        else ""
    )
    split_value = (
        "by_cases nonzero : environment.value ≠ 0#256 <;> " if local_guard else ""
    )
    theorem = f"""
def sourceDecoder (environment : World.Environment) : MIR.Effects.Tree :=
  {guard}if environment.input.length < 36 then MIR.Effects.abiDecodeFailure else
    let size := BitVec.ofNat 256 environment.input.length
    let offset := Execution.readWord environment.input 4
    let head := offset + 4
    let start := head + 32
    let count := Execution.readWord environment.input head.toNat
    if (size - 4 - 32).toNat < offset.toNat then MIR.Effects.abiDecodeFailure
    else if ((size - start) >>> 5).toNat < count.toNat then MIR.Effects.abiDecodeFailure
    else .done ⟨"return", [(256,start),(256,count)]⟩
def targetDecoder (environment : World.Environment) : MIR.Effects.Tree :=
{target_decoder}
theorem sameDecoder (environment : World.Environment) (bounded : environment.input.length < 2^256) :
    sourceDecoder environment = targetDecoder environment := by
  {split_value}by_cases short : environment.input.length < 36 <;>
    simp [*, sourceDecoder, targetDecoder, MIR.Effects.abiDecodeFailure,
      MIR.compare, MIR.eval, MIR.shift, Word.shift, BitVec.ult_iff_toNat_lt,
      BitVec.toNat_ofNat, Nat.mod_eq_of_lt bounded, Decidable.imp_iff_not_or]
theorem composed {{V S R O : Type}} (env : World.Environment)
    (bounded : env.input.length < 2^256)
    (handler : MIR.Effects.Request → S → Except R (Word × S))
    (reject : MIR.Effects.Boundary → S → R)
    (bind : MIR.Effects.Boundary → Locals V)
    (bodyHandler : Handler V S O) (finish : MIR.CfgFlow.Result O → R)
    (work : Nat) (state : S) :
    MIR.Effects.interpret handler (fun boundary state =>
      if boundary.kind = "return" then
        finish (MIR.CfgFlow.run before0 {limit} bodyHandler work 0 (bind boundary) state)
      else reject boundary state) (sourceDecoder env) state =
    MIR.Effects.interpret handler (fun boundary state =>
      if boundary.kind = "return" then
        finish (MIR.CfgFlow.run after0 {limit} bodyHandler work 0 (bind boundary) state)
      else reject boundary state) (targetDecoder env) state := by
  rw [sameDecoder env bounded]
  congr 1
  funext boundary state
  rw [preserved0 bodyHandler work (bind boundary) (bind boundary) state]
#print axioms sameDecoder
#print axioms composed
"""
    return (
        prelude
        + "open MIR.CfgFlow\nset_option maxRecDepth 16384\n"
        + mir_cfg.certificate(0, old, new, witnesses, limit)
        + theorem,
        {"kind": "kernel-checked-abi-loop-entry", "paired_points": len(points)},
    )
