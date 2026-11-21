"""Check emitted Morpho message boundaries and reentrant execution."""

import json
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import (
    builtin_lowering,
    call_budget,
    caller_entry,
    caller_replay,
    callers,
    entry_proof,
    native,
    owner_reentry,
    packing_boundary,
    recovery_boundary,
    world,
)
from validator.check import build_model


class MorphoMessageBoundaryTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        temporary = tempfile.TemporaryDirectory()
        cls.addClassCleanup(temporary.cleanup)
        cls.directory = Path(temporary.name)
        scheduled, _, disassembly, _, cls.runtime, cls.mir_pair = call_budget.capture(
            SOLAR, cls.directory, 45
        )
        cls.replay = native.default(SOLAR)
        cls.catalog = {
            row["name"]: row for row in native.catalog(cls.replay, 30)["evmir"]
        }
        cls.decoded = call_budget.decode_runtime(cls.runtime, cls.catalog)
        _, _, setup, _ = call_budget.extract(scheduled, disassembly)
        cls.helper = call_budget.extract_final(cls.runtime, setup, cls.catalog)[0]
        entry_proof.success(build_model(45, entry_proof.MODULE), entry_proof.MODULE)
        entry_proof.success(
            entry_proof.lean(
                cls.directory,
                "EntryImage",
                entry_proof.image_source(cls.runtime),
                45,
                module=True,
            ),
            "runtime image",
        )

    def fixture(self, name, value):
        file = self.directory / f"{name}.world.json"
        file.write_text(json.dumps(value))
        parsed = world.read(file, SOLAR, "exact")
        next(
            account
            for account in parsed["accounts"]
            if account["address"] == parsed["address"]
        )["code"] = list(self.runtime)
        return parsed

    def test_morpho_prehelper_call_caps_and_residual_gas(self):
        catalog = {row["opcode"]: row for row in self.catalog.values()}
        state = self.fixture(
            "prehelper", caller_entry.default_fixture("flashLoan", SOLAR, 30)
        )
        config = world.configuration(SOLAR, "cancun", state, 45, self.replay)
        evidence = caller_entry.certify_prehelper_calls(
            self.directory,
            self.directory,
            self.runtime,
            self.decoded,
            catalog,
            self.helper,
            SOLAR,
            self.replay,
            config,
            45,
        )
        self.assertTrue((self.directory / Path(evidence["proof"]).name).is_file())
        self.assertEqual(set(evidence["sites"]), set(callers.SELECTORS))
        self.assertEqual(
            [len(evidence["sites"][name]) for name in callers.SELECTORS],
            [1, 1, 1, 3, 2],
        )
        state = world.read(
            self.directory / "flashLoan-callback.world.json", SOLAR, "exact"
        )
        state["accounts"][0]["code"] = list(self.runtime)
        actor = next(
            account
            for account in state["accounts"]
            if account["address"] == state["caller"]
        )
        # Return the child's GAS observation in a word. The wrapper advances
        # past CALL before the next Solidity statement consumes the result.
        actor["code"] = list(bytes.fromhex("5a5f5260205ff3"))
        pc, _ = evidence["sites"]["flashLoan"][-1]
        before = caller_entry.replay(state, pc, self.replay, 30)
        after = caller_entry.replay(state, pc + 1, self.replay, 30)
        before_gas = before["gas"]
        self.assertGreater(int(before["stack"][0]), before_gas)
        self.assertEqual(before["memory"], after["memory"])
        available = before_gas - 100  # The caller's address is already warm.
        forwarded = available - available // 64
        self.assertEqual(
            int.from_bytes(bytes(after["returndata"]), "big"), forwarded - 2
        )
        self.assertEqual(after["gas"], available - 15)

    def test_authorization_prices_the_emitted_staticcall(self):
        site = recovery_boundary.find(self.decoded)
        packing = packing_boundary.plan(
            self.runtime, self.decoded, site.gas, self.catalog
        )
        call_data = bytes.fromhex("8069218f") + b"".join(
            map(
                caller_replay.word,
                (caller_replay.CALLER, 0x20000, 1, 0, 10**10, 0, 0, 0),
            )
        )
        state = self.fixture(
            "authorization",
            {
                "version": 1,
                "address": caller_replay.MORPHO,
                "caller": caller_replay.CALLER,
                "gas": 300000,
                "calldata": "0x" + call_data.hex(),
                "accounts": [{"address": caller_replay.MORPHO, "nonce": 1}],
            },
        )
        _, models, _ = builtin_lowering.load_pair(
            self.mir_pair, SOLAR, self.replay, "cancun", 45
        )
        config = world.configuration(SOLAR, "cancun", state, 45, self.replay)
        abi = caller_entry.certify_authorization(
            self.directory,
            self.runtime,
            self.decoded,
            {row["opcode"]: row for row in self.catalog.values()},
            list(map(builtin_lowering.functions, models)),
            state,
            config,
            self.replay,
            45,
        )
        self.assertTrue((self.directory / Path(abi["proof"]).name).is_file())
        reached = caller_entry.replay(
            {**state, "gas": abi["gas"]}, abi["body_pc"], self.replay, 30
        )
        self.assertEqual(reached["gas"], 0)
        for expired in (False, True):
            data = state["calldata"].copy()
            if expired:
                data[132:164] = caller_replay.word(0)
            selected = (
                abi["deadline_expired_pc"] if expired else abi["deadline_success_pc"]
            )
            checkpoint = caller_entry.replay(
                {**state, "calldata": data, "gas": abi["gas"] + abi["deadline_gas"]},
                selected,
                self.replay,
                30,
            )
            self.assertEqual(checkpoint["gas"], 0)
        self.assertEqual(
            caller_entry.replay(
                {**state, "gas": abi["gas"] - 1}, None, self.replay, 30
            )[0],
            "out-of-gas",
        )
        for offset, invalid in ((4, 2**160), (36, 2**160), (68, 2)):
            corrupt = state["calldata"].copy()
            corrupt[offset : offset + 32] = caller_replay.word(invalid)
            self.assertEqual(
                caller_entry.replay(
                    {**state, "calldata": corrupt}, None, self.replay, 30
                )[0],
                "revert",
            )

        expired = state["calldata"].copy()
        expired[132:164] = caller_replay.word(0)
        short = caller_entry.replay(
            {**state, "calldata": expired, "gas": abi["expired_gas"] - 1},
            None,
            self.replay,
            30,
        )
        self.assertEqual(short[:4], ["out-of-gas", [], 0, 0])
        message = b"signature expired"
        revert_data = (
            bytes.fromhex("08c379a0")
            + caller_replay.word(32)
            + caller_replay.word(len(message))
            + message.ljust(32, b"\0")
        )
        for surplus in (0, 37):
            stopped = caller_entry.replay(
                {**state, "calldata": expired, "gas": abi["expired_gas"] + surplus},
                None,
                self.replay,
                30,
            )
            self.assertEqual(stopped[:4], ["revert", list(revert_data), surplus, 0])
            self.assertEqual(stopped[5], [])

        nonce_pc = abi["nonce_store_pc"]
        self.assertEqual(nonce_pc, site.nonce)
        nonce_full = caller_entry.replay(state, nonce_pc, self.replay, 30)
        key = nonce_full["stack"][0]
        prefix = abi["gas"] + abi["deadline_gas"] + abi["nonce_fixed_gas"]
        for price, warm in ((2100, []), (100, [(state["address"], key)])):
            for old in (0, 17):
                account = {**state["accounts"][0], "storage": [[key, str(old)]]}
                ready = {
                    **state,
                    "accounts": [account],
                    "warm_slots": warm,
                    "gas": prefix + price,
                }
                reached = caller_entry.replay(ready, nonce_pc, self.replay, 30)
                self.assertEqual(reached["gas"], 0)
                self.assertEqual(reached["stack"][:3], [key, str(old + 1), str(old)])
                failed = caller_entry.replay(
                    {**ready, "gas": ready["gas"] - 1}, None, self.replay, 30
                )
                self.assertEqual(failed[:4], ["out-of-gas", [], 0, 0])
                self.assertEqual(failed[5], [])

        evidence = recovery_boundary.certify(
            self.directory, site, packing, state, SOLAR, self.replay, 45
        )
        self.assertTrue((self.directory / Path(evidence["proof"]).name).is_file())
        before_packing = caller_entry.replay(
            state, evidence["packing_pc"], self.replay, 30
        )
        after_packing = caller_entry.replay(state, packing.next_pc, self.replay, 30)
        self.assertEqual(
            before_packing["gas"] - after_packing["gas"], evidence["packing_gas"]
        )
        sample = caller_entry.replay(state, site.gas, self.replay, 30)
        call = caller_entry.replay(state, site.call, self.replay, 30)
        returned = caller_entry.replay(state, site.call + 1, self.replay, 30)
        self.assertEqual(call["gas"], sample["gas"] - 2)
        self.assertEqual(call["stack"], [str(call["gas"]), *sample["stack"]])
        self.assertEqual(call["memory"], sample["memory"])
        self.assertEqual(returned["stack"][0], "1")
        self.assertEqual(call["gas"] - returned["gas"], 3100)
        self.assertEqual(caller_entry.replay(state, None, self.replay, 30)[0], "revert")

        too_little = {**state, "gas": state["gas"] - sample["gas"] + 1}
        self.assertEqual(
            caller_entry.replay(too_little, site.gas, self.replay, 30)["gas"], 1
        )
        self.assertEqual(
            caller_entry.replay(too_little, None, self.replay, 30)[0], "out-of-gas"
        )
        unpaid_call = {**too_little, "gas": too_little["gas"] + 100}
        self.assertEqual(
            caller_entry.replay(unpaid_call, site.call, self.replay, 30)["gas"], 99
        )
        self.assertEqual(
            caller_entry.replay(unpaid_call, None, self.replay, 30)[0], "out-of-gas"
        )

        altered = list(self.runtime)
        altered[site.gas] = 0x5F
        broken = {**state, "accounts": [{**state["accounts"][0], "code": altered}]}
        rejected = caller_entry.replay(broken, site.call + 1, self.replay, 30)
        self.assertEqual(rejected["stack"][0], "0")

    def test_authorization_reentry_commits_and_rolls_back(self):
        site = recovery_boundary.find(self.decoded)
        packing = packing_boundary.plan(
            self.runtime, self.decoded, site.gas, self.catalog
        )
        _, models, _ = builtin_lowering.load_pair(
            self.mir_pair, SOLAR, self.replay, "cancun", 45
        )
        tables = list(map(builtin_lowering.functions, models))
        # A fixed signature for the pinned Morpho domain and this authorization.
        # The token submits it with a nested CALL during transferFrom.
        signatures = {
            True: 0x4F8C8CC2DF646C47881F49253C98A3A39F6F5A0C95C2ACD55B5204AA5CAF5828,
            False: 0x0B2A20151741914053235149B51D70C5BE8D497F5205C07BF73140CB593C7919,
        }
        auth_slot = "0x36a320cf0eede54290cc183360d09f3bf8d29f568fed549dd6326f34ca6c0690"
        for accepted, grant in (
            (True, True),
            (False, True),
            (True, False),
            (False, False),
        ):
            with self.subTest(accepted=accepted, grant=grant):
                calldata = bytes.fromhex("8069218f") + b"".join(
                    map(
                        caller_replay.word,
                        (
                            0x7E5F4552091A69125D5DFCB7B8C2659029395BDF,
                            0x20000,
                            int(grant),
                            0,
                            10**10,
                            28,
                            0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798,
                            signatures[grant],
                        ),
                    )
                )
                fixture = caller_entry.default_fixture("supplyCollateral", SOLAR, 30)
                if not grant:
                    fixture["accounts"][0]["storage"][auth_slot] = "1"
                # Copy the appended calldata, call Morpho, save its success
                # flag to token storage, and return the ERC-20 boolean.
                token = (
                    bytearray.fromhex("61010460005f395f5f6101045f5f62")
                    + caller_replay.MORPHO.to_bytes(3, "big")
                    + bytes.fromhex(f"5af15f5560{int(accepted):02x}5f5260205ff3")
                )
                token[4] = len(token)
                fixture["accounts"][1]["code"] = "0x" + (token + calldata).hex()
                state = self.fixture(f"authorization-{accepted}-{grant}", fixture)
                if accepted and grant:
                    evidence = recovery_boundary.certify(
                        self.directory,
                        site,
                        packing,
                        state,
                        SOLAR,
                        self.replay,
                        45,
                        tables=tables,
                    )
                    self.assertEqual(
                        evidence["store_pcs"], [site.nonce, site.authorization]
                    )
                observed = caller_entry.replay(state, None, self.replay, 45)
                self.assertEqual(observed[0], "success" if accepted else "revert")
                slots = {int(account[0]): dict(account[4]) for account in observed[4]}
                self.assertEqual(
                    slots[caller_replay.TOKEN].get("0"), "1" if accepted else None
                )
                # The success path commits the nonce and authorization. The
                # final transferFrom failure rolls them and all three logs back.
                self.assertEqual(observed[3], 4800 if accepted and not grant else 0)
                current = slots[caller_replay.MORPHO].get(str(int(auth_slot, 16)))
                self.assertEqual(current, "1" if accepted == grant else None)
                self.assertEqual(len(observed[5]), 3 if accepted else 0)
                result = entry_proof.lean(
                    self.directory,
                    "ReplayAuthorization",
                    recovery_boundary.replay_source(state),
                    45,
                )
                self.assertEqual(
                    result.returncode, 0, result.stdout[:1000] + result.stderr
                )
                checked = json.loads(result.stdout)
                self.assertEqual(checked[:2], [observed, observed])
                if accepted:
                    self.assertEqual(checked[2][0], "success")
                    self.assertNotEqual(checked[2], observed)
                    self.assertEqual(checked[3][0], "success")
                    corrupted = {
                        int(account[0]): dict(account[4]) for account in checked[3][4]
                    }
                    self.assertNotEqual(
                        corrupted[caller_replay.MORPHO].get(str(int(auth_slot, 16))),
                        current,
                    )
                if accepted and grant:
                    changed_domain = list(self.runtime)
                    changed_domain[evidence["packing_pc"] + 32] ^= 1
                    root = next(
                        account
                        for account in state["accounts"]
                        if account["address"] == state["address"]
                    )
                    root["code"] = changed_domain
                    invalid_signature = caller_entry.replay(
                        state, None, self.replay, 45
                    )
                    self.assertNotEqual(invalid_signature, observed)
                    self.assertEqual(checked[4], invalid_signature)
                    altered = list(self.runtime)
                    altered[site.gas] = 0x5F
                    root["code"] = altered
                    broken = caller_entry.replay(state, None, self.replay, 45)
                    slots = {int(account[0]): dict(account[4]) for account in broken[4]}
                    self.assertEqual(broken[0], "success")
                    self.assertEqual(slots[caller_replay.TOKEN], {})
                    self.assertEqual(len(broken[5]), 1)

    def test_reentrant_owner_matches_native_execution(self):
        rows = owner_reentry.steps(self.runtime, self.decoded, self.catalog)
        _, models, _ = builtin_lowering.load_pair(
            self.mir_pair, SOLAR, self.replay, "cancun", 45
        )
        tables = list(map(builtin_lowering.functions, models))
        fixture = caller_entry.default_fixture("supplyCollateral", SOLAR, 30)
        fixture["accounts"][0]["storage"]["0"] = str((1 << 240) + 0x77)
        fixture["gas"] = 150000
        # ERC-20 transferFrom reenters Morpho with a read-only owner() call,
        # records the ABI word, then returns the chosen ERC-20 success flag.
        call_owner = (
            bytes.fromhex("63")
            + owner_reentry.SELECTOR.to_bytes(4, "big")
            + bytes.fromhex("60e01b5f5260205f60045f62")
            + caller_replay.MORPHO.to_bytes(3, "big")
            + bytes.fromhex("5afa505f515f55")
        )
        paid_gas = []
        for accepted, warm in ((True, False), (True, True), (False, False)):
            with self.subTest(accepted=accepted, warm=warm):
                token = (
                    call_owner
                    + bytes.fromhex("60")
                    + bytes([int(accepted)])
                    + bytes.fromhex("5f5260205ff3")
                )
                fixture["accounts"][1]["code"] = "0x" + token.hex()
                fixture["warm_slots"] = [[caller_replay.MORPHO, 0]] if warm else []
                state = self.fixture(f"owner-{accepted}-{warm}", fixture)
                config = world.configuration(SOLAR, "cancun", state, 45, self.replay)
                if accepted and not warm:
                    evidence = owner_reentry.certify(
                        self.directory, rows, config, tables, self.catalog, 45
                    )
                    self.assertTrue(
                        (self.directory / Path(evidence["proof"]).name).is_file()
                    )
                replayed = caller_entry.replay(state, None, self.replay, 45)
                self.assertEqual(replayed[0], "success" if accepted else "revert")
                accounts = {int(row[0]): dict(row[4]) for row in replayed[4]}
                self.assertEqual(
                    accounts[caller_replay.TOKEN].get("0"), "119" if accepted else None
                )
                source = owner_reentry.replay_source(state)
                result = entry_proof.lean(self.directory, "ReplayOwner", source, 45)
                matches = json.loads(
                    entry_proof.success(result, "reentrant owner replay")
                )
                self.assertEqual(matches[:2], [replayed, replayed])
                if accepted:
                    paid_gas.append(replayed[2])
                    self.assertNotEqual(matches[2], replayed)
        self.assertEqual(paid_gas[1] - paid_gas[0], 2000)
