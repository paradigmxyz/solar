"""Explicit block context, fork activation and bytecode introspection replay."""

import unittest

import test_world as world

MASK = 2**256 - 1


def returned(expressions):
    body = []
    for i, expression in enumerate(expressions):
        body.extend([expression, f"push {32 * i}", "mstore"])
    return "\n".join([*body, f"push {32 * len(expressions)}", "push 0", "return"])


def constants(values):
    return returned([f"push {value}" for value in values])


class EnvironmentTests(unittest.TestCase):
    pair = world.WorldTests.pair

    def test_nondefault_block_and_transaction_context(self):
        fixture = {
            "origin": 999,
            "gas_price": 17,
            "chain_id": 8453,
            "block": {
                "number": 300,
                "timestamp": 123456,
                "coinbase": world.LEAF,
                "gas_limit": 12345678,
                "base_fee": 123,
                "blob_base_fee": 456,
            },
        }
        cases = [
            (["origin", "gasprice", "chainid"], [999, 17, 8453]),
            (["number", "timestamp", "coinbase"], [300, 123456, world.LEAF]),
            (["gaslimit", "basefee", "blobbasefee"], [12345678, 123, 456]),
        ]
        for expressions, values in cases:
            with self.subTest(expressions=expressions):
                source = returned(expressions)
                self.pair(source, constants(values), fixture=fixture)
                self.pair(source, source, fixture=fixture, mode="exact")

    def test_paris_selects_randomness_instead_of_difficulty(self):
        fixture = {"block": {"difficulty": 123, "prevrandao": MASK}}
        for fork, value in [
            ("istanbul", 123),
            ("london", 123),
            ("paris", MASK),
            ("osaka", MASK),
        ]:
            with self.subTest(fork=fork):
                self.pair(
                    returned(["prevrandao"]),
                    constants([value]),
                    fixture=fixture,
                    version=fork,
                )

    def test_blockhash_exact_window_and_absent_history(self):
        fixture = {
            "block": {
                "number": 300,
                "hashes": {"43": 43, "44": 44, "299": MASK, "300": 300, "301": 301},
            }
        }
        source = returned(
            [f"push {n}\nblockhash" for n in [43, 44, 298, 299, 300, 301, MASK]]
        )
        self.pair(source, constants([0, 44, 0, MASK, 0, 0, 0]), fixture=fixture)
        self.pair(
            returned(["push 0\nblockhash"]),
            constants([0]),
            fixture={"block": {"number": 0, "hashes": {"0": 9}}},
        )

    def test_blob_indices_are_not_truncated(self):
        fixture = {"blob_hashes": [2**248 + 42, 2**248 + 7]}
        source = returned([f"push {n}\nblobhash" for n in [0, 1, 2, MASK]])
        self.pair(source, constants([2**248 + 42, 2**248 + 7, 0, 0]), fixture=fixture)
        self.pair(source, source, fixture=fixture, mode="exact")

    def test_slotnum_and_amsterdam_environment_fragment(self):
        fixture = {"block": {"slot": 2**64 - 1, "number": 123}}
        self.pair(
            returned(["slotnum", "number"]),
            constants([2**64 - 1, 123]),
            fixture=fixture,
            version="amsterdam",
        )
        self.pair(
            returned(["slotnum"]),
            returned(["slotnum"]),
            fixture=fixture,
            version="amsterdam",
            mode="exact",
        )

    def test_new_opcodes_fail_before_their_fork(self):
        for opcode, fork in [
            (0x48, "berlin"),
            (0x49, "shanghai"),
            (0x4A, "shanghai"),
            (0x4B, "osaka"),
        ]:
            with self.subTest(opcode=opcode):
                # Legacy PUSH1 works on every admitted fork. BLOBHASH consumes an index.
                prefix = "6000" if opcode == 0x49 else ""
                fixture = {
                    "accounts": [
                        {
                            "address": world.MID,
                            "code": f"0x{prefix}{opcode:02x}60005260206000f3",
                        }
                    ]
                }
                before = (
                    world.call(output=32) + "\npush 0\nmstore\npush 32\npush 0\nreturn"
                )
                self.pair(before, constants([0]), fixture=fixture, version=fork)

    def test_extcodecopy_padding_bounds_and_warm_costs(self):
        fixture = {"accounts": [{"address": world.MID, "code": "0x123456"}]}
        before = f"push 5\npush 1\npush 0\npush {world.MID}\nextcodecopy\npush 32\npush 0\nreturn"
        self.pair(
            before,
            constants([int.from_bytes(bytes.fromhex("3456") + bytes(30), "big")]),
            fixture=fixture,
        )
        self.pair(before, before, fixture=fixture, mode="exact")
        for fork in ("istanbul", "berlin", "osaka"):
            with self.subTest(fork=fork):
                self.pair(before, before, fixture=fixture, mode="exact", version=fork)
        for source, length in [(MASK, 32), (MASK, 0)]:
            with self.subTest(length=length):
                body = f"push {length}\npush {source}\npush 0\npush {world.MID}\nextcodecopy\nstop"
                self.pair(body, body, fixture=fixture, mode="exact")
        warm = {**fixture, "warm_addresses": [world.MID]}
        self.pair(before, before, fixture=warm, mode="exact")

    def test_coinbase_warmth_changes_at_shanghai(self):
        fixture = {
            "block": {"coinbase": world.MID},
            "accounts": [{"address": world.MID, "balance": 7}],
        }
        before = "coinbase\nbalance\npop\nstop"
        for fork in ("paris", "shanghai"):
            with self.subTest(fork=fork):
                self.pair(before, before, fixture=fixture, mode="exact", version=fork)

    def test_mir_context_and_nested_frames(self):
        fixture = {"block": {"timestamp": 42}}
        self.pair(
            "v0 = timestamp\nmstore 0, v0\nreturndata 0, 32",
            "mstore 0, 42\nreturndata 0, 32",
            layer="mir",
            fixture=fixture,
        )
        fixture["accounts"] = [
            {"address": world.MID, "code": world.bytecode(returned(["timestamp"]))}
        ]
        self.pair(
            world.call(output=32) + "\npop\npush 32\npush 0\nreturn",
            constants([42]),
            fixture=fixture,
        )

    def test_wrong_environment_fold_is_a_replayed_counterexample(self):
        _, record = self.pair(
            returned(["timestamp"]),
            constants([41]),
            fixture={"block": {"timestamp": 42}},
            status=1,
        )
        self.assertEqual(record["witness"]["observations"][0][1], [0] * 31 + [42])

    def test_malformed_context_is_rejected(self):
        for fixture in [
            {"block": {"slot": 2**64}},
            {"block": {"hashes": {"1": 1, "0x1": 2}}},
            {"block": {"unknown": 0}},
            {"blob_hashes": [0] * 65},
        ]:
            with self.subTest(fixture=fixture):
                result, _ = self.pair("stop", "stop", fixture=fixture, status=2)
                self.assertIn("inconclusive:", result.stderr)
