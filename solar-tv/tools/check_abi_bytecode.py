"""Bind the first lowered MIR certificate to a fresh compiler capture."""

import argparse
import hashlib
import json
from pathlib import Path

from validator import entry_proof, heap_entry, mir_state, native, type_prune, world
from validator.check import build_model, run
from validator.common import Program

ROOT = Path(__file__).resolve().parents[2]
SOURCE = Path("solar-tv/examples/solidity/abi-bytecode/Entry.sol")
MODULE = "SolarTV.Examples.AbiBytecode"
FORK = "cancun"
TIMEOUT = 120
SELECTOR = bytes.fromhex("8f51edf8")


def digest(data):
    return hashlib.sha256(data).hexdigest()


def capture(solar, directory):
    args = [
        solar,
        SOURCE,
        "-j1",
        "-Ogas",
        f"--evm-version={FORK}",
        "--emit=abi,bin,bin-runtime",
    ]
    plain = entry_proof.success(run(args, ROOT, TIMEOUT), "compiler baseline")
    captures = directory / "captures"
    captures.mkdir()
    validated = entry_proof.success(
        run([*args, f"-Zsolar-tv={captures}", "-Zsolar-tv-capture"], ROOT, TIMEOUT),
        "compiler capture",
    )
    artifact = json.loads(validated)
    entry_proof.require(
        json.loads(plain) == artifact, "capture changed compiler output"
    )
    (contract,) = artifact["contracts"].values()
    (mir,) = (
        manifest.with_name("after.mir")
        for manifest in captures.glob("captures/*/regions.jsonl")
        if any(
            row.get("layer") == "mir"
            and row.get("pass") == "lower-evm-shaped"
            and row.get("module") == "Entry"
            and row.get("target") == FORK
            and row.get("status") == "captured"
            for row in map(json.loads, manifest.read_text().splitlines())
        )
    )
    text = mir.read_text()
    before = mir.with_name("before.mir").read_text()
    entry_proof.require(
        "@phase lowered" in text.splitlines()
        and "@phase lowered" not in before.splitlines(),
        "capture must cross into lowered MIR",
    )
    entry_proof.success(
        run(
            [solar, mir, "-j1", "-Zmir-pipeline=none", f"--evm-version={FORK}"],
            ROOT,
            TIMEOUT,
        ),
        "first lowered MIR round-trip",
    )
    return artifact["version"], contract, text


def certify(replay, directory, text, image):
    catalog = native.catalog(replay, TIMEOUT)
    definitions = {op["name"]: op for op in catalog["evmir"]}
    model = type_prune.export(text, replay, TIMEOUT)
    functions = ",\n".join(
        mir_state.function_body(body, definitions) for body in model["functions"]
    )
    source = f"""import {MODULE}
open SolarTV EvmRules Execution EVM.State AbiBytecode

def capturedMir : MIR.State.Functions := [{functions}]
def capturedEntry : Nat := {model["dispatch_entry"]}
def emitted : List Byte := {list(image)}
def cancun : Config := {{fork := .cancun, opcodes := {world.opcode_rows(catalog, FORK)}, root := 65536}}

theorem mirBinding : capturedMir = firstLowered := rfl
theorem entryBinding : capturedEntry = entryId := rfl
theorem imageBinding : emitted = image := rfl

theorem schema : CancunPrices cancun := by
  refine ⟨rfl, ?_⟩
  have checked : (priceRows.all fun pair => decide (available cancun pair.1 = pair.2)) = true := by decide
  intro opcode row member
  exact of_decide_eq_true (List.all_eq_true.mp checked (opcode, row) member)

theorem backend (env : World.Environment) (journal : Journal) (fuel gas : Nat) :
    executeBytes emitted cancun env journal fuel gas =
      meterMir journal gas (needed env) (executeMir capturedMir capturedEntry env journal) := by
  rw [imageBinding, mirBinding, entryBinding]
  exact abi_to_bytecode cancun schema env journal fuel gas

#print axioms backend
"""
    entry_proof.success(build_model(TIMEOUT, MODULE), MODULE)
    output = entry_proof.success(
        entry_proof.lean(directory, "EmittedAbiEntry", source, TIMEOUT),
        "first lowered MIR to emitted bytecode",
    ).splitlines()
    entry_proof.check_axioms(output, 1)
    return output[0]


def replay_mir(text, fixture, executable):
    program = Program(
        ("world-entry", "mir"),
        [],
        "",
        "",
        replay_ir=text,
        selector="entry",
        evm_version=FORK,
        world=fixture,
    )
    result = native.replay(program, program, [], executable=executable, timeout=TIMEOUT)
    left, right = result["observations"]
    entry_proof.require(left == right, "independent MIR replay is unstable")
    return json.loads(left)


def replay_cases(solar, replay, directory, text, image):
    value = bytes(range(1, 33))
    disabled = bytes(32)
    enabled = (1).to_bytes(32, "big")
    malformed = (2).to_bytes(32, "big")
    cases = (
        ("wrong-selector", b"", 53, "revert", b""),
        ("short-calldata", SELECTOR + enabled + value[:-1], 89, "revert", b""),
        ("noncanonical-bool", SELECTOR + malformed + value, 89, "revert", b""),
        ("disabled", SELECTOR + disabled + value, 111, "revert", b""),
        ("enabled", SELECTOR + enabled + value, 125, "success", value),
        (
            "trailing-calldata",
            SELECTOR + enabled + bytes([255] * 32) + b"extra",
            125,
            "success",
            bytes([255] * 32),
        ),
    )
    fixture_path = directory / "world.json"
    results = {}
    for name, calldata, debit, status, data in cases:
        # Observe a preexisting storage cell across success, rollback and OOG.
        fixture_path.write_text(
            json.dumps(
                {
                    "version": 1,
                    "calldata": "0x" + calldata.hex(),
                    "accounts": [{"address": 65536, "storage": {"7": "42"}}],
                }
            )
        )
        mir = replay_mir(
            text, world.read(fixture_path, solar, None, kind="mir"), replay
        )
        fixture = world.read(fixture_path, solar, "exact")
        accounts = [
            [
                *account[:3],
                [] if account[0] == fixture["address"] else account[3],
                account[4],
            ]
            for account in mir["accounts"]
        ]
        entry_proof.require(
            [mir["outcome"], mir["data"]] == [status, list(data)], f"{name} MIR outcome"
        )
        observations = {}
        for gas in (debit - 1, debit, debit + 7):
            observed = heap_entry.observe(
                image, {**fixture, "gas": gas}, FORK, replay, TIMEOUT
            )
            expected = (
                ["out-of-gas", [], 0]
                if gas < debit
                else [status, list(data), gas - debit]
            )
            entry_proof.require(
                observed == [*expected, 0, accounts, mir["logs"]],
                f"{name} bytecode at gas {gas}",
            )
            observations[gas] = observed
        results[name] = {"mir": mir, "bytecode": observations}
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--solar", required=True, type=Path)
    parser.add_argument("--replay", required=True, type=Path)
    parser.add_argument(
        "--output", required=True, type=Path, help="new evidence directory"
    )
    args = parser.parse_args()
    solar = args.solar.resolve()
    replay = args.replay.resolve()
    directory = args.output.resolve()
    directory.mkdir(parents=True)
    version, contract, mir = capture(solar, directory)
    image = bytes.fromhex(contract["bin-runtime"])
    proof = certify(str(replay), directory, mir, image)
    replays = replay_cases(str(solar), str(replay), directory, mir, image)
    report = {
        "compiler": version,
        "compiler_sha256": digest(solar.read_bytes()),
        "replay_sha256": digest(replay.read_bytes()),
        "source_sha256": digest((ROOT / SOURCE).read_bytes()),
        "lowered_mir_sha256": digest(mir.encode()),
        "creation_sha256": digest(bytes.fromhex(contract["bin"])),
        "runtime_sha256": digest(image),
        "proof": proof,
        "captures_bytecode_neutral": True,
        "replay": replays,
    }
    (directory / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(
        f"Checked the emitted ABI theorem and {len(replays) * 3} Rust bytecode replays"
    )
    print(directory / "report.json")


if __name__ == "__main__":
    main()
