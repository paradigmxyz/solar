"""Bind paid GAS/call proofs to the pinned permit and FlashLoan runtimes."""

import argparse
import json
from pathlib import Path

from check_gas_refinement import AMPLE, FORK, TIMEOUT, digest
from validator import audit, entry_proof, heap_entry, world
from validator.check import build_model
from validator.common import Unsupported

HEADER = """import SolarTV.EVMIR.State.FundedTrace
open SolarTV EvmRules Execution EVM.State
set_option autoImplicit false
set_option maxRecDepth 16384
"""

OPCODES = """(by rw [imageEq, atCut]; decide +kernel)
    (by rw [imageEq, atCut]; decide +kernel)"""

MODULE = "SolarTV.EVMIR.State.FundedTrace"


def permit(pc):
    return f"""
theorem funded_cut (frame : Frame) (world : Journal)
    (inputStart inputSize outStart outSize : Word) (tail : EVM.Stack)
    (imageEq : frame.code = image) (atCut : frame.pc = {pc})
    (shape : frame.stack = 1 :: inputStart :: inputSize :: outStart :: outSize :: tail)
    (room : tail.length+5 < 1024) (depth : frame.environment.depth < 1024)
    (empty : (account world 1).code = [])
    (input : expand frame.memory inputStart.toNat inputSize.toNat = .ok frame.memory)
    (output : expand frame.memory outStart.toNat outSize.toNat = .ok frame.memory) :
    let fee := callBoundaryCharge tvWorldConfig world ⟨.staticcall,1,0,[]⟩ 0xfa 0 0 0
    let data := Precompiles.Signatures.recover (readBytes frame.memory inputStart.toNat inputSize.toNat)
    let next := {{frame with
      pc := frame.pc+2
      stack := 1 :: tail
      returnData := data
      memory := writeBytes frame.memory outStart.toNat (data.take outSize.toNat)}}
    FundedTrace tvWorldConfig world frame (warmAddress world 1) next 2 (2+fee+3047) (2+fee+3000) :=
  funded_recovery tvWorldConfig world frame inputStart inputSize outStart outSize tail rfl rfl
    (by decide +kernel) (by decide +kernel)
    {OPCODES}
    shape room depth empty input output
"""


def flash_loan(pc):
    return f"""
open MIR.CallHandoff (Request)

theorem funded_cut (frame : Frame) (world : Journal) (offset size : Word) (tail : EVM.Stack)
    (imageEq : frame.code = image) (atCut : frame.pc = {pc})
    (shape : frame.stack = 13107 :: 0 :: offset :: size :: 0 :: 0 :: tail)
    (room : tail.length+6 < 1024) (depth : frame.environment.depth < 1024)
    (input : expand frame.memory offset.toNat size.toNat = .ok frame.memory)
    (output : expand frame.memory 0 0 = .ok frame.memory)
    (code : (account world 13107).code = [0x60,1,0x5f,0x52,0x60,32,0x5f,0xf3]) :
    let request : Request := ⟨0,13107,offset,size⟩
    let fee := MIR.CallHandoff.charge tvWorldConfig request world
    let next := {{frame with pc := frame.pc+2, stack := 1 :: tail, returnData := wordBytes 1}}
    FundedTrace tvWorldConfig world frame (warmAddress world 13107) next 8 (2+fee+16) (2+fee+16) :=
  funded_return_true tvWorldConfig world frame ⟨0,13107,offset,size⟩ tail rfl rfl
    (by decide +kernel) (by decide +kernel) (by decide +kernel) (by decide +kernel)
    (by decide +kernel) (by decide +kernel)
    {OPCODES}
    shape room depth input output code (by
      change precompile .cancun 13107 = false
      decide +kernel)
"""


def callback(pc):
    return f"""
open MIR.CallHandoff (Request)

theorem funded_cut (frame : Frame) (world : Journal)
    (requested offset size : Word) (tail : EVM.Stack)
    (imageEq : frame.code = image) (atCut : frame.pc = {pc})
    (shape : frame.stack = requested :: 8738 :: 0 :: offset :: size :: 0 :: 0 :: tail)
    (room : tail.length ≤ 1017) (depth : frame.environment.depth < 1024)
    (input : expand frame.memory offset.toNat size.toNat = .ok frame.memory)
    (output : expand frame.memory 0 0 = .ok frame.memory)
    (code : (account world 8738).code = [0]) :
    let request : Request := ⟨requested,8738,offset,size⟩
    let fee := MIR.CallHandoff.charge tvWorldConfig request world
    let next := {{frame with pc := frame.pc+1, stack := 1 :: tail, returnData := []}}
    FundedTrace tvWorldConfig world frame (warmAddress world 8738) next 2 fee fee ∧
    ∀ fuel gas, gas < fee →
      run (fuel+1) tvWorldConfig world {{frame with gas}} = .ok ⟨.outOfGas,[],world,0,1⟩ :=
  funded_stop_call tvWorldConfig world frame ⟨requested,8738,offset,size⟩ tail rfl rfl
    (by decide +kernel) (by decide +kernel) (by rw [imageEq, atCut]; decide +kernel)
    shape room depth input output code (by
      change precompile .cancun 8738 = false
      decide +kernel)
"""


def cuts(image, pc, fixture, replay, minimum, debit, operands, span):
    first = heap_entry.observe(image, fixture, FORK, replay, 30, pc=pc)
    prefix_fee = fixture["gas"] - first["gas"]
    rows = []
    for budget in (prefix_fee + minimum - 1, prefix_fee + minimum, AMPLE):
        state = {**fixture, "gas": budget}
        before = heap_entry.observe(image, state, FORK, replay, 30, pc=pc)
        paid = before["gas"] >= minimum
        try:
            after = heap_entry.observe(image, state, FORK, replay, 30, pc=pc + span)
        except Unsupported as error:
            if (
                span != 1
                or paid
                or str(error)
                != "Rust replay: bytecode probe was not reached (OutOfGas)"
            ):
                raise
            assert heap_entry.observe(image, state, FORK, replay, 30)[0] == "out-of-gas"
            rows.append(
                {
                    "entry_gas": budget,
                    "gas_at_boundary": before["gas"],
                    "gas_after_call": None,
                    "flag": None,
                    "terminal": "out-of-gas",
                }
            )
            continue
        assert after["stack"] == [str(int(paid)), *before["stack"][operands:]]
        if paid:
            assert before["gas"] - after["gas"] == debit
        rows.append(
            {
                "entry_gas": budget,
                "gas_at_boundary": before["gas"],
                "gas_after_call": after["gas"],
                "flag": str(int(paid)),
            }
        )
    return rows


def gas_sensitive_control(fixture, images, replay):
    """An admitted caller cannot erase an arbitrary child's gas observation."""
    token_address = 13107
    # GAS; PUSH0; SSTORE; then the same return-true suffix.
    token = list(bytes.fromhex("5a5f5560015f5260205ff3"))
    state = {
        **fixture,
        "accounts": [
            {**account, "code": token}
            if int(account["address"]) == token_address
            else account
            for account in fixture["accounts"]
        ],
    }
    results = [heap_entry.observe(image, state, FORK, replay, 30) for image in images]
    before, after = results
    assert before[0] == after[0] == "success"
    assert before[1] == after[1] and before[5] == after[5]
    observations = []
    for result in results:
        (account,) = (a for a in result[4] if int(a[0]) == token_address)
        ((key, gas),) = account[4]
        assert int(key) == 0 and account[3] == token
        observations.append(int(gas))
    assert observations[0] != observations[1]
    return {
        "entry_gas": state["gas"],
        "child_code": bytes(token).hex(),
        "stored_gas": observations,
        "rust_observations": results,
    }


def check(args, case, name, theorem, minimum, debit, operands, *, repayment=False):
    root = args.worlds / case
    manifest = json.loads((root / "physical-calls.json").read_text())
    fixture = world.read(root / "world.json", str(args.compiler), "exact")
    if case == "solady-weth":
        # The original fixture's invalid-signature route with zero r.
        fixture["calldata"][-64:-32] = [0] * 32
    reports, images = [], []
    for side in ("before", "after"):
        (compiled,) = json.loads((root / side / "combined.json").read_text())[
            "contracts"
        ].values()
        image = bytes.fromhex(compiled["bin-runtime"])
        images.append(image)
        sha = digest(image)
        assert sha == manifest[side]["runtime_sha256"]
        sites = [("sample", 0, theorem, 2, minimum, debit, operands)]
        if repayment:
            sites.extend(
                (
                    ("callback", 1, callback, 1, 100, 100, 7),
                    ("repayment", 2, theorem, 2, minimum, debit, operands),
                )
            )
        for boundary, index, proof, span, admission, charge, arguments in sites:
            pc = manifest[side]["calls"][index]["pc"] + 1 - span
            rows = cuts(
                image, pc, fixture, args.replay, admission, charge, arguments, span
            )
            source = (
                HEADER
                + entry_proof.image_definitions(image)
                + world.configuration(
                    str(args.compiler), FORK, fixture, 30, args.replay
                )
                + proof(pc)
                + "\n#print axioms funded_cut\n"
            )
            result = entry_proof.lean(
                args.output, name + boundary.title() + side.title(), source, TIMEOUT
            )
            checked = entry_proof.success(result, f"{case} {side}")
            entry_proof.check_axioms(checked.splitlines(), 1)
            print(f"{case} {side} PC {pc}: {checked.strip()}\n{rows}", flush=True)
            reports.append(
                {
                    "side": side,
                    "boundary": boundary,
                    "runtime_sha256": sha,
                    "pc": pc,
                    "rust": rows,
                    "proof_sha256": digest(source.encode()),
                }
            )
    control = (
        gas_sensitive_control(fixture, images, args.replay)
        if case == "morpho-blue"
        else None
    )
    return {"boundaries": reports, "gas_sensitive_control": control}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for flag in ("worlds", "output", "compiler", "replay"):
        parser.add_argument(f"--{flag}", required=True, type=Path)
    args = parser.parse_args()
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    semantics = audit.semantics()
    entry_proof.success(build_model(TIMEOUT, MODULE), MODULE)
    cases = {
        "solady-weth": check(args, "solady-weth", "Permit", permit, 3149, 3102, 5),
        "morpho-blue": check(
            args, "morpho-blue", "FlashLoan", flash_loan, 118, 118, 6, repayment=True
        ),
    }
    assert audit.semantics() == semantics
    report = {"semantics": semantics, "checks": cases}
    (args.output / "funded-cuts.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
