"""EIP-7702 execution context, one-hop resolution and access-gas replay."""

import unittest

import test_world as world
from test_environment import constants, returned


def indicator(target):
    return "0xef0100" + target.to_bytes(20, "big").hex()


def call_result(kind="call", value=0, gas=300000, size=32):
    return (
        world.call(kind=kind, value=value, gas=gas, output=size)
        + f"\npop\npush {size}\npush 0\nreturn"
    )


def fixture(code, *, target=world.LEAF):
    return {
        "accounts": [
            {"address": world.ADDRESS, "balance": 100},
            {"address": world.MID, "code": indicator(target)},
            {"address": world.LEAF, "code": code},
        ]
    }


class DelegationTests(unittest.TestCase):
    pair = world.WorldTests.pair

    def test_delegation_activates_at_prague(self):
        initial = fixture(world.bytecode(constants([42])))
        self.pair(call_result(), constants([0]), fixture=initial, version="cancun")
        self.pair(call_result(), constants([42]), fixture=initial, version="prague")
        self.pair(
            call_result(), call_result(), fixture=initial, version="osaka", mode="exact"
        )

    def test_all_call_kinds_keep_their_context(self):
        code = world.bytecode(returned(["address", "caller", "callvalue"]))
        for kind in ("call", "callcode", "delegatecall", "staticcall"):
            with self.subTest(kind=kind):
                before = call_result(
                    kind, value=7 if kind in {"call", "callcode"} else 0, size=96
                )
                _, record = self.pair(
                    before, "stop", fixture=fixture(code), version="prague", status=1
                )
                data = bytes(record["witness"]["observations"][0][1])
                values = [
                    int.from_bytes(data[i : i + 32], "big") for i in range(0, 96, 32)
                ]
                self.assertEqual(
                    values,
                    [
                        world.MID if kind in {"call", "staticcall"} else world.ADDRESS,
                        131072 if kind == "delegatecall" else world.ADDRESS,
                        7 if kind in {"call", "callcode"} else 0,
                    ],
                )

    def test_storage_and_revert_use_authority_not_code_owner(self):
        initial = fixture(world.bytecode("push 7\npush 0\nsstore\nstop"))
        before = world.call() + "\npop\nstop"
        _, record = self.pair(
            before, "stop", fixture=initial, version="prague", status=1
        )
        accounts = record["witness"]["observations"][0][4]
        self.assertEqual(
            next(a for a in accounts if a[0] == str(world.MID))[4], [["0", "7"]]
        )
        self.assertEqual(next(a for a in accounts if a[0] == str(world.LEAF))[4], [])
        initial = fixture(
            world.bytecode("push 7\npush 0\nsstore\npush 0\npush 0\nrevert")
        )
        self.pair(before, "stop", fixture=initial, version="prague")

    def test_delegation_to_precompile_does_not_invoke_it(self):
        for target in (2, 4, 10, 256):
            with self.subTest(target=target):
                initial = {
                    "accounts": [{"address": world.MID, "code": indicator(target)}]
                }
                before = (
                    world.call(output=32) + "\npush 0\nmstore\npush 32\npush 0\nreturn"
                )
                self.pair(before, constants([1]), fixture=initial, version="osaka")

    def test_chains_and_self_delegation_are_not_followed(self):
        for target in (world.LEAF, world.MID):
            with self.subTest(target=target):
                initial = fixture(indicator(world.ADDRESS), target=target)
                before = world.call() + "\npush 0\nmstore\npush 32\npush 0\nreturn"
                self.pair(before, constants([0]), fixture=initial, version="prague")

    def test_only_exact_designators_resolve(self):
        for code in (
            indicator(world.LEAF)[:-2],
            indicator(world.LEAF) + "00",
            "0xef0200" + "00" * 20,
        ):
            with self.subTest(code=code):
                initial = {"accounts": [{"address": world.MID, "code": code}]}
                before = world.call() + "\npush 0\nmstore\npush 32\npush 0\nreturn"
                self.pair(before, constants([0]), fixture=initial, version="prague")

    def test_introspection_reads_designator_but_codesize_reads_target(self):
        code = world.bytecode(returned(["codesize"]))
        initial = fixture(code)
        self.pair(
            returned([f"push {world.MID}\nextcodesize"]),
            constants([23]),
            fixture=initial,
            version="prague",
        )
        self.pair(
            call_result(),
            constants([len(bytes.fromhex(code[2:]))]),
            fixture=initial,
            version="prague",
        )
        before = f"push 23\npush 0\npush 0\npush {world.MID}\nextcodecopy\npush 32\npush 0\nreturn"
        expected = int.from_bytes(
            bytes.fromhex(indicator(world.LEAF)[2:]) + bytes(9), "big"
        )
        self.pair(before, constants([expected]), fixture=initial, version="prague")
        before = returned([f"push {world.MID}\nextcodehash"])
        after = f"push {expected}\npush 0\nmstore\npush 23\npush 0\nkeccak256\npush 0\nmstore\npush 32\npush 0\nreturn"
        self.pair(before, after, fixture=initial, version="prague")

    def test_resolution_gas_warmth_forwarding_and_balance_rejection(self):
        initial = fixture(world.bytecode(returned(["gas"])))
        before = call_result(gas=2**256 - 1)
        self.pair(
            before,
            before,
            fixture={**initial, "gas": 20000},
            version="prague",
            mode="exact",
        )
        for warm in ([], [world.LEAF], [world.MID, world.LEAF]):
            with self.subTest(warm=warm):
                self.pair(
                    before,
                    before,
                    fixture={**initial, "warm_addresses": warm},
                    version="osaka",
                    mode="exact",
                )
        rejected = {
            "accounts": [
                {"address": world.MID, "code": indicator(world.LEAF)},
                {"address": world.LEAF, "code": "0x00"},
            ]
        }
        before = world.call(value=7) + "\npop\nstop"
        self.pair(before, before, fixture=rejected, version="prague", mode="exact")

    def test_target_warmth_survives_child_revert(self):
        initial = fixture(world.bytecode("push 0\npush 0\nrevert"))
        before = world.call() + f"\npop\npush {world.LEAF}\nextcodesize\npop\nstop"
        self.pair(before, before, fixture=initial, version="prague", mode="exact")

    def test_static_delegation_and_out_of_gas(self):
        initial = fixture(world.bytecode("push 7\npush 0\nsstore\nstop"))
        self.pair(
            call_result("staticcall"), constants([0]), fixture=initial, version="prague"
        )
        for budget in (10, 2600, 3000, 6000):
            with self.subTest(budget=budget):
                before = world.call() + "\npop\nstop"
                self.pair(
                    before,
                    before,
                    fixture={**initial, "gas": budget},
                    version="prague",
                    mode="exact",
                )

    def test_delegated_selfdestruct_affects_authority(self):
        initial = fixture(world.bytecode(f"push {world.ADDRESS}\nselfdestruct"))
        initial["accounts"][1]["balance"] = 7
        _, record = self.pair(
            world.call() + "\npop\nstop",
            "stop",
            fixture=initial,
            version="prague",
            status=1,
        )
        accounts = record["witness"]["observations"][0][4]
        self.assertEqual(next(a for a in accounts if a[0] == str(world.MID))[2], "0")
        self.assertEqual(
            next(a for a in accounts if a[0] == str(world.ADDRESS))[2], "107"
        )
