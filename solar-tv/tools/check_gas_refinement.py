"""Check Cancun gas refinement on the pinned permit and FlashLoan capture pairs."""

import argparse
import hashlib
import json
import subprocess
from functools import partial
from pathlib import Path

from validator import audit, entry_proof, heap_entry, world
from validator.common import Unsupported

FORK = "cancun"
FUEL = 512
TIMEOUT = 120
AMPLE = 500_000
SUFFIX = (
    "egraph,word-sequence,byte-run-loads,licm,indvar-simplify,loop-unroll,"
    "check-elim,cfg-simplify,merge-aborts,inline-physical,cfg-simplify,"
    "lower-evm-shaped,const-fold,branch-simplify,place-reads,dce,spell-width-tests,"
    "dce,lower-shifts,evm-inst-schedule"
)
CASES = (("solady-weth", "Permit", 148, 6226), ("morpho-blue", "FlashLoan", 362, 6279))
HEADER = """open SolarTV EvmRules Execution EVM.State MIR.TransferFromEntry
set_option autoImplicit false
set_option maxRecDepth 16384
set_option maxHeartbeats 16000000
set_option Elab.async false
"""


def digest(data):
    return hashlib.sha256(data).hexdigest()


def emit(source, directory, compiler):
    subprocess.run(
        [
            compiler,
            source,
            "-j1",
            "-Ogas",
            f"--evm-version={FORK}",
            f"-Zmir-pipeline={SUFFIX}",
            "-Zvalidate-ir=true",
            "--emit=bin,bin-runtime",
            "--out-dir",
            directory,
        ],
        check=True,
        timeout=TIMEOUT,
        capture_output=True,
        text=True,
    )
    (contract,) = json.loads((directory / "combined.json").read_text())[
        "contracts"
    ].values()
    return [
        bytes.fromhex(contract[key].removeprefix("0x"))
        for key in ("bin", "bin-runtime")
    ]


def calls(image, fixture, replay):
    """Propose the exact root schedule from independent physical PC probes."""
    sites = []
    pc = 0
    codes = {
        int(account["address"]): account["code"] for account in fixture["accounts"]
    }
    while pc < len(image):
        opcode = image[pc]
        following = pc + 1 + (opcode - 0x5F if 0x60 <= opcode <= 0x7F else 0)
        if opcode in (0xF1, 0xFA):
            try:
                cut = heap_entry.observe(image, fixture, FORK, replay, 30, pc=pc)
            except Unsupported as error:
                if not str(error).startswith(
                    "Rust replay: bytecode probe was not reached"
                ):
                    raise
                pc = following
                continue
            stack = list(map(int, cut["stack"]))
            requested, destination = stack[:2]
            value, offset, length = stack[2:5] if opcode == 0xF1 else (0, *stack[2:4])
            assert value == 0 and offset + length <= len(cut["memory"])
            kind = "call" if opcode == 0xF1 else "staticcall"
            rule = (
                kind,
                destination,
                value,
                cut["memory"][offset : offset + length],
                codes.get(destination, []),
            )
            sites.append((len(cut["pcs"]), pc, requested, rule))
        pc = following
    return [(pc, requested, rule) for _, pc, requested, rule in sorted(sites)]


def observation(result):
    status, data, gas, refund, accounts, logs = result
    stop = {
        "out-of-gas": "outOfGas",
        "success": "success",
        "revert": "reverted",
        "exception": "exception",
    }[status]
    account_rows = ",".join(
        f"⟨{address},{nonce},{balance},{code},"
        f"[{','.join(f'({key},{value})' for key, value in slots)}],[],false,false⟩"
        for address, nonce, balance, code, slots in accounts
    )
    log_rows = ",".join(
        f"⟨{address},[{','.join(topics)}],{payload}⟩"
        for address, topics, payload in logs
    )
    return f"⟨.{stop},{data},{gas},{refund},[{account_rows}],[{log_rows}]⟩"


def prove_image(
    directory, name, image, fixture, compiler, replay, policy, result, full
):
    config = world.configuration(compiler, FORK, fixture, 30, replay)
    env = world.environment_expression(fixture)
    accounts = world.accounts_expression(fixture)
    warm = ",".join(world.warm_addresses(fixture, FORK))
    rules = ",".join(
        f"⟨⟨.{kind},{address},{value},{data}⟩,{code}⟩"
        for kind, address, value, data, code in policy
    )
    source = (
        "import SolarTV.MIR.Transforms.EntryTrace\n"
        + HEADER
        + entry_proof.image_definitions(image)
        + config
        + f"""
def env : World.Environment := {env}
def accounts : List Account := ({accounts} : List Account).map
  (fun account => if account.address = env.address then {{account with code := code.bytes}} else account)
def checkpoint : Journal := ⟨accounts,accounts,[{warm}],[],[],0⟩
def policy : List CallRule := [{rules}]
def byteTailStart : ByteCut := ⟨⟨env,code.bytes,0,[],[],[],{fixture["gas"]}⟩,checkpoint,[],[],[]⟩
def byteTailWith (hash : List Byte → Word) := checkedByteExitStep tvWorldConfig code policy hash
"""
    )
    prove = partial(entry_proof.lean, directory, namespace=name)
    entry_proof.success(prove(name, source, TIMEOUT, module=True), name)
    memo, checked, segments = entry_proof.phase(
        prove, name, (("byteTail", "ByteCut", "ByteExit", FUEL),), TIMEOUT
    )
    finished = (
        ""
        if not full
        else """
theorem all_calls : byteTailCut.budgets.length = policy.length := by decide +kernel
#print axioms all_calls
"""
    )
    source = (
        entry_proof.import_modules(checked)
        + HEADER
        + f"""
def terminal := finishByte checkpoint byteTailCut.result
theorem execution : runByteExit tvWorldConfig {FUEL} byteTailStart = .ok byteTailCut := by
  apply checkedByteExit_sound tvWorldConfig code policy
  rw [← {memo}.hashChecked_eq]
  exact byteTailReached
theorem replay : observe tvWorldConfig .exact terminal = {observation(result)} := by decide +kernel
#print axioms execution
#print axioms replay
{finished}
"""
    )
    module = name + "Report"
    verdict = prove(module, source, TIMEOUT, module=True)
    (directory / f"{module}.log").write_text(verdict.stdout + verdict.stderr)
    entry_proof.check_axioms(
        entry_proof.success(verdict, module).splitlines(), 2 + full
    )
    return segments, module


def prove_pair(directory, name, modules):
    before, after = (module.removesuffix("Report") for module in modules)
    source = (
        entry_proof.import_modules(modules)
        + HEADER
        + f"""
theorem physical_refinement :
    equivalent {before}.tvWorldConfig {after}.tvWorldConfig .refine
      ((runByteExit {before}.tvWorldConfig {FUEL} {before}.byteTailStart).map
        (finishByte {before}.checkpoint ∘ ByteExit.result))
      ((runByteExit {after}.tvWorldConfig {FUEL} {after}.byteTailStart).map
        (finishByte {after}.checkpoint ∘ ByteExit.result)) := by
  rw [{before}.execution,{after}.execution]
  change equivalent {before}.tvWorldConfig {after}.tvWorldConfig .refine
    (.ok {before}.terminal) (.ok {after}.terminal)
  decide +kernel
#print axioms physical_refinement
"""
    )
    verdict = entry_proof.lean(directory, name, source, TIMEOUT)
    (directory / f"{name}.log").write_text(verdict.stdout + verdict.stderr)
    entry_proof.check_axioms(entry_proof.success(verdict, name).splitlines(), 1)
    return digest((directory / f"{name}.lean").read_bytes())


def check_case(args, case, name, index, edge):
    directory = (args.output / case).resolve()
    directory.mkdir(parents=True, exist_ok=True)
    capture = json.loads((args.audit / case / f"check-{index:03}.json").read_text())
    baseline = json.loads((args.audit / case / "baseline.json").read_text())[
        "contracts"
    ]
    (contract,) = [value for unit in baseline.values() for value in unit.values()]
    expected = [
        bytes.fromhex(contract["evm"][key]["object"])
        for key in ("bytecode", "deployedBytecode")
    ]
    emitted = []
    for side, entry in zip(("before", "after"), capture["inputs"], strict=True):
        path = Path(entry["path"])
        assert digest(path.read_bytes()) == entry["capture_sha256"]
        destination = directory / side
        destination.mkdir(exist_ok=True)
        emitted.append(emit(path, destination, args.compiler))
    assert emitted[1] == expected
    images = [runtime for _, runtime in emitted]
    fixture = world.read(args.worlds / case / "world.json", str(args.compiler), "exact")
    if case == "solady-weth":
        # Force the invalid-signature branch without evaluating elliptic-curve arithmetic.
        fixture = {
            **fixture,
            "calldata": fixture["calldata"][:-64]
            + [0] * 32
            + fixture["calldata"][-32:],
        }
    schedules = [
        calls(image, {**fixture, "gas": AMPLE}, args.replay) for image in images
    ]
    policies = [[rule for _, _, rule in schedule] for schedule in schedules]
    assert policies[0] == policies[1] and len(policies[0]) == (
        1 if case == "solady-weth" else 3
    )
    print(
        f"{name} Rust call PCs and requested gas: {[[(pc, gas) for pc, gas, _ in s] for s in schedules]}",
        flush=True,
    )
    rows = []
    for gas in (edge, AMPLE):
        state = {**fixture, "gas": gas}
        results = [
            heap_entry.observe(image, state, FORK, args.replay, 30) for image in images
        ]
        old, new = results
        assert old[0] == "out-of-gas" or (
            old[:2] == new[:2] and old[3:] == new[3:] and old[2] <= new[2]
        )
        modules = []
        segments = 0
        for side, image, result in zip(
            ("Before", "After"), images, results, strict=True
        ):
            print(
                f"{name} {gas} {side}: Rust {result[0]}, {result[2]} gas; checking Lean",
                flush=True,
            )
            count, module = prove_image(
                directory,
                f"{name}{gas}{side}",
                image,
                state,
                str(args.compiler),
                args.replay,
                policies[0],
                result,
                gas == AMPLE,
            )
            modules.append(module)
            segments += count
        proof = prove_pair(directory, f"{name}{gas}Refinement", modules)
        rows.append(
            {
                "gas": gas,
                "segments": segments,
                "rust_observations": results,
                "proof_sha256": proof,
            }
        )
    return {
        "capture": capture["inputs"],
        "runtime_sha256": list(map(digest, images)),
        "creation_sha256": [digest(creation) for creation, _ in emitted],
        "call_policy": policies[0],
        "ample_call_sites": schedules,
        "fixtures": rows,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for flag in ("audit", "worlds", "output", "compiler", "replay"):
        parser.add_argument(
            f"--{flag}", required=True, type=lambda path: Path(path).resolve()
        )
    parser.add_argument("--case", choices=[case for case, *_ in CASES])
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    semantics = audit.semantics()
    for case, name, index, edge in CASES:
        if args.case is not None and args.case != case:
            continue
        report = check_case(args, case, name, index, edge)
        assert audit.semantics() == semantics
        report.update(
            {
                "semantics": semantics,
                "compiler_sha256": digest(args.compiler.read_bytes()),
                "replay_sha256": digest(args.replay.read_bytes()),
            }
        )
        (args.output / case / "refinement.json").write_text(
            json.dumps(report, indent=2) + "\n"
        )


if __name__ == "__main__":
    main()
