"""Bind a terminal word or byte object to the ABI return observations."""

from copy import deepcopy

from . import abi_effects, abi_loop, mir_cfg, mir_effects
from .common import Unsupported

BYTE_OBJECT = {"kind": "bytes", "location": 0}


def source_tail(model):
    entry = next(body for body in model["functions"] if body["id"] == model["entry_id"])
    layouts = [
        inst["operation"].get("shapes")
        for block in entry["blocks"]
        for inst in block["instructions"]
        if inst["operation"]["kind"] == "abi_encode_tree"
    ]
    if layouts == [[BYTE_OBJECT]]:
        is_bytes, layout = True, BYTE_OBJECT
    elif layouts == [[abi_loop.WORD_ARRAY]]:
        is_bytes, layout = False, abi_loop.WORD_ARRAY
    else:
        raise Unsupported(
            "ABI zero-copy proof needs one terminal byte or word-array encoder"
        )
    source = abi_effects.scratch_returns(model, layout, "abi_encode_return_object")
    return source, is_bytes


def target_tail(model):
    model = deepcopy(model)
    body = next(body for body in model["functions"] if body["id"] == model["entry_id"])
    targets = [
        block for block in body["blocks"] if block["terminator"]["kind"] == "returndata"
    ]
    if len(targets) != 1:
        raise Unsupported("ABI zero-copy proof needs one lowered return")
    (block,) = targets
    instructions = block["instructions"]
    starts = [
        i
        for i, inst in enumerate(instructions)
        if inst["operation"]["kind"] == "opcode"
        and inst["operation"]["name"] == "mload"
    ]
    if not starts:
        raise Unsupported("ABI zero-copy proof needs the original length word")
    start = starts[-1]
    (object_,) = instructions[start]["operation"]["args"]
    ty = object_["type"]
    if (
        ty.get("kind") != "pointer"
        or ty.get("location") != "memory"
        or ty.get("bits") != 256
    ):
        raise Unsupported("ABI zero-copy requires a typed memory object")
    suffix = deepcopy(block)
    suffix["instructions"] = suffix["instructions"][start:]
    suffix["phis"] = []
    defined = {
        inst["result"] for inst in suffix["instructions"] if inst["result"] is not None
    }

    def operand(value):
        if value == object_:
            return {"kind": "arg", "type": ty, "index": 0}
        if (
            value["kind"] == "arg"
            or value["kind"] == "local"
            and value["index"] not in defined
        ):
            raise Unsupported("ABI zero-copy tail uses another incoming value")
        return value

    for inst in suffix["instructions"]:
        inst["operation"]["args"] = [
            operand(value) for value in inst["operation"]["args"]
        ]
    suffix["terminator"]["args"] = [
        operand(value) for value in suffix["terminator"]["args"]
    ]
    block["instructions"] = instructions[:start]
    block["terminator"] = {"kind": "abi_encode_return_object", "args": [object_]}
    tail = {**body, "params": [ty], "returns": [], "blocks": [suffix]}
    return model, {"entry_id": tail["id"], "functions": [tail]}


def known_slices(body):
    body = deepcopy(body)
    slices = {}
    for block in body["blocks"]:
        for inst in block["instructions"]:
            op = inst["operation"]
            if op["kind"] == "make_slice":
                slices[inst["result"]] = op["args"]
    aliases = {}
    for block in body["blocks"]:
        kept = []
        for inst in block["instructions"]:
            op = inst["operation"]
            if op["kind"] == "extract_value":
                (aggregate,) = op["args"]
                if aggregate["kind"] == "local" and aggregate["index"] in slices:
                    (pointer, length) = slices[aggregate["index"]]
                    if op["index"] not in (0, 1):
                        raise Unsupported("ABI zero-copy slice projection index")
                    value = (pointer, length)[op["index"]]
                    if value["type"] != inst["type"] or value["kind"] == "undefined":
                        raise Unsupported("ABI zero-copy slice carrier")
                    aliases[inst["result"]] = value
                    continue
            kept.append(inst)
        block["instructions"] = kept

    def resolve(value):
        if isinstance(value, list):
            return [resolve(item) for item in value]
        if isinstance(value, dict):
            if value.get("kind") == "local" and value["index"] in aliases:
                return resolve(aliases[value["index"]])
            return {key: resolve(item) for key, item in value.items()}
        return value

    body["blocks"] = resolve(body["blocks"])
    return body


def proof(models, definitions, fork, prelude):
    old, actual = models
    source, is_bytes = source_tail(old)
    target, tail = target_tail(actual)
    retained, tables = abi_effects.unchanged_callees(old, actual)
    if any(
        body["id"] != model["entry_id"] and body["id"] not in retained
        for model in models
        for body in model["functions"]
    ):
        raise Unsupported("ABI zero-copy proof needs unchanged closed callees")
    bodies = [
        known_slices(
            next(body for body in model["functions"] if body["id"] == model["entry_id"])
        )
        for model in (source, target)
    ]
    old_graph, new_graph = map(mir_cfg.import_graph, bodies)
    points, edges, live, limit, word_rewrites, increment_rewrites = mir_cfg.pairing(
        old_graph, new_graph
    )
    if word_rewrites or increment_rewrites:
        raise Unsupported("ABI zero-copy proof requires preserved body actions")
    witnesses = [
        mir_cfg.render_point(point, successors, locals_)
        for point, successors, locals_ in zip(points, edges, live, strict=True)
    ]
    generated = mir_effects.render(tail, definitions, abi_fork=fork)
    byte_object = str(is_bytes).lower()
    return (
        prelude
        + (abi_effects.retained_callee_code(tables, definitions) if retained else "")
        + """
open MIR.CfgFlow MIR.LowerAbiEncode.ZeroCopy
set_option maxRecDepth 16384
set_option maxHeartbeats 5000000
"""
        + mir_cfg.certificate(0, old_graph, new_graph, witnesses, limit)
        + f"""
def actualReuse (environment : World.Environment) (arg0 : Word) : MIR.Effects.Tree :=
{generated}
theorem checkedTail (environment : World.Environment) (object : Word) :
    actualReuse environment object = MIR.LowerAbiEncode.reuseReturn {byte_object} object := by
  simp [actualReuse, MIR.LowerAbiEncode.reuseReturn, MIR.LowerAbiEncode.load,
    MIR.LowerAbiEncode.store, MIR.eval, MIR.cast, MIR.shift, Word.shift, Word.binary,
    MIR.LowerAbiEncode.preceding, BitVec.add_assoc]

def observe {{A : Type}} (encode : Layout {byte_object} → Except String (List Execution.Byte))
    (result : Result (A ⊕ (A × Layout {byte_object}))) :
    Result (A ⊕ (A × Except String (List Execution.Byte))) :=
  match result with
  | .invalid => .invalid
  | .exhausted => .exhausted
  | .halted (.inl failure) => .halted (.inl failure)
  | .halted (.inr (state,view)) =>
    .halted (.inr (state,encode view))

theorem observed {{V S A : Type}} (environment : World.Environment)
    (handler : Handler V S (A ⊕ (A × Layout {byte_object})))
    (work : Nat) (locals : Locals V) (state : S) :
    observe sourceOutput (MIR.CfgFlow.run before0 {limit} handler work 0 locals state) =
      observe (fun view => runTail (actualReuse environment (BitVec.ofNat 256 view.object)) view.memory)
        (MIR.CfgFlow.run after0 {limit} handler work 0 locals state) := by
  rw [preserved0 handler work locals locals state]
  cases MIR.CfgFlow.run after0 {limit} handler work 0 locals state with
  | invalid | exhausted => rfl
  | halted exit =>
    cases exit with
    | inl failure => rfl
    | inr value =>
      obtain ⟨state,view⟩ := value
      simp only [observe, checkedTail, run_reuse, output view]
#print axioms checkedTail
#print axioms observed
""",
        {
            "paired_points": len(points),
            "word_array": not is_bytes,
            "retained_callees": retained,
        },
    )
