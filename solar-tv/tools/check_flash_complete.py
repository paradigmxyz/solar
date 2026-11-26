"""Check complete pinned FlashLoan bytecode runs through callback and repayment."""

import argparse
import hashlib
import json
from dataclasses import dataclass
from functools import partial
from pathlib import Path

from validator import entry_proof, heap_entry, world

CALLER = 8738
TOKEN = 13107
MORPHO = 65536
CALLBACK_CALL = 6477
REPAYMENT_CALL = 3111
BASE = "CheckedFlashLoanCall"
FUEL = 512
TIMEOUT = 120
HEADER = """open SolarTV EvmRules Execution EVM.State MIR.TransferFromEntry
set_option autoImplicit false
set_option maxRecDepth 16384
set_option maxHeartbeats 16000000
set_option Elab.async false
"""


@dataclass(frozen=True)
class Scenario:
    name: str
    token: str
    callback: str
    payload: bytes
    result: bytes | None
    caller_slot: int
    token_slot: int


# On repayment, the last token returns false. Its first transfer writes slot zero.
SCENARIOS = (
    Scenario("Success", "602a5f5500", "60775f5500", b"\xde\xad\xbe", None, 119, 42),
    Scenario(
        "CallbackRevert",
        "602a5f5500",
        "60775f5563deadbeef5f526004601cfd",
        b"",
        bytes.fromhex("deadbeef"),
        0,
        0,
    ),
    Scenario(
        "RepaymentFalse",
        "5f3560e01c63a9059cbb146015575f5f5260205ff35b602a5f5500",
        "60775f5500",
        b"",
        bytes.fromhex("08c379a0")
        + (32).to_bytes(32)
        + (27).to_bytes(32)
        + b"transferFrom returned false".ljust(32, b"\0"),
        0,
        0,
    ),
)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def fixture_for(scenario, fixture):
    code = {CALLER: scenario.callback, TOKEN: scenario.token}
    accounts = [
        {**account, "code": list(bytes.fromhex(code[int(account["address"])]))}
        if int(account["address"]) in code
        else account
        for account in fixture["accounts"]
    ]
    data = bytes(fixture["calldata"])
    assert len(data) == 132 and not any(data[-32:])
    if scenario.payload:
        data = (
            data[:-32]
            + len(scenario.payload).to_bytes(32)
            + scenario.payload.ljust(32, b"\0")
        )
    return {**fixture, "accounts": accounts, "calldata": list(data)}


def payload(selector, *words):
    return bytes.fromhex(selector) + b"".join(word.to_bytes(32) for word in words)


def message(cut, data, destination):
    sample, address, value, start, size, out_offset, out_size = map(
        int, cut["stack"][:7]
    )
    assert (address, value, out_offset, out_size) == (destination, 0, 0, 0)
    assert bytes(cut["memory"][start : start + size]) == data
    return {"requested": sample, "destination": address, "input": data.hex()}


def observe(image, state, replay, scenario):
    result = heap_entry.observe(image, state, "cancun", replay, 30)
    failed = scenario.result is not None
    expected = ["revert", list(scenario.result)] if failed else ["success", []]
    assert result[:2] == expected
    assert len(result[5]) == (0 if failed else 1)
    accounts = {int(account[0]): account for account in result[4]}
    assert int(dict(accounts[CALLER][4]).get("0", 0)) == scenario.caller_slot
    assert int(dict(accounts[TOKEN][4]).get("0", 0)) == scenario.token_slot

    callback = heap_entry.observe(image, state, "cancun", replay, 30, pc=CALLBACK_CALL)
    data = payload("31f57072", 1, 64, len(scenario.payload))
    if scenario.payload:
        data += scenario.payload.ljust(32, b"\0")
    callback = message(callback, data, CALLER)
    repayment = None
    if scenario.name != "CallbackRevert":
        repayment = heap_entry.observe(
            image, state, "cancun", replay, 30, pc=REPAYMENT_CALL
        )
        repayment = message(repayment, payload("23b872dd", CALLER, MORPHO, 1), TOKEN)
    return result, callback, repayment


def check(directory, state, scenario, result):
    env = world.environment_expression(state)
    warm = ",".join(world.warm_addresses(state, "cancun"))
    accounts = world.accounts_expression(state)
    source = (
        f"import {BASE}\nimport SolarTV.MIR.Transforms.EntryTrace\n"
        + HEADER
        + f"""
def env : World.Environment := {env}
def replayAccounts : List Account := ({accounts} : List Account).map
  (fun account => if account.address = env.address then {{account with code := code.bytes}} else account)
def checkpoint : Journal := ⟨replayAccounts,replayAccounts,[{warm}],[],[],0⟩
def start : ByteCut := ⟨dispatchFrame env {state["gas"]} 0 [],checkpoint,[],[],[]⟩
def byteTailStart := byteExitStart start
def byteTailWith (hash : List Byte → Word) := chunkedByteExitStep tvWorldConfig code hash

structure Observation where
  stop : Stop
  data : List Byte
  gas : Nat
  callerSlot : Word
  tokenSlot : Word
  logs : List World.Log
  deriving DecidableEq

def observed (result : Result) : Observation :=
  let final := finishByte checkpoint result
  ⟨final.stop,final.data,final.gas,slotsRead (account final.world {CALLER}).storage 0,
    slotsRead (account final.world {TOKEN}).storage 0,final.world.logs⟩
"""
    )
    name = "FlashLoanComplete" + scenario.name
    prove = partial(entry_proof.lean, directory, namespace=name)
    entry_proof.success(prove(name, source, TIMEOUT, module=True), name)
    memo, checked, segments = entry_proof.phase(
        prove, name, (("byteTail", "ByteCut", "ByteExit", FUEL),), TIMEOUT
    )
    logs = (
        "["
        + ",".join(
            f"⟨{address},[{','.join(topics)}],{data}⟩"
            for address, topics, data in result[5]
        )
        + "]"
    )
    stop = ".success" if result[0] == "success" else ".reverted"
    source = (
        entry_proof.import_modules(checked)
        + HEADER
        + f"""
theorem charged_exit :
    (runByteExit tvWorldConfig {FUEL} start).toOption.map (observed ∘ ByteExit.result) =
      some ⟨{stop},{result[1]},{result[2]},{scenario.caller_slot},{scenario.token_slot},{logs}⟩ := by
  unfold runByteExit
  change (Trace.run (byteExitStep tvWorldConfig code.bytes) {FUEL} byteTailStart).toOption.map _ = _
  rw [← chunkedByteExitStep_eq tvWorldConfig code, ← {memo}.hashChecked_eq]
  change (Trace.run byteTailStep {FUEL} byteTailStart).toOption.map _ = _
  rw [byteTailReached]
  decide +kernel

#print axioms charged_exit
"""
    )
    proof = name + "Proof"
    verdict = prove(proof, source, TIMEOUT)
    (directory / f"{proof}.log").write_text(verdict.stdout + verdict.stderr)
    entry_proof.check_axioms(entry_proof.success(verdict, proof).splitlines(), 1)
    return {
        "segments": segments,
        "proof_sha256": digest((directory / f"{proof}.lean").read_bytes()),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--world", type=Path, required=True)
    parser.add_argument("--compiler", type=Path, required=True)
    parser.add_argument("--replay", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    directory = args.output.resolve()
    report = json.loads((directory / "FlashLoanTransferReturn.json").read_text())
    baseline = json.loads(args.baseline.read_text())["contracts"]["src/Morpho.sol"][
        "Morpho"
    ]
    image = bytes.fromhex(baseline["evm"]["deployedBytecode"]["object"])
    assert report["status"] == "verified" and report["runtime_sha256"] == digest(image)
    replay = str(args.replay.resolve())
    assert report["replay_sha256"] == digest(args.replay.read_bytes())
    base = directory / f"{BASE}.lean"
    assert report["call_proof_sha256"] == digest(base.read_bytes())
    verdict = entry_proof.lean(directory, BASE, base.read_text(), TIMEOUT, module=True)
    entry_proof.check_axioms(entry_proof.success(verdict, BASE).splitlines(), 16)
    fixture = world.read(args.world, str(args.compiler.resolve()), "exact")
    results = {}
    for scenario in SCENARIOS:
        state = fixture_for(scenario, fixture)
        result, callback, repayment = observe(image, state, replay, scenario)
        print(f"{scenario.name} replay matched, checking Lean", flush=True)
        results[scenario.name] = {
            **check(directory, state, scenario, result),
            "callback": callback,
            "repayment": repayment,
            "exit": result,
        }
    (directory / "flash-complete-replay.json").write_text(
        json.dumps(
            {
                "status": "verified",
                "runtime_sha256": digest(image),
                "replay_sha256": report["replay_sha256"],
                "scenarios": results,
            },
            indent=2,
        )
        + "\n"
    )
    print("Lean and Rust agree on all three complete FlashLoan runs", flush=True)


if __name__ == "__main__":
    main()
