"""Check the real ABI path to the internal Morpho helper."""

import json
from pathlib import Path

from . import caller_replay, dispatch_proof, entry_proof, mir_state, native, world
from .callers import require
from .check import PRELUDE, ROOT, build_model, run
from .common import Program

MODULE = "SolarTV.MIR.Transforms.TransferFromEntry"
FORK = "cancun"


def body_context(table, site):
    entry = table[site.name]
    helper = table["safeTransferFrom"]["id"]
    by_id = {body["id"]: body for body in table.values()}
    seen = {entry["id"]: entry}
    pending = [entry]
    returns = None
    while pending:
        body = pending.pop()
        for block in body["blocks"]:
            for row in (*block["instructions"], {"operation": block["terminator"]}):
                op = row["operation"]
                target = op.get("function")
                if target == helper:
                    require(
                        body is entry and returns is None and op["kind"] == "call",
                        "MIR helper cut must be the unique caller icall",
                    )
                    tail = block["instructions"][block["instructions"].index(row) + 1 :]
                    returns = [inst["operation"]["args"][1] for inst in tail]
                if target is not None and target not in seen:
                    seen[target] = by_id[target]
                    pending.append(by_id[target])
    require(returns is not None, f"cannot find MIR helper cut in {site.name}")
    return seen.values(), entry["id"], helper, returns


def default_fixture(name, solar, timeout):
    token = world.runtime(caller_replay.TOKEN_IR, "evmir", solar, FORK, timeout)
    oracle = b"\x7f" + caller_replay.word(10**36) + bytes.fromhex("5f5260205ff3")
    return {
        "version": 1,
        "address": caller_replay.MORPHO,
        "caller": caller_replay.CALLER,
        "gas": 300000,
        "calldata": "0x" + caller_replay.calldata(name).hex(),
        "accounts": [
            {
                "address": caller_replay.MORPHO,
                "nonce": 1,
                "storage": {
                    hex(key): str(value) for key, value in caller_replay.INITIAL.items()
                },
            },
            {
                "address": caller_replay.TOKEN,
                "nonce": 1,
                "code": "0x" + token.hex(),
            },
            {
                "address": caller_replay.ORACLE,
                "nonce": 1,
                "code": "0x" + oracle.hex(),
            },
            {"address": caller_replay.CALLER, "nonce": 1, "code": "0x00"},
        ],
    }


def fixtures(code, site, directory, solar, timeout, worlds=None):
    fixture = directory / f"{site.name}.world.json"
    if worlds is None:
        fixture.write_text(json.dumps(default_fixture(site.name, solar, timeout)))
    else:
        supplied = Path(worlds) / fixture.name
        require(supplied.stat().st_size <= 262144, "world fixture exceeds 256 KiB")
        fixture.write_bytes(supplied.read_bytes())
    state = world.read(fixture, solar, "exact")
    require(not state["symbols"], "ABI caller fixtures require concrete world values")
    require(
        bytes(state["calldata"][:4]) == site.selector.to_bytes(4, "big"),
        f"wrong ABI selector in {site.name} world",
    )
    root = next(
        account
        for account in state["accounts"]
        if account["address"] == state["address"]
    )
    root["code"] = list(code)
    return state


def replay_source(
    state,
    runtime,
    helper_pc,
    site,
    tables,
    solar,
    executable,
    timeout,
    *,
    observed=None,
    live=None,
):
    env = world.environment_expression(state)
    context = world.context_expression(state)
    config = world.configuration(solar, FORK, state, timeout, executable)
    catalog = {row["name"]: row for row in native.catalog(executable, timeout)["evmir"]}
    available = [
        row["opcode"] for row in catalog.values() if row["forks"][FORK]["available"]
    ]
    warm = ", ".join(world.warm_addresses(state, FORK))
    warm_slots = ", ".join(f"({address},{key})" for address, key in state["warm_slots"])
    seed = {
        **state,
        "accounts": [
            {**account, "code": []}
            if account["address"] == state["address"]
            else account
            for account in state["accounts"]
        ],
    }
    kernel = observed is not None
    header = (
        "import EntryImage\n" + entry_proof.HEADER
        if kernel
        else PRELUDE.replace("import SolarTV\n", f"import {MODULE}\n", 1)
        + "open MIR.TransferFromEntry\nset_option maxRecDepth 16384\n"
    )
    if not kernel:
        header += f'def image := imageBytes "{runtime.hex()}"\n'
    program = (
        header
        + config
        + f"""
def initialAccounts : List Execution.Account :=
  ({world.accounts_expression(seed)} : List Execution.Account).map fun a =>
    if a.address = {state["address"]} then {{a with code := image}} else a
def checkpoint : Execution.Journal := ⟨initialAccounts,initialAccounts,[{warm}],[{warm_slots}],[],0⟩
def env : World.Environment := {env}
def startFrame : EVM.State.Frame := ⟨env,image,0,[],[],[],{state["gas"]}⟩
def initial : ByteCut := ⟨startFrame,checkpoint,[],[],[]⟩
def mirCfg : MIR.State.Config :=
  {{ fork := .cancun, environment := env, context := {context}, available := {available},
    entry := false, externalConfig := some {{tvWorldConfig with unknownCode := some env.address}} }}
"""
    )
    if kernel:
        program += f"""
def physicalWith (hash : List Execution.Byte → Word) := chunkedByteStep {helper_pc} tvWorldConfig code hash
def mirGas := {{initial with samples := {observed["samples"]}, budgets := {observed["budgets"]}}}
""" + live_in_definitions(site, live)
    else:
        program += f"def priced := runBytes {helper_pc} 2048 tvWorldConfig initial\n"
    for label, table in zip(("before", "after"), tables, strict=True):
        functions, entry, helper, returned = body_context(table, site)
        model = ",\n".join(mir_state.function_body(body, catalog) for body in functions)
        program += f"\ndef {label}Functions : MIR.State.Functions := [{model}]\n"
        operands = mir_state.operands(returned)
        if kernel:
            program += f"""
def {label}Entry := {entry}
def {label}Helper := {helper}
def {label}Returned : List MIR.State.Operand := {operands}
def {label}Start := mirStart mirCfg {label}Functions {label}Entry selector caller checkpoint mirGas
def {label}With (hash : List Execution.Byte → Word) := mirTrace mirCfg {label}Functions {label}Helper {label}Returned mirGas hash
"""
        else:
            program += f"""
def {label} (cut : ByteCut) : Except String MirCut :=
  mirPrefix mirCfg {label}Functions {entry} {helper} {operands}
    {site.selector} .{site.name} checkpoint cut 1024
"""
    return (
        program
        if kernel
        else program + "#eval IO.println (describe priced before after)\n"
    )


def replay(state, pc, executable, timeout):
    program = Program(
        ("world-entry", "evmir"), [], "", "", world=state, evm_version=FORK, gas="exact"
    )
    raw = native.replay(
        program,
        program,
        [],
        executable=executable,
        timeout=timeout,
        probe_pc=pc,
        raw_runtime=pc is None,
    )
    left, right = raw["observations"]
    require(left == right, "unstable native helper-entry snapshot")
    return json.loads(left)


def live_values(site, snapshots):
    args, returned = snapshots["before"]
    require(len(args) == 4, "unexpected Morpho transferFrom argument count")
    values = list(map(int, args))
    require(
        all(0 <= value < 2**160 for value in values[:3]), "noncanonical helper address"
    )
    other = 0
    if site.live_words:
        require(len(returned) == 2, "unexpected Morpho caller return count")
        other = int(returned[0 if site.name == "liquidate" else 1])
    return values, other


def live_in_definitions(site, live):
    suffix = (
        f".pair {site.live_words[0]} {site.live_words[1]}"
        if site.live_words
        else ".stop"
    )
    args, other = live
    words = ",".join(map(str, args))
    return f"""
open MIR.TransferFromCallers
def caller : Caller := .{site.name}
def selector : Word := {site.selector}
def suffix : Suffix := ⟨{suffix},{site.gas}⟩
def returnPC : Word := {site.return_pc}
def inputs : MIR.TransferFromFrame.Inputs := ⟨{words}⟩
def other : Word := {other}
"""


def live_in_source(site, observations):
    stack = list(map(int, observations["bytecode"]["stack"]))
    source = (
        PRELUDE.replace("import SolarTV\n", f"import {MODULE}\n", 1)
        + "open MIR.TransferFromEntry\n"
        + live_in_definitions(site, live_values(site, observations))
        + f"def stack : List Word := {stack}\n"
    )
    for side in ("before", "after"):
        arguments, returned = (list(map(int, row)) for row in observations[side])
        source += f"""
theorem {side}_entry :
    related caller suffix returnPC inputs other stack {arguments} {returned} := by decide +kernel
theorem {side}_live :
    ∃ rest, stack = returnPC :: (argumentWords inputs ++ rest) ∧
      suffix.publication.words rest = .ok (liveOperands caller inputs.amount other) :=
  liveIns caller suffix returnPC inputs other stack {arguments} {returned} {side}_entry
"""
    return source


def check(
    runtime, helper_pc, sites, tables, directory, solar, executable, timeout, worlds
):
    directory.mkdir()
    built = build_model(timeout, entry_proof.MODULE)
    require(not built.returncode, built.stdout + built.stderr)
    built = build_model(timeout, "SolarTV.MIR.Transforms.TransferFromABI")
    require(not built.returncode, built.stdout + built.stderr)
    kernel = directory / "kernel"
    kernel.mkdir()
    entry_proof.success(
        entry_proof.lean(
            kernel,
            "EntryImage",
            entry_proof.image_source(runtime),
            timeout,
            module=True,
        ),
        "runtime image",
    )
    evidence = {}
    catalog = {
        row["opcode"]: row for row in native.catalog(executable, timeout)["evmir"]
    }
    for site in sites:
        state = fixtures(runtime, site, directory, solar, timeout, worlds)
        observed = replay(state, helper_pc, executable, timeout)
        routed = replay(state, site.abi_pc, executable, timeout)
        route = dispatch_proof.plan(runtime, site, observed["pcs"], catalog)
        require(
            routed["pc"] == route.pc
            and routed["pcs"] == [inst.pc for inst in route.instructions]
            and routed["stack"] == list(map(str, route.stack))
            and routed["gas"] == state["gas"] - route.gas
            and not (
                routed["samples"]
                or routed["budgets"]
                or routed["memory"]
                or routed["returndata"]
            ),
            f"{site.name} dispatcher disagrees with raw bytecode replay",
        )
        (directory / f"{site.name}-dispatch.json").write_text(json.dumps(routed))
        source = replay_source(
            state, runtime, helper_pc, site, tables, solar, executable, timeout
        )
        evaluation = (directory / f"{site.name}-replay.lean").resolve()
        evaluation.write_text(source)
        command = ["lake", "env", "lean", "-j1", "--memory=2048"]
        result = run([*command, evaluation], ROOT, timeout)
        require(
            result.returncode == 0 and not result.stdout.startswith("unsupported: "),
            f"{site.name} ABI entry replay failed\n" + result.stdout + result.stderr,
        )
        snapshots = json.loads(result.stdout)
        require(
            snapshots["bytecode"] == observed,
            f"{site.name} native helper entry disagrees with Lean",
        )
        certificate = (directory / f"{site.name}.lean").resolve()
        certificate.write_text(live_in_source(site, snapshots))
        proved = run([*command, certificate], ROOT, timeout)
        require(
            proved.returncode == 0,
            f"{site.name} live-in proof failed\n" + proved.stdout + proved.stderr,
        )
        (directory / f"{site.name}.json").write_text(json.dumps(snapshots))
        fixture = replay_source(
            state,
            runtime,
            helper_pc,
            site,
            tables,
            solar,
            executable,
            timeout,
            observed=observed,
            live=live_values(site, snapshots),
        )
        proof = entry_proof.certify(
            kernel, site, fixture, observed, helper_pc, route, len(runtime), timeout
        )
        terminal = replay(state, None, executable, timeout)
        exit_proof = entry_proof.certify_exit(
            kernel, proof["proof"], helper_pc, terminal, timeout
        )
        (directory / f"{site.name}-exit.json").write_text(json.dumps(terminal))
        evidence[site.name] = {
            **proof,
            "exit": {**exit_proof, "status": terminal[0], "gas": terminal[2]},
            "world_sha256": state["fixture_sha256"],
            "pc": helper_pc,
            "gas": observed["gas"],
            "steps": len(observed["pcs"]),
        }
    return evidence, entry_proof.certify_suite(kernel, evidence, timeout)
