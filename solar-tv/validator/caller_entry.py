"""Check the real ABI path to the internal Morpho helper."""

import json

from . import caller_replay, mir_state, native, world
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
                elif target is not None and target not in seen:
                    seen[target] = by_id[target]
                    pending.append(by_id[target])
    require(returns is not None, f"cannot find MIR helper cut in {site.name}")
    return seen.values(), entry["id"], helper, returns


def fixtures(code, site, directory, solar, timeout):
    token = world.runtime(caller_replay.TOKEN_IR, "evmir", solar, FORK, timeout)
    oracle = b"\x7f" + caller_replay.word(10**36) + bytes.fromhex("5f5260205ff3")
    fixture = directory / f"{site.name}.world.json"
    fixture.write_text(
        json.dumps(
            {
                "version": 1,
                "address": caller_replay.MORPHO,
                "caller": caller_replay.CALLER,
                "gas": 300000,
                "calldata": "0x" + caller_replay.calldata(site.name).hex(),
                "accounts": [
                    {
                        "address": caller_replay.MORPHO,
                        "nonce": 1,
                        "storage": {
                            hex(key): str(value)
                            for key, value in caller_replay.INITIAL.items()
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
        )
    )
    state = world.read(fixture, solar, "exact")
    root = next(
        account
        for account in state["accounts"]
        if account["address"] == state["address"]
    )
    root["code"] = list(code)
    return state


def replay_source(state, runtime, helper_pc, site, tables, solar, executable, timeout):
    env = world.environment_expression(state)
    context = world.context_expression(state)
    config = world.configuration(solar, FORK, state, timeout, executable)
    catalog = {row["name"]: row for row in native.catalog(executable, timeout)["evmir"]}
    available = [
        row["opcode"] for row in catalog.values() if row["forks"][FORK]["available"]
    ]
    warm = ", ".join(world.warm_addresses(state, FORK))
    seed = {
        **state,
        "accounts": [
            {**account, "code": []}
            if account["address"] == state["address"]
            else account
            for account in state["accounts"]
        ],
    }
    program = (
        PRELUDE.replace("import SolarTV\n", f"import {MODULE}\n", 1)
        + "open MIR.TransferFromEntry\nset_option maxRecDepth 16384\n"
        + config
        + f"""def image := imageBytes "{runtime.hex()}"
def initialAccounts : List Execution.Account :=
  ({world.accounts_expression(seed)} : List Execution.Account).map fun a =>
    if a.address = {state["address"]} then {{a with code := image}} else a
def checkpoint : Execution.Journal := ⟨initialAccounts,initialAccounts,[{warm}],[],[],0⟩
def env : World.Environment := {env}
def startFrame : EVM.State.Frame := ⟨env,image,0,[],[],[],{state["gas"]}⟩
def initial : ByteCut := ⟨startFrame,checkpoint,[],[],[]⟩
def priced := runBytes {helper_pc} 2048 tvWorldConfig initial
def mirCfg : MIR.State.Config :=
  {{ fork := .cancun, environment := env, context := {context}, available := {available},
    entry := false, externalConfig := some tvWorldConfig }}
"""
    )
    for label, table in zip(("before", "after"), tables, strict=True):
        functions, entry, helper, returned = body_context(table, site)
        model = ",\n".join(mir_state.function_body(body, catalog) for body in functions)
        program += f"""
def {label}Functions : MIR.State.Functions := [{model}]
def {label} (cut : ByteCut) : Except String MirCut :=
  mirPrefix mirCfg {label}Functions {entry} {helper} {mir_state.operands(returned)}
    {site.selector} .{site.name} checkpoint cut 1024
"""
    return program + "#eval IO.println (describe priced before after)\n"


def replay(state, pc, executable, timeout):
    program = Program(
        ("world-entry", "evmir"), [], "", "", world=state, evm_version=FORK, gas="exact"
    )
    raw = native.replay(
        program, program, [], executable=executable, timeout=timeout, probe_pc=pc
    )
    left, right = raw["observations"]
    require(left == right, "unstable native helper-entry snapshot")
    return json.loads(left)


def live_in_source(site, observations):
    suffix = (
        f".pair {site.live_words[0]} {site.live_words[1]}"
        if site.live_words
        else ".stop"
    )
    other = {"supply": 7_000_000, "repay": 7_000_000, "liquidate": 8}.get(site.name, 0)
    stack = list(map(int, observations["bytecode"]["stack"]))
    source = (
        PRELUDE.replace("import SolarTV\n", f"import {MODULE}\n", 1)
        + f"""
open MIR.TransferFromEntry MIR.TransferFromCallers
def caller : Caller := .{site.name}
def suffix : Suffix := ⟨{suffix},{site.gas}⟩
def returnPC : Word := {site.return_pc}
def args : MIR.TransferFromFrame.Inputs := ⟨{caller_replay.TOKEN},{caller_replay.CALLER},{caller_replay.MORPHO},7⟩
def other : Word := {other}
def stack : List Word := {stack}
"""
    )
    for side in ("before", "after"):
        arguments, returned = (list(map(int, row)) for row in observations[side])
        source += f"""
theorem {side}_entry :
    related caller suffix returnPC args other stack {arguments} {returned} := by decide +kernel
theorem {side}_live :
    ∃ rest, stack = returnPC :: (argumentWords args ++ rest) ∧
      suffix.publication.words rest = .ok (liveOperands caller args.amount other) :=
  liveIns caller suffix returnPC args other stack {arguments} {returned} {side}_entry
"""
    return source


def check(runtime, helper_pc, sites, tables, directory, solar, executable, timeout):
    directory.mkdir()
    built = build_model(timeout, MODULE)
    require(not built.returncode, built.stdout + built.stderr)
    evidence = {}
    for site in sites:
        state = fixtures(runtime, site, directory, solar, timeout)
        observed = replay(state, helper_pc, executable, timeout)
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
        evidence[site.name] = {
            "pc": helper_pc,
            "gas": observed["gas"],
            "steps": len(observed["pcs"]),
        }
    return evidence
