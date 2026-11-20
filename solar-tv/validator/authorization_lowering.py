"""Bind the signing envelope and recovery call proofs to a captured MIR pair."""

from . import mir_state
from .builtin_lowering import (
    body_blocks,
    constant,
    expect,
    instruction,
    normalize,
    opcode,
    operation,
    pointer,
    require,
    slice_type,
    word,
)


def is_recovery(op):
    return (
        op["kind"] == "extcall"
        and op["name"] == "staticcall"
        and op["output_word"]
        and not op["gas"]
        and op["args"][:1] == [constant(1, word(160))]
    )


def unique_site(body, matches):
    sites = [
        (block, index, inst)
        for block, data in enumerate(body["blocks"])
        for index, inst in enumerate(data["instructions"])
        if matches(inst["operation"])
    ]
    require(len(sites) == 1, "expected one signing expansion site of each kind")
    return sites[0]


def contract_envelope(old, instructions, index):
    expanded = instructions[index : index + 8]
    require(len(expanded) == 8, "incomplete packed signing envelope")
    alloc, header, prefix, tag, first, write_domain, second, write_digest = expanded
    obj = expect(
        alloc,
        operation("allocate", [constant(128)], zeroed=False, checked=False),
        pointer(),
    )
    expect(header, opcode("mstore", obj, constant(66)))
    prefix = expect(prefix, operation("pointer_add", [obj, constant(32)]), pointer())
    expect(tag, opcode("mstore", prefix, constant(0x1901 << 240)))
    first = expect(first, operation("pointer_add", [obj, constant(34)]), pointer())
    second = expect(second, operation("pointer_add", [obj, constant(66)]), pointer())
    stores = [write_domain["operation"]["args"], write_digest["operation"]["args"]]
    require(
        all(len(args) == 2 and args[1]["type"] == word() for args in stores),
        "unexpected packed signing operands",
    )
    domain, digest = (args[1] for args in stores)
    expect(write_domain, opcode("mstore", first, domain))
    expect(write_digest, opcode("mstore", second, digest))
    packed = operation(
        "packed",
        [domain, digest],
        hash=False,
        parts=[
            {"kind": "literal", "bytes": [0x19, 0x01]},
            {"kind": "scalar", "index": 0, "size": 32, "fixed": True},
            {"kind": "scalar", "index": 1, "size": 32, "fixed": True},
        ],
    )
    expect(old, {**packed, "args": old["operation"]["args"]}, pointer())
    require(
        [arg["type"] for arg in old["operation"]["args"]] == [word(), word()],
        "unexpected semantic signing operands",
    )
    instructions[index : index + 8] = [instruction(packed, obj["index"], pointer())]


def contract_recovery(old, instructions, index):
    expanded = instructions[index : index + 5]
    require(len(expanded) == 5, "incomplete static word call")
    offset, length, clear, gas, raw = expanded
    ty = slice_type("memory")
    args = offset["operation"]["args"]
    require(len(args) == 1 and args[0]["type"] == ty, "unexpected recovery slice")
    input_ = args[0]
    pointer_ = expect(
        offset,
        operation("extract_value", [input_], aggregate_type=ty, index=0),
        pointer(),
    )
    size = expect(
        length, operation("extract_value", [input_], aggregate_type=ty, index=1), word()
    )
    expect(clear, opcode("mstore", constant(0), constant(0)))
    sampled = expect(gas, opcode("gas"), word())
    success = expect(
        raw,
        opcode(
            "staticcall",
            sampled,
            constant(1),
            pointer_,
            size,
            constant(0),
            constant(32),
        ),
        word(1),
    )
    address = constant(1, word(160))
    call = operation(
        "extcall",
        [address, input_],
        name="staticcall",
        clear_word=True,
        gas=False,
        input=True,
        output_word=True,
        value=False,
    )
    old_args = old["operation"]["args"]
    require(
        len(old_args) == 2 and old_args[1]["type"] == ty,
        "invalid semantic recovery slice",
    )
    expect(old, {**call, "args": old_args}, word(1))
    instructions[index : index + 5] = [instruction(call, success["index"], word(1))]
    return old_args[1]


def source(before, after):
    before, after = (
        normalize({**body, "blocks": body_blocks(body)}) for body in (before, after)
    )
    packed_block, packed_index, packed = unique_site(
        before, lambda op: op["kind"] == "packed"
    )
    call_block, call_index, call = unique_site(before, is_recovery)
    require(packed_block != call_block, "overlapping signing expansion blocks")
    graph = body_blocks(after)
    require(
        packed_block in graph and call_block in graph, "missing signing expansion block"
    )
    contract_envelope(packed, graph[packed_block]["instructions"], packed_index)
    input_ = contract_recovery(call, graph[call_block]["instructions"], call_index)
    require(
        before == normalize({**after, "blocks": graph}),
        "signing expansion changes the SSA context",
    )
    return PROOF.replace("{input}", mir_state.operand(input_)).replace(
        "{address}", mir_state.operand(constant(1, word(160)))
    )


PROOF = """
open SolarTV.MIR.State SolarTV.MIR.LowerPacked SolarTV.MIR.LowerStaticWord
open SolarTV.MIR.LowerReturndata (invoke)

theorem contextual :
    (∀ (frame : Frame) (domain digest : Word),
      abiPacked frame signingParts false [domain,digest] =
        loweredSigningEnvelope frame domain digest) ∧
    (∀ (cfg : Config) (fuel : Nat) (frame : Frame) (pointer length : Scalar),
      readValue frame {input} = .ok (.aggregate [.scalar pointer,.scalar length]) →
      cfg.available.contains 0xfa = true →
      cfg.fork.atLeast .amsterdam = false →
      cfg.fork.atLeast .tangerineWhistle = true →
      ∀ (before budget : Nat) (budgets : List Nat),
        cfg.layout.gasBefore = some before → 2 ≤ before →
        budget ≤ before-2 → before-2 ≤ 30000000 →
        cfg.messageGas = some budgets →
        budgets[frame.messageGas.length]? = some budget →
        invoke cfg fuel
          (messageArguments cfg frame (.address 0xfa false false true true true)
            [{address},{input}]) = lowered cfg fuel frame 1 pointer.word length.word) := by
  refine ⟨signingEnvelope, ?_⟩
  intro cfg fuel frame pointer length hi available old eip150 before budget budgets
    sampled paid cap limit ledger selected
  exact staticWord cfg fuel frame _ _ 1 pointer length rfl hi available old eip150
    before budget budgets sampled paid cap limit ledger selected

#print axioms contextual
"""
