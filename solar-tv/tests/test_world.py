"""Entry proofs with real storage, nested calls, constructors and native replay."""

import json
import subprocess
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

ADDRESS = 65536
MID = 196608
LEAF = 262144


def bytecode(text):
    binary = str(Path(SOLAR).with_name("solar-tv-replay"))
    definitions = json.loads(subprocess.check_output([binary, "--catalog"], text=True))
    names = {op["name"]: op["opcode"] for op in definitions["evmir"]}
    code = bytearray()
    for line in text.splitlines():
        if not line.strip():
            continue
        if line.startswith("push "):
            value = int(line[5:], 0)
            if value == 0:
                code.append(names["push0"])
            else:
                size = (value.bit_length() + 7) // 8
                code.append(names[f"push{size}"])
                code.extend(value.to_bytes(size, "big"))
        else:
            code.append(names[line])
    return "0x" + code.hex()


def call(to=MID, *, kind="call", gas=300000, value=0, output=0):
    args = [output, 0, 0, 0]
    if kind in {"call", "callcode"}:
        args.append(value)
    args.extend([to, gas])
    return "\n".join([*(f"push {arg}" for arg in args), kind])


def creation(initcode, *, salt=None, value=0):
    code = bytes.fromhex(initcode.removeprefix("0x"))
    assert 0 < len(code) <= 32
    stage = f"push {int.from_bytes(code, 'big')}\npush 0\nmstore\n"
    salt_arg = "" if salt is None else f"push {salt}\n"
    return (
        stage
        + salt_arg
        + f"push {len(code)}\npush {32 - len(code)}\npush {value}\n"
        + ("create" if salt is None else "create2")
    )


def deploy(runtime):
    code = bytes.fromhex(runtime.removeprefix("0x"))
    assert 0 < len(code) <= 32
    return bytecode(
        f"push {int.from_bytes(code, 'big')}\npush 0\nmstore\npush {len(code)}\npush {32 - len(code)}\nreturn"
    )


class WorldTests(unittest.TestCase):
    def pair(
        self,
        before,
        after,
        *,
        fixture=None,
        status=0,
        mode=None,
        layer="evmir",
        height=0,
        version="cancun",
    ):
        with tempfile.TemporaryDirectory() as tmp:
            folder = Path(tmp)
            world = folder / "world.json"
            world.write_text(json.dumps({"version": 1, **(fixture or {})}))
            paths = [folder / f"{name}.{layer}" for name in ("before", "after")]
            for path, body in zip(paths, (before, after), strict=True):
                text = (
                    f"@module World\nbb0 [stack={height}]:\n{body}\n"
                    if layer == "evmir"
                    else f"@module World\n@phase lowered\nfn @entry() [entry] {{\nbb0:\n{body}\n}}\n"
                )
                path.write_text(text)
            args = [
                CLI,
                layer,
                *paths,
                "--solar",
                SOLAR,
                "--world",
                world,
                f"--evm-version={version}",
                "--report",
                folder / "audit.json",
                "--verbose",
            ]
            if mode:
                args.append(f"--gas={mode}")
            result = subprocess.run(
                args, text=True, capture_output=True, check=False, timeout=180
            )
            record = json.loads((folder / "audit.json").read_text())
        self.assertEqual(result.returncode, status, result.stdout + result.stderr)
        if status in {0, 1}:
            self.assertIn("world", record)
            self.assertIn(
                "compiled full entry; trusted compiler backend lowering",
                record["assumptions"],
            )
            self.assertEqual(len(record["world"]["backend_images"]), 2)
        return result, record

    def test_storage_counterexample_names_the_slot(self):
        result, _ = self.pair(
            "push 7\npush 0\nsstore\nstop", "push 8\npush 0\nsstore\nstop", status=1
        )
        self.assertIn("Storage slot 0 changed at account", result.stdout)
        self.assertIn("Rust interpreter: matched", result.stdout)

    def test_storage_dead_write_in_one_world(self):
        self.pair(
            "push 1\npush 0\nsstore\npush 7\npush 0\nsstore\nstop",
            "push 7\npush 0\nsstore\nstop",
        )

    def test_symbolic_storage_load(self):
        fixture = {"accounts": [{"address": ADDRESS, "storage": {"0": "$seed"}}]}
        # An unused SLOAD is removable for every stored word, at this budget.
        self.pair("push 0\nsload\npop\nstop", "stop", fixture=fixture)

    def test_loaded_word_reaches_return_data(self):
        before = "push 0\nsload\npush 0\nadd\npush 0\nmstore\npush 32\npush 0\nreturn"
        fixture = {"accounts": [{"address": ADDRESS, "storage": {"0": 2**255 + 7}}]}
        self.pair(before, before.replace("push 0\nadd\n", ""), fixture=fixture)

    def test_nested_calls_commit_child_storage(self):
        leaf = bytecode("push 7\npush 0\nsstore\nstop")
        mid = bytecode(call(LEAF, gas=100000) + "\npop\nstop")
        fixture = {
            "accounts": [{"address": MID, "code": mid}, {"address": LEAF, "code": leaf}]
        }
        before = call(gas=500000) + "\npop\nstop"
        self.pair(
            before,
            before.replace("pop\nstop", "pop\npush 0\npop\nstop"),
            fixture=fixture,
        )
        _, record = self.pair(
            before, call(gas=2200) + "\npop\nstop", fixture=fixture, status=1
        )
        before_world = record["witness"]["observations"][0]
        leaf_account = next(entry for entry in before_world[4] if entry[0] == str(LEAF))
        self.assertEqual(leaf_account[4], [["0", "7"]])

    def test_reverted_child_restores_storage_but_returns_data(self):
        child = bytecode(
            "push 7\npush 0\nsstore\npush 42\npush 0\nmstore8\npush 1\npush 0\nrevert"
        )
        fixture = {"accounts": [{"address": MID, "code": child}]}
        before = call(output=1) + "\npop\npush 1\npush 0\nreturn"
        result, record = self.pair(before, "stop", fixture=fixture, status=1)
        observation = record["witness"]["observations"][0]
        self.assertEqual(observation[1], [42])
        self.assertEqual(next(a for a in observation[4] if a[0] == str(MID))[4], [])
        self.assertIn("Returned or reverted byte payload changed", result.stdout)

    def test_staticcall_rejects_callee_write(self):
        fixture = {
            "accounts": [
                {"address": MID, "code": bytecode("push 7\npush 0\nsstore\nstop")}
            ]
        }
        suffix = "\npush 0\nmstore\npush 32\npush 0\nreturn"
        _, record = self.pair(
            call(kind="staticcall") + suffix, call() + suffix, fixture=fixture, status=1
        )
        self.assertEqual(record["witness"]["observations"][0][1], [0] * 32)
        self.assertEqual(record["witness"]["observations"][1][1], [0] * 31 + [1])

    def test_delegatecall_keeps_storage_owner_and_caller(self):
        child = bytecode("caller\npush 0\nsstore\naddress\npush 1\nsstore\nstop")
        fixture = {"accounts": [{"address": MID, "code": child}]}
        _, record = self.pair(
            call(kind="delegatecall") + "\npop\nstop",
            call() + "\npop\nstop",
            fixture=fixture,
            status=1,
        )
        root = next(
            a for a in record["witness"]["observations"][0][4] if a[0] == str(ADDRESS)
        )
        self.assertEqual(root[4], [["0", "131072"], ["1", str(ADDRESS)]])

    def test_value_transfer_and_parent_rollback(self):
        fixture = {
            "accounts": [
                {"address": ADDRESS, "balance": 100},
                {"address": MID, "code": "0x00"},
            ]
        }
        before = call(value=7) + "\npop\npush 0\npush 0\nrevert"
        self.pair(before, "push 0\npush 0\nrevert", fixture=fixture)
        result, _ = self.pair(
            call(value=7) + "\npop\nstop",
            call(value=8) + "\npop\nstop",
            fixture=fixture,
            status=1,
        )
        self.assertIn("Balance changed at account", result.stdout)

    def test_mir_entry_executes_storage(self):
        self.pair("sstore 0, 7\nstop", "sstore 0, 8\nstop", layer="mir", status=1)

    def test_sstore_sentry_is_not_a_noop_elision(self):
        self.pair(
            "push 0\npush 0\nsstore\nstop",
            "stop",
            fixture={"gas": 2300, "warm_slots": [[ADDRESS, 0]]},
            status=1,
        )

    def test_unsupported_child_cannot_prove_identity(self):
        # SHA256 is a real precompile, not an empty successful account.
        result, record = self.pair(
            call(2) + "\npop\nstop", call(2) + "\npop\nstop", status=2
        )
        self.assertIn("unmodeled precompile", result.stderr)
        self.assertEqual(record["status"], "unsupported")

    def test_create_deploys_and_calls_the_new_contract(self):
        init = deploy(bytecode("push 7\npush 0\nsstore\nstop"))
        # Save the returned address, then use it as the next CALL's code owner.
        before = (
            creation(init)
            + "\npush 64\nmstore\n"
            + "\n".join(["push 0"] * 5)
            + "\npush 64\nmload\npush 100000\ncall\npop\nstop"
        )
        _, record = self.pair(before, before + "\n", status=0)
        self.assertEqual(record["world"]["native_samples"], 1)
        _, record = self.pair(before, creation(init) + "\npop\nstop", status=1)
        deployed = next(
            a for a in record["witness"]["observations"][0][4] if a[0] != str(ADDRESS)
        )
        self.assertEqual(deployed[1], "1")
        self.assertEqual(
            deployed[3],
            list(bytes.fromhex(bytecode("push 7\npush 0\nsstore\nstop")[2:])),
        )
        self.assertEqual(deployed[4], [["0", "7"]])

    def test_create2_address_salt_and_collision(self):
        init = deploy("0x00")
        before = creation(init, salt=0) + "\npop\nstop"
        result, record = self.pair(
            before, creation(init, salt=1) + "\npop\nstop", status=1
        )
        self.assertIn("Account creation or existence changed", result.stdout)
        first = record["witness"]["observations"][0]
        repeated = (
            creation(init, salt=0) + "\npop\n" + creation(init, salt=0) + "\npop\nstop"
        )
        _, record = self.pair(before, repeated, status=1)
        after = record["witness"]["observations"][1]
        self.assertEqual(len(first[4]), 2)
        self.assertEqual(len(after[4]), 2)
        root = next(a for a in after[4] if a[0] == str(ADDRESS))
        self.assertEqual(root[1], "3")

    def test_constructor_revert_keeps_nonce_and_return_data(self):
        init = bytecode("push 42\npush 0\nmstore8\npush 1\npush 0\nrevert")
        before = (
            creation(init)
            + "\npop\npush 1\npush 0\npush 0\nreturndatacopy\npush 1\npush 0\nreturn"
        )
        _, record = self.pair(before, "stop", status=1)
        observation = record["witness"]["observations"][0]
        self.assertEqual(observation[1], [42])
        self.assertEqual(len(observation[4]), 1)
        self.assertEqual(observation[4][0][1], "2")

    def test_create_code_deposit_failure_and_parent_rollback(self):
        # A 64-byte runtime needs 12,800 deposit gas, unavailable after CREATE.
        init = bytecode("push 64\npush 0\nreturn")
        before = creation(init) + "\npop\nstop"
        _, record = self.pair(before, "stop", fixture={"gas": 40000}, status=1)
        observation = record["witness"]["observations"][0]
        self.assertEqual(len(observation[4]), 1)
        self.assertEqual(observation[4][0][1], "2")
        before = creation(deploy("0x00"), value=7) + "\npop\npush 0\npush 0\nrevert"
        self.pair(
            before,
            "push 0\npush 0\nrevert",
            fixture={"accounts": [{"address": ADDRESS, "balance": 100}]},
        )

    def test_constructor_rejects_reserved_code_prefix(self):
        _, record = self.pair(
            creation(deploy("0xef")) + "\npop\nstop",
            creation(deploy("0xfe")) + "\npop\nstop",
            status=1,
        )
        observations = record["witness"]["observations"]
        self.assertEqual(len(observations[0][4]), 1)
        self.assertEqual(len(observations[1][4]), 2)

    def test_callcode_and_identity_precompile(self):
        fixture = {
            "accounts": [
                {"address": MID, "code": bytecode("caller\npush 0\nsstore\nstop")}
            ]
        }
        _, record = self.pair(
            call(kind="callcode") + "\npop\nstop",
            call(kind="delegatecall") + "\npop\nstop",
            fixture=fixture,
            status=1,
        )
        self.assertEqual(
            next(
                a
                for a in record["witness"]["observations"][0][4]
                if a[0] == str(ADDRESS)
            )[4],
            [["0", str(ADDRESS)]],
        )
        # One input byte is copied synchronously by precompile 4.
        before = "push 42\npush 0\nmstore8\npush 1\npush 0\npush 1\npush 0\npush 0\npush 4\npush 10000\ncall\npop\npush 1\npush 0\nreturn"
        self.pair(before, "push 42\npush 0\nmstore8\npush 1\npush 0\nreturn")

    def test_gas_policy_and_refunds(self):
        fixture = {"accounts": [{"address": ADDRESS, "storage": {"0": 7}}]}
        before = "push 0\npush 0\nsstore\nstop"
        self.pair(before, before + "\n", fixture=fixture, mode="exact")
        self.pair("push 0\npop\nstop", "stop", mode="refine")
        self.pair("push 0\npop\nstop", "stop", mode="exact", status=1)
        self.pair("push 0\npop\nstop", "stop", mode="refine", fixture={"gas": 0})

    def test_model_limits_are_not_exception_equivalence(self):
        result, record = self.pair(
            "push 0\npop\nstop", "push 0\npop\nstop", fixture={"fuel": 1}, status=2
        )
        self.assertEqual(record["status"], "unsupported")
        self.assertIn("evaluation limit", result.stderr)

    def test_untouched_accounts_and_storage_remain_observable(self):
        fixture = {
            "accounts": [
                {"address": ADDRESS, "storage": {"0": 7, "1": 8}},
                {"address": MID, "balance": 9, "storage": {"2": 10}},
            ]
        }
        self.pair("push 0\nsload\npop\nstop", "stop", fixture=fixture)

    def test_call_rejection_stipend_and_static_callcode(self):
        fixture = {"accounts": [{"address": MID, "code": "0x00"}]}
        before = call(value=7) + "\npop\nstop"
        self.pair(before, before, fixture=fixture, mode="exact")
        # CALLCODE may carry CALLVALUE in a static frame; it transfers no value.
        fixture = {
            "static": True,
            "accounts": [
                {"address": ADDRESS, "balance": 100},
                {
                    "address": MID,
                    "code": bytecode(
                        "callvalue\npush 0\nmstore\npush 32\npush 0\nreturn"
                    ),
                },
            ],
        }
        suffix = "\npop\npush 32\npush 0\nreturn"
        self.pair(
            call(kind="callcode", value=7, output=32) + suffix,
            call(kind="callcode", value=8, output=32) + suffix,
            fixture=fixture,
            status=1,
            mode="exact",
        )

    def test_storage_and_call_gas_on_every_admitted_fork(self):
        fixture = {
            "accounts": [
                {"address": ADDRESS, "storage": {"0": 7}},
                {"address": MID, "code": "0x00"},
            ]
        }
        before = "push 0\nsload\npop\npush 0\npush 0\nsstore\n" + call() + "\npop\nstop"
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
                self.pair(before, before, fixture=fixture, mode="exact", version=fork)

    def test_transient_storage_and_logs_roll_back(self):
        child = bytecode(
            "push 7\npush 0\ntstore\npush 9\npush 0\npush 0\nlog1\npush 0\npush 0\nrevert"
        )
        fixture = {"accounts": [{"address": MID, "code": child}]}
        self.pair(call(kind="delegatecall") + "\npop\nstop", "stop", fixture=fixture)
        before = "push 7\npush 0\ntstore\npush 0\ntload\npush 0\nsstore\npush 9\npush 0\npush 0\nlog1\nstop"
        _, record = self.pair(before, "stop", status=1)
        observation = record["witness"]["observations"][0]
        self.assertEqual(observation[4][0][4], [["0", "7"]])
        self.assertEqual(observation[5], [[str(ADDRESS), ["9"], []]])

    def test_mir_entry_calls_and_creates(self):
        fixture = {
            "accounts": [
                {"address": MID, "code": bytecode("push 7\npush 0\nsstore\nstop")}
            ]
        }
        self.pair(
            f"v0 = call 300000, {MID}, 0, 0, 0, 0, 0\nstop",
            "stop",
            fixture=fixture,
            layer="mir",
            status=1,
        )
        init = deploy("0x00")
        code = bytes.fromhex(init[2:])
        before = f"mstore 0, {int.from_bytes(code, 'big')}\nv0: i256 = create 0, {32 - len(code)}, {len(code)}\nstop"
        self.pair(before, "stop", layer="mir", status=1)

    def test_symbolic_storage_key_aliases_are_not_assumed_distinct(self):
        fixture = {"accounts": [{"address": ADDRESS, "storage": {"$key": 7, "0": 8}}]}
        before = "push 0\nsload\npush 0\nmstore\npush 32\npush 0\nreturn"
        after = "push 7\npush 0\nmstore\npush 32\npush 0\nreturn"
        _, record = self.pair(before, after, fixture=fixture, status=1)
        self.assertEqual(record["witness"]["inputs"], {"state0": "0"})
        self.assertEqual(record["witness"]["observations"][0][1], [0] * 31 + [8])

    def test_nested_factory_and_rlp_nonce_boundaries(self):
        factory = bytecode(creation(deploy("0x00")) + "\npop\nstop")
        for nonce in (0, 127, 128):
            with self.subTest(nonce=nonce):
                fixture = {
                    "accounts": [{"address": MID, "nonce": nonce, "code": factory}]
                }
                _, record = self.pair(
                    call() + "\npop\nstop", "stop", fixture=fixture, status=1
                )
                accounts = record["witness"]["observations"][0][4]
                self.assertEqual(len(accounts), 3)
                self.assertEqual(
                    next(a for a in accounts if a[0] == str(MID))[1], str(nonce + 1)
                )

    def test_documented_examples(self):
        for layer, example, other, status in [
            ("mir", "storage", "broken", 1),
            ("evmir", "storage", "after", 0),
            ("evmir", "creation", "broken", 1),
        ]:
            with self.subTest(example=example, layer=layer):
                folder = ROOT / "examples" / layer / example
                result = subprocess.run(
                    [
                        CLI,
                        layer,
                        folder / f"before.{layer}",
                        folder / f"{other}.{layer}",
                        "--solar",
                        SOLAR,
                        "--world",
                        folder / "world.json",
                    ],
                    text=True,
                    capture_output=True,
                    check=False,
                    timeout=180,
                )
                self.assertEqual(
                    result.returncode, status, result.stdout + result.stderr
                )
                if example == "creation":
                    self.assertIn("Deployed code changed at account", result.stdout)
