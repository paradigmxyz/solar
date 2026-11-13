"""Deferred deletion, EIP-6780, refunds and rollback through real entry replay."""

import unittest

import test_world as world


class SelfdestructTests(unittest.TestCase):
    pair = world.WorldTests.pair

    def test_existing_account_preserved_from_cancun(self):
        fixture = {
            "accounts": [
                {"address": world.ADDRESS, "balance": 100, "storage": {"0": 7}},
                {"address": world.MID, "nonce": 1},
            ]
        }
        before = f"push {world.MID}\nselfdestruct"
        for fork in ("shanghai", "cancun"):
            with self.subTest(fork=fork):
                _, record = self.pair(
                    before, "stop", fixture=fixture, version=fork, status=1
                )
                accounts = record["witness"]["observations"][0][4]
                roots = [a for a in accounts if a[0] == str(world.ADDRESS)]
                self.assertEqual(len(roots), int(fork == "cancun"))
                if roots:
                    self.assertEqual(roots[0][2], "0")
                    self.assertEqual(roots[0][4], [["0", "7"]])
                self.assertEqual(
                    next(a for a in accounts if a[0] == str(world.MID))[2], "100"
                )

    def test_self_beneficiary_retains_funds_only_for_old_cancun_account(self):
        fixture = {"accounts": [{"address": world.ADDRESS, "balance": 100}]}
        before = f"push {world.ADDRESS}\nselfdestruct"
        self.pair(before, "stop", fixture=fixture, version="cancun")
        _, record = self.pair(
            before, "stop", fixture=fixture, version="shanghai", status=1
        )
        self.assertEqual(record["witness"]["observations"][0][4], [])

    def test_created_account_is_deleted_in_cancun(self):
        dying = world.bytecode(f"push {world.ADDRESS}\nselfdestruct")
        before = world.creation(dying, value=7) + "\npop\nstop"
        after = world.creation("0x00", value=7) + "\npop\nstop"
        _, record = self.pair(
            before,
            after,
            fixture={"accounts": [{"address": world.ADDRESS, "balance": 100}]},
            status=1,
        )
        accounts = record["witness"]["observations"][0][4]
        self.assertEqual(len(accounts), 1)
        self.assertEqual(accounts[0][1:3], ["2", "100"])

    def test_code_remains_visible_until_transaction_end(self):
        code = world.bytecode(f"push {world.LEAF}\nselfdestruct")
        fixture = {"accounts": [{"address": world.MID, "code": code, "balance": 7}]}
        prefix = world.call() + "\npop\n"
        suffix = "push 0\nmstore\npush 32\npush 0\nreturn"
        before = prefix + f"push {world.MID}\nextcodesize\n" + suffix
        after = prefix + f"push {len(bytes.fromhex(code[2:]))}\n" + suffix
        self.pair(before, after, fixture=fixture, version="shanghai")

    def test_parent_revert_restores_deletion_balance_and_refund(self):
        fixture = {
            "accounts": [
                {
                    "address": world.MID,
                    "code": world.bytecode(f"push {world.LEAF}\nselfdestruct"),
                    "balance": 7,
                }
            ]
        }
        self.pair(
            world.call() + "\npop\npush 0\npush 0\nrevert",
            "push 0\npush 0\nrevert",
            fixture=fixture,
            version="berlin",
        )

    def test_static_permission_and_gas_forks(self):
        fixture = {
            "accounts": [
                {"address": world.ADDRESS, "balance": 100},
                {"address": world.MID, "nonce": 1},
            ]
        }
        before = f"push {world.MID}\nselfdestruct"
        for fork in (
            "istanbul",
            "berlin",
            "london",
            "paris",
            "shanghai",
            "cancun",
            "prague",
            "osaka",
        ):
            with self.subTest(fork=fork):
                self.pair(before, before, fixture=fixture, version=fork, mode="exact")
        _, record = self.pair(
            before, "stop", fixture={**fixture, "static": True}, status=1
        )
        self.assertEqual(record["witness"]["observations"][0][0], "exception")

    def test_no_double_refund_and_no_premature_deletion(self):
        fixture = {
            "accounts": [
                {
                    "address": world.MID,
                    "code": world.bytecode(f"push {world.LEAF}\nselfdestruct"),
                    "balance": 7,
                }
            ]
        }
        before = world.call() + "\npop\n" + world.call() + "\npop\nstop"
        self.pair(before, before, fixture=fixture, version="berlin", mode="exact")

    def test_create2_cannot_reuse_destroyed_address_in_same_transaction(self):
        init = world.bytecode(f"push {world.ADDRESS}\nselfdestruct")
        prefix = (
            world.creation(init, salt=0)
            + "\npop\n"
            + world.creation(init, salt=0)
            + "\n"
        )
        suffix = "push 0\nmstore\npush 32\npush 0\nreturn"
        self.pair(prefix + suffix, prefix + "pop\npush 0\n" + suffix)
