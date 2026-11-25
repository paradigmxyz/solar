"""Check an explicit transaction or Amsterdam gas fixture against evm2."""

import argparse
import hashlib
import json
import os
import subprocess
import tempfile
from pathlib import Path

from . import audit, entry_proof, native, world
from .common import Unsupported

SET_CODE = "SolarTV.EVMIR.State.Transactions.SetCode"
RESERVOIR = "SolarTV.EVMIR.State.Transactions.Reservoir"
NESTED_RESERVOIR = "SolarTV.EVMIR.State.Transactions.NestedReservoir"


def natural(value):
    number = value if type(value) is int else int(value, 0)
    if not 0 <= number < 2**256:
        raise ValueError("expected an unsigned 256-bit integer")
    return number


def bytes_of(value):
    return list(bytes.fromhex(value.removeprefix("0x")))


def account(row):
    return (
        f"{{address := {natural(row['address'])},nonce := {row['nonce']},"
        f"balance := {natural(row['balance'])},code := {bytes_of(row['code'])}}}"
    )


def authorization(row):
    fields = ("chainId", "address", "nonce", "yParity", "r", "s")
    return "⟨" + ",".join(str(natural(row[key])) for key in fields) + "⟩"


def set_code_source(request, observed, solar, executable, timeout, directory):
    version = request["fork"]
    if version not in {"prague", "osaka"}:
        raise Unsupported("set-code checking covers Prague through Osaka")
    fixture = directory / "world.json"
    fixture.write_text(
        json.dumps(
            {
                "version": 1,
                "address": natural(request["tx"]["to"]),
                "block": {
                    "gas_limit": request["block_gas_limit"],
                    "base_fee": natural(request["base_fee"]),
                    "coinbase": natural(request.get("beneficiary", 0)),
                },
            }
        )
    )
    fixture = world.read(fixture, solar, "exact")
    cfg = world.configuration(solar, version, fixture, timeout, executable)
    tx = request["tx"]
    fields = (
        "to",
        "chainId",
        "nonce",
        "gas",
        "maxFeePerGas",
        "maxPriorityFeePerGas",
        "value",
    )
    args = ",".join(str(natural(tx[key])) for key in fields)
    accounts = "[" + ",".join(map(account, request["accounts"])) + "]"
    access = (
        "["
        + ",".join(
            f"({natural(row['address'])},{list(map(natural, row['storageKeys']))})"
            for row in tx["accessList"]
        )
        + "]"
    )
    authorities = []
    bindings = []
    for i, (auth, signer) in enumerate(
        zip(tx["authorizationList"], observed["authorities"], strict=True)
    ):
        resolved = "none" if signer is None else f"some {natural(signer)}"
        authorities.append(
            f"def auth{i} : Authorization := {authorization(auth)}\n"
            f"theorem recovered{i} : authority auth{i} = {resolved} := by decide +kernel\n"
            f"#print axioms recovered{i}\n"
        )
        bindings.append(f"(auth{i},{resolved})")
    authorizations = "[" + ",".join(f"auth{i}" for i in range(len(authorities))) + "]"
    reductions = "".join(f",recovered{i}" for i in range(len(authorities)))
    expected = "none"
    if observed["admitted"]:
        selected = (
            "["
            + ",".join(
                "none" if row is None else f"some ({row[0]},{row[1]})"
                for row in observed["accounts"]
            )
            + "]"
        )
        expected = (
            f"(some ⟨{str(observed['success']).lower()},{observed['output']},"
            f"{observed['spent']},{observed['refund']},{observed['floor_gas']},"
            f"{observed['gas_used']},{observed['execution_gas']},"
            f"{observed['state_gas']},{natural(observed['sender_balance'])},"
            f"{natural(observed['beneficiary_balance'])},{selected}⟩)"
        )
    return f"""import {SET_CODE}
open SolarTV EvmRules Execution EVM.Transaction.SetCode
set_option maxRecDepth 16384
set_option maxHeartbeats 12000000
set_option exponentiation.threshold 1024
{cfg}
def startAccounts : List Account := {accounts}
def initial : Journal := ⟨startAccounts,startAccounts,[],[],[],0⟩
{"".join(authorities)}
def tx : Envelope := ⟨{natural(request["sender"])},{args},{bytes_of(tx["input"])},{access},{authorizations}⟩
theorem recovery :
    tx.authorizations.map (fun auth => (auth,authority auth)) = [{",".join(bindings)}] := by
  simp only [tx,List.map_cons,List.map_nil{reductions}]
def inspect : List Address := {list(map(natural, request["inspect"]))}
def selected (state : Journal) : List (Option (Nat × List Byte)) :=
  inspect.map fun signer =>
    (state.accounts.find? (·.address == signer)).map (fun account => (account.nonce,account.code))
structure Observation where
  success : Bool
  data : List Byte
  spent : Nat
  refund : Nat
  floor : Nat
  gasUsed : Nat
  executionGas : Nat
  stateGas : Nat
  senderBalance : Word
  beneficiaryBalance : Word
  accounts : List (Option (Nat × List Byte))
  deriving DecidableEq
def observation : Except String (Option Observation) :=
  match prepare tvWorldConfig.fork 1 {request["block_gas_limit"]} {natural(request["base_fee"])}
    tvWorldConfig.block.beneficiary initial tx with
  | .error _ => .ok none
  | .ok ready => do
    let (result,gas) ← executeSettled 512 tvWorldConfig tx ready
    return some ⟨result.stop == .success,result.data,gas.spent,gas.refund,gas.floor,gas.receipt,
      gas.execution,gas.stateSpent,
      (account result.world tx.signer).balance,
      (account result.world tvWorldConfig.block.beneficiary).balance,selected result.world⟩
theorem matches_replay : observation.toOption = some {expected} := by
  unfold observation prepare
  rw [recovery]
  decide +kernel
#print axioms matches_replay
"""


def charge_source(kind, cost):
    if kind == "rollback":
        return ".rollback"
    if kind not in {"execution", "state", "refill"}:
        raise Unsupported("unknown reservoir charge")
    return f".{kind} {natural(cost)}"


def reservoir_source(request, observed):
    charges = [
        charge_source(*row) for debit in request["charges"] for row in debit.items()
    ]
    meters = (
        []
        if observed is None
        else [
            "none" if row is None else f"some ⟨{','.join(map(str, row))}⟩"
            for row in observed
        ]
    )
    expected = "none" if observed is None else f"some [{','.join(meters)}]"
    return f"""import {RESERVOIR}
open SolarTV.EVM.Transaction.Reservoir
def charges : List Charge := [{",".join(charges)}]
theorem matches_replay :
    (begin {natural(request["limit"])} {natural(request["intrinsic"])}).map
      (fun meter => trace meter charges) = {expected} := by decide
#print axioms matches_replay
"""


def frame_action(kind, value):
    if kind == "enter":
        return ".enter " + " ".join(
            str(natural(value[key])) for key in ("forward", "stipend", "reserved")
        )
    if kind == "finish" and value in {"success", "revert", "halt"}:
        return f".finish .{value}"
    if kind == "refund" and type(value) is int:
        return f".refund ({value})"
    return f".charge ({charge_source(kind, value)})"


def nested_reservoir_source(request, observed):
    actions = [
        frame_action(*row) for action in request["actions"] for row in action.items()
    ]
    meters = (
        []
        if observed is None
        else [
            "none"
            if row is None
            else f"some (⟨{','.join(map(str, row[:4]))}⟩,{row[4]},{row[5]})"
            for row in observed
        ]
    )
    expected = "none" if observed is None else f"some [{','.join(meters)}]"
    return f"""import {NESTED_RESERVOIR}
open SolarTV.EVM.Transaction.Reservoir
def actions : List FrameAction := [{",".join(actions)}]
theorem matches_replay :
    (begin {natural(request["limit"])} {natural(request["intrinsic"])}).map
      (fun meter => frameTrace ⟨⟨meter,0⟩,[]⟩ actions) = {expected} := by decide
#print axioms matches_replay
"""


def check(request, solar, timeout):
    if request["kind"] == "amsterdam_storage":
        from . import amsterdam_storage

        return amsterdam_storage.check(request, solar, timeout)
    from .check import build_model

    helper = native.default(solar)
    replay = subprocess.run(
        [helper, "--transaction"],
        input=json.dumps(request),
        text=True,
        capture_output=True,
        timeout=timeout,
        check=False,
    )
    report = json.loads(replay.stdout)
    if replay.returncode or report.get("version") != 1 or report.get("status") != "ok":
        raise Unsupported(f"transaction replay: {report.get('reason', replay.stderr)}")
    observed = report["observation"]
    with tempfile.TemporaryDirectory(prefix="solar-tv-transaction-") as folder:
        directory = Path(folder)
        if request["kind"] == "set_code":
            module = SET_CODE
            source = set_code_source(
                request, observed, solar, helper, timeout, directory
            )
        elif request["kind"] == "nested_reservoir":
            module = NESTED_RESERVOIR
            source = nested_reservoir_source(request, observed)
        else:
            module = RESERVOIR
            source = reservoir_source(request, observed)
        entry_proof.success(build_model(timeout, module), module)
        output = entry_proof.success(
            entry_proof.lean(directory, "Transaction", source, timeout),
            "transaction certificate",
        )
        entry_proof.check_axioms(output.splitlines(), source.count("#print axioms"))
    assumptions = {
        "set_code": [
            "the fixture supplies a decoded Prague or Osaka envelope and its outer signer",
            "checks signed authorizations, message outcome, gas settlement, and the selected account observations",
        ],
        "nested_reservoir": [
            "the fixture supplies forwarded execution gas, stipend, upfront state charge and the child exit",
            "compares all gas pools, signed ledgers, refund counters and frame depths against the pinned gas tracker",
        ],
    }.get(request["kind"])
    return {
        "version": 1,
        "status": "verified",
        "evidence": "kernel-and-rust-replay",
        "semantics": audit.semantics(),
        "fixture_sha256": hashlib.sha256(
            json.dumps(request, sort_keys=True).encode()
        ).hexdigest(),
        "target": request.get("fork", "amsterdam"),
        "observation": observed,
        **({"assumptions": assumptions} if assumptions else {}),
    }


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("fixture", type=Path)
    parser.add_argument("--timeout", type=int, default=60)
    parser.add_argument(
        "--solar",
        default=os.getenv("SOLAR")
        or str(Path(__file__).resolve().parents[2] / "target/debug/solar"),
    )
    args = parser.parse_args(argv)
    try:
        if args.fixture.stat().st_size > 128 * 1024:
            raise Unsupported("transaction fixture exceeds 128 KiB")
        result = check(json.loads(args.fixture.read_text()), args.solar, args.timeout)
    except (
        KeyError,
        TypeError,
        ValueError,
        OSError,
        Unsupported,
        subprocess.TimeoutExpired,
    ) as error:
        parser.exit(2, f"inconclusive: {error}\n")
    print(json.dumps(result))
    return 0
