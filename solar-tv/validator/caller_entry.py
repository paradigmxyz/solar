"""Check the real ABI path to the internal Morpho helper."""

import json
from pathlib import Path

from . import (
    authorization_entry,
    caller_context,
    caller_gas,
    caller_replay,
    collateral_mir,
    collateral_mir_tail,
    collateral_proof,
    collateral_tail,
    decoder_proof,
    dispatch_proof,
    entry_proof,
    flashloan_rejection,
    market_proof,
    market_rejection,
    mir_state,
    native,
    owner_reentry,
    packing_boundary,
    prehelper_calls,
    recovery_boundary,
    world,
)
from .callers import SELECTORS, require
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


def certify_authorization(
    kernel,
    runtime,
    decoded,
    catalog,
    tables,
    state,
    config,
    executable,
    timeout,
    *,
    nonce_pc,
    signing_pc,
):
    layout = authorization_entry.plan(runtime, decoded, catalog)
    calldata = list(layout.selector.to_bytes(4, "big")) + [0] * 256
    probe = {**state, "calldata": calldata, "gas": 300000}
    reached = replay(probe, layout.pc, executable, timeout)
    route = dispatch_proof.plan(runtime, layout, reached["pcs"], catalog)
    stack = [
        str(
            word
            if isinstance(word, int)
            else decoder_proof.read_word(calldata, word[1])
        )
        for word in layout.stack
    ]
    memory = b"".join(
        caller_replay.word(
            word
            if isinstance(word, int)
            else decoder_proof.read_word(calldata, word[1])
        )
        for word in layout.memory
    )
    require(
        reached["pc"] == layout.pc
        and reached["gas"] == probe["gas"] - route.gas - layout.gas
        and reached["stack"] == stack
        and reached["memory"] == list(memory)
        and reached["pcs"]
        == [inst.pc for inst in route.instructions]
        + [step.inst.pc for step in layout.instructions],
        "authorization ABI cut disagrees with raw runtime",
    )
    return authorization_entry.certify(
        kernel,
        runtime,
        decoded,
        layout,
        route,
        config,
        tables,
        catalog,
        nonce_pc,
        signing_pc,
        timeout,
    )


def certify_prehelper_calls(
    directory,
    kernel,
    runtime,
    decoded,
    catalog,
    helper,
    solar,
    executable,
    config,
    timeout,
):
    # Cover the callback routes even when the supplied fixtures take the empty branch.
    cuts = {}
    probes, snapshots = [], []
    for name in SELECTORS:
        fixture = default_fixture(name, solar, timeout)
        data = caller_replay.calldata(name)
        fixture["calldata"] = (
            "0x" + (data[:-32] + caller_replay.word(1) + b"\xab" + bytes(31)).hex()
        )
        file = directory / f"{name}-callback.world.json"
        file.write_text(json.dumps(fixture))
        probe = world.read(file, solar, "exact")
        probe["accounts"][0]["code"] = list(runtime)
        reached = replay(probe, helper, executable, timeout)
        probes.append(probe)
        snapshots.append(reached)
        cuts[name] = [
            (pc, decoded[pc][0])
            for pc in reached["pcs"]
            if decoded[pc][0] in {"call", "staticcall"}
        ]
        require(cuts[name], f"missing {name} pre-helper message")
        for pc, opcode in cuts[name]:
            if opcode == "call":
                stack = replay(probe, pc, executable, timeout)["stack"]
                require(stack[2] == "0", "pre-helper CALL sends value")
    evidence = prehelper_calls.certify(kernel, cuts, config, catalog, timeout)
    meter = caller_gas.certify(kernel, helper, config, timeout)
    reports = caller_gas.evaluate(kernel, probes, timeout)
    for report, probe, snapshot in zip(reports, probes, snapshots, strict=True):
        caller_gas.check_reached(report, probe, snapshot)
    evidence["abi_meter"] = meter
    return evidence


def check(
    runtime, helper_pc, sites, tables, directory, solar, executable, timeout, worlds
):
    from .call_budget import decode_runtime

    directory.mkdir()
    built = build_model(timeout, entry_proof.MODULE)
    require(not built.returncode, built.stdout + built.stderr)
    built = build_model(timeout, "SolarTV.MIR.Transforms.TransferFromCollateral")
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
    names = {row["name"]: row for row in catalog.values()}
    context = caller_context.certify(kernel, tables, sites, names, timeout)
    decoded = decode_runtime(runtime, names)
    boundaries = None
    meter_inputs = []
    for site in sites:
        state = fixtures(runtime, site, directory, solar, timeout, worlds)
        if boundaries is None:
            recovery = recovery_boundary.find(decoded)
            packing = packing_boundary.plan(runtime, decoded, recovery.gas, names)
            owner = owner_reentry.steps(runtime, decoded, names)
            config = world.configuration(solar, FORK, state, timeout, executable)
            authorization = certify_authorization(
                kernel,
                runtime,
                decoded,
                catalog,
                tables,
                state,
                config,
                executable,
                timeout,
                nonce_pc=recovery.nonce,
                signing_pc=packing.steps[0].pc,
            )
            boundaries = {
                "prehelper_calls": certify_prehelper_calls(
                    directory,
                    kernel,
                    runtime,
                    decoded,
                    catalog,
                    helper_pc,
                    solar,
                    executable,
                    config,
                    timeout,
                ),
                "authorization_entry": authorization,
                "recovery_boundary": recovery_boundary.certify(
                    kernel,
                    recovery,
                    packing,
                    state,
                    solar,
                    executable,
                    timeout,
                    tables=tables,
                    entry=authorization["signing_record"]["proof"],
                ),
                "reentrant_owner": owner_reentry.certify(
                    kernel, owner, config, tables, names, timeout
                ),
            }
        observed = replay(state, helper_pc, executable, timeout)
        meter_inputs.append((site.name, state, observed))
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
        decoder = decoder_proof.plan(runtime, site, observed["pcs"], catalog)
        market = (
            market_proof.plan(
                runtime,
                decoder,
                observed["pcs"],
                catalog,
                market_proof.mir_markets(tables, site.name, catalog),
            )
            if site.name != "flashLoan"
            else None
        )
        concrete = decoder.accepts(state["calldata"])
        collateral = None
        if site.name == "supplyCollateral":
            write = collateral_proof.plan(runtime, observed["pcs"], catalog, market)
            collateral = (
                write,
                collateral_mir.plan(tables, catalog, market),
                collateral_tail.plan(runtime, catalog, write, helper_pc),
                collateral_mir_tail.plan(tables, catalog),
            )
        market_snapshot = None
        if concrete:
            reached = replay(state, decoder.pc, executable, timeout)
            stack, memory = decoder.replay_state(state["calldata"])
            require(
                reached["pc"] == decoder.pc
                and reached["pcs"]
                == routed["pcs"] + [step.inst.pc for step in decoder.instructions]
                and reached["stack"] == stack
                and reached["memory"] == memory
                and reached["gas"] == routed["gas"] - decoder.gas
                and not (
                    reached["samples"] or reached["budgets"] or reached["returndata"]
                ),
                f"{site.name} ABI decoder disagrees with raw bytecode replay",
            )
            (directory / f"{site.name}-decoder.json").write_text(json.dumps(reached))
            if market is not None:
                before_read = replay(state, market.load_pc, executable, timeout)
                read = replay(state, market.pc, executable, timeout)
                require(
                    read["pcs"]
                    == reached["pcs"] + [step.inst.pc for step in market.instructions],
                    f"{site.name} first market read disagrees with raw bytecode replay",
                )
                market_snapshot = before_read, read
                (directory / f"{site.name}-market.json").write_text(json.dumps(read))
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
            kernel,
            site,
            fixture,
            observed,
            helper_pc,
            route,
            decoder,
            concrete,
            market,
            market_snapshot,
            collateral,
            len(runtime),
            timeout,
        )
        if market is not None:
            read = proof["market_read"]
            read["guard"]["failure"] = market_rejection.certify(
                kernel,
                read["proof"],
                runtime,
                decoded,
                catalog,
                market,
                decoder,
                route,
                timeout,
            )
        elif site.name == "flashLoan":
            proof["zero_assets"] = flashloan_rejection.certify(
                kernel,
                proof["decoder"]["proof"],
                runtime,
                decoded,
                catalog,
                decoder,
                route,
                timeout,
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
    reports = caller_gas.evaluate(
        kernel, [state for _, state, _ in meter_inputs], timeout
    )
    for (name, state, observed), report in zip(meter_inputs, reports, strict=True):
        caller_gas.check_reached(report, state, observed)
        (directory / f"{name}-abi-gas.json").write_text(json.dumps(report))
    return (
        evidence,
        entry_proof.certify_suite(kernel, evidence, timeout),
        context,
        boundaries,
    )
