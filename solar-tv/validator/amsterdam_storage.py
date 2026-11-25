"""Check actual Amsterdam storage bytecode with separate execution and state pools."""

import hashlib
import json
import tempfile
from pathlib import Path

from . import audit, entry_proof, native, world
from .common import Program, Unsupported
from .transaction import natural

MODEL = "SolarTV.EVMIR.State.Transactions.Storage"


def expected_observation(observation):
    stop, data, remaining, refund, accounts, logs, _ = observation
    stop = {
        "success": ".success",
        "revert": ".reverted",
        "out-of-gas": ".outOfGas",
        "exception": ".exception",
    }[stop]
    state = []
    for address, nonce, balance, code, slots in accounts:
        storage = ",".join(f"({natural(key)},{natural(word)})" for key, word in slots)
        state.append(
            f"⟨{natural(address)},{natural(nonce)},{natural(balance)},"
            f"{code},[{storage}],[],false,false⟩"
        )
    events = [
        f"⟨{natural(address)},{list(map(natural, topics))},{payload}⟩"
        for address, topics, payload in logs
    ]
    return (
        f"⟨{stop},{data},{remaining},{refund},[{','.join(state)}],[{','.join(events)}]⟩"
    )


def source(image, fixture, reservoir, observation, configuration):
    world_state = world.accounts_expression(fixture, image)
    warm_addresses = ",".join(world.warm_addresses(fixture, "amsterdam"))
    warm_slots = ",".join(f"({owner},{key})" for owner, key in fixture["warm_slots"])
    environment = world.environment_expression(fixture)
    gas = fixture["gas"]
    expected = expected_observation(observation)
    meter = ",".join(str(value) for value in observation[6])
    return f"""import {MODEL}
import SolarTV.EVMIR.State.Observe
open SolarTV EvmRules Execution
set_option maxRecDepth 16384
set_option maxHeartbeats 6000000
{configuration}
def accounts : List Execution.Account := {world_state}
def initial : Journal := ⟨accounts,accounts,[{warm_addresses}],[{warm_slots}],[],0⟩
def frame : EVM.State.Frame := ⟨{environment},{image},0,[],[],[],{gas}⟩
def execution := EVM.State.Amsterdam.execute tvWorldConfig {fixture["fuel"]}
  initial frame ⟨{gas},{reservoir},0,0⟩
theorem matches_replay :
    execution.toOption.map (fun result => (EVM.State.observe tvWorldConfig .exact result.result,result.meter)) =
      some ({expected},⟨{meter}⟩) := by decide +kernel
#print axioms matches_replay
"""


def check(request, solar, timeout):
    from .check import build_model

    if request.keys() != {"kind", "runtime", "reservoir", "world"}:
        raise Unsupported("invalid Amsterdam storage fixture fields")
    image = world._bytes(request["runtime"])
    reservoir = natural(request["reservoir"])
    replay = native.default(solar)
    with tempfile.TemporaryDirectory(prefix="solar-tv-amsterdam-storage-") as folder:
        directory = Path(folder)
        fixture_path = directory / "world.json"
        fixture_path.write_text(json.dumps(request["world"]))
        fixture = world.read(fixture_path, solar, "exact")
        if reservoir + fixture["gas"] > 30_000_000 or fixture["symbols"]:
            raise Unsupported(
                "Amsterdam storage replay requires a concrete world and at most 30000000 total gas"
            )
        program = Program(
            ("world-entry", "evmir"),
            [],
            "",
            "",
            world=world.with_runtime(fixture, image),
            evm_version="amsterdam",
        )
        data = native.replay(
            program,
            program,
            [],
            executable=replay,
            timeout=timeout,
            raw_runtime=True,
            amsterdam_reservoir=reservoir,
        )
        before, after = map(json.loads, data["observations"])
        if before != after or len(before) != 7 or len(before[6]) != 4:
            raise Unsupported("incomplete Amsterdam storage replay")
        configuration = world.configuration(
            solar, "amsterdam", fixture, timeout, replay
        )
        proof = source(image, fixture, reservoir, before, configuration)
        entry_proof.success(build_model(timeout, MODEL), MODEL)
        output = entry_proof.success(
            entry_proof.lean(directory, "AmsterdamStorage", proof, timeout),
            "Amsterdam storage replay",
        )
        entry_proof.check_axioms(output.splitlines(), 1)
    return {
        "version": 1,
        "status": "verified",
        "evidence": "kernel-and-rust-replay",
        "semantics": audit.semantics(),
        "fixture_sha256": hashlib.sha256(
            json.dumps(request, sort_keys=True).encode()
        ).hexdigest(),
        "image_sha256": hashlib.sha256(bytes(image)).hexdigest(),
        "target": "amsterdam",
        "observation": before,
        "assumptions": [
            "concrete raw bytecode and initial world, with supplied execution and reservoir balances",
            "settled execution gas, reservoir, signed state gas, spill, refund and persistent effects",
        ],
    }
