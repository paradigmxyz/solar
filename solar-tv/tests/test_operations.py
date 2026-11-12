"""Schema coverage and scalar boundaries against kernel and Rust execution."""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import operations

REPLAY = str(Path(SOLAR).with_name("solar-tv-replay"))
MASK = 2**256 - 1
SIGN = 2**255


def scalar_cases():
    return [
        ("add", [MASK, 1], 0),
        ("sub", [0, 1], MASK),
        ("mul", [SIGN, 2], 0),
        ("div", [MASK, 0], 0),
        ("div", [MASK, 2], MASK // 2),
        ("sdiv", [MASK - 6, 3], MASK - 1),
        ("sdiv", [SIGN, MASK], SIGN),
        ("sdiv", [MASK, 0], 0),
        ("mod", [7, 0], 0),
        ("mod", [7, 3], 1),
        ("smod", [MASK - 6, 3], MASK),
        ("smod", [MASK, 0], 0),
        ("exp", [2, 256], 0),
        ("exp", [MASK, MASK], MASK),
        ("exp", [0, 0], 1),
        ("addmod", [MASK, MASK, MASK - 1], 2),
        ("addmod", [MASK, MASK, 0], 0),
        ("mulmod", [MASK, MASK, MASK - 1], 1),
        ("mulmod", [MASK, MASK, 0], 0),
        ("signextend", [0, 128], MASK - 127),
        ("signextend", [MASK, 128], 128),
        ("lt", [0, MASK], 1),
        ("gt", [0, MASK], 0),
        ("slt", [SIGN, 0], 1),
        ("sgt", [MASK, 0], 0),
        ("eq", [MASK, MASK], 1),
        ("iszero", [0], 1),
        ("and", [MASK, 17], 17),
        ("or", [2, 1], 3),
        ("xor", [3, 1], 2),
        ("not", [MASK], 0),
        ("byte", [0, 128 << 248], 128),
        ("byte", [32, MASK], 0),
        ("shl", [256, 1], 0),
        ("shr", [255, SIGN], 1),
        ("sar", [256, SIGN], MASK),
        ("sar", [MASK, 1], 0),
        ("clz", [0], 256),
        ("clz", [SIGN], 0),
    ]


def mir(body, width=256):
    return f"@module Ops\nfn @f() -> i{width} {{\nbb0:\n{body}\n}}\n"


def evm(body, height):
    return f"@module Ops\nbb0 [stack=0]:\n{body}\n  push bb1\n  jump\nbb1 [stack={height}]:\n  stop\n"


class OperationTests(unittest.TestCase):
    def native(self, layer, before, after, expected):
        request = {
            "version": 1,
            "kind": layer,
            "before": before,
            "after": after,
            "inputs": {},
            "memory": False,
            "gas": False,
            "evm_version": "osaka",
        }
        result = subprocess.run(
            [REPLAY],
            input=json.dumps(request),
            text=True,
            capture_output=True,
            check=True,
            timeout=30,
        )
        self.assertEqual(
            json.loads(result.stdout),
            {"version": 1, "status": "ok", "observations": [expected, expected]},
        )

    def pair(self, layer, before, after, status=0):
        with tempfile.TemporaryDirectory() as tmp:
            paths = [Path(tmp) / f"{name}.{layer}" for name in ("before", "after")]
            for path, text in zip(paths, (before, after), strict=True):
                path.write_text(text)
            result = subprocess.run(
                [CLI, layer, *paths, "--solar", SOLAR, "--evm-version=osaka"],
                text=True,
                capture_output=True,
                check=False,
                timeout=120,
            )
        self.assertEqual(result.returncode, status, result.stdout + result.stderr)
        return result

    def test_reflected_pure_vocabulary_is_exhaustive(self):
        catalog = json.loads(subprocess.check_output([REPLAY, "--catalog"], text=True))
        pure = {op["name"] for op in catalog["evmir"] if op["pure"]}
        self.assertEqual(
            pure, operations.UNARY | operations.BINARY | operations.TERNARY
        )
        self.assertEqual(pure, {op for op, _, _ in scalar_cases()})
        self.assertEqual(len(catalog["mir"]), 107)
        self.assertEqual(len(catalog["evmir"]), 154)
        self.assertEqual(len(catalog["evmir"][0]["forks"]), 15)

    def test_all_evm_word_operations(self):
        before, after, expected = [], [], []
        for op, args, result in scalar_cases():
            before.extend(f"  push {value}" for value in reversed(args))
            before.append(f"  {op}")
            after.append(f"  push {result}")
            expected.insert(0, result)
        a, b = (
            evm("\n".join(before), len(expected)),
            evm("\n".join(after), len(expected)),
        )
        self.pair("evmir", a, b)
        self.native("evmir", a, b, f"bb1, stack={expected}")

    def test_mir_word_operations(self):
        before, after, total = [], [], 0
        for index, (op, args, result) in enumerate(scalar_cases()):
            if op == "iszero":
                continue
            if op in operations.SHIFTS:
                args = list(reversed(args))
            name = f"v{index * 3}"
            before.append(f"{name} = {op} " + ", ".join(map(str, args)))
            if op in operations.COMPARISONS:
                before.append(f"v{index * 3 + 1} = zext i1 {name} to i256")
                name = f"v{index * 3 + 1}"
            previous = "0" if not after else after[-1]
            acc = f"v{index * 3 + 2}"
            before.append(f"{acc} = add {name}, {previous}")
            after.append(acc)
            total = (total + result) & MASK
        a, b = mir("\n".join([*before, f"ret {after[-1]}"])), mir(f"ret {total}")
        self.pair("mir", a, b)
        self.native("mir", a, b, str(total))

    def test_narrow_signed_operations(self):
        for width in (8, 32, 160, 256):
            mask = 2**width - 1
            a = mir(
                f"v0 = sdiv i{width} {mask - 6}, i{width} 3\nv1 = sar v0, i{width} 1\nv2 = clz v1\nret v2",
                width,
            )
            b = mir(f"ret i{width} 0", width)
            self.pair("mir", a, b)
            self.native("mir", a, b, "0")

    def test_checked_division_remainder_and_power(self):
        cases = [
            ("checked_div", "uint256", 7, 0, "revert Panic(18)"),
            ("checked_rem", "int256", MASK, 0, "revert Panic(18)"),
            ("checked_div", "int256", SIGN, MASK, "revert Panic(17)"),
            ("wrapping_div", "int256", SIGN, MASK, str(SIGN)),
            ("checked_rem", "int256", SIGN, MASK, "0"),
            ("checked_pow", "uint256", 2, 256, "revert Panic(17)"),
            ("checked_pow", "int256", MASK - 1, 3, str(MASK - 7)),
            ("checked_pow", "uint256", MASK, 0, "1"),
            ("checked_pow", "uint256", 1, MASK, "1"),
        ]
        for op, kind, a, b, expected in cases:
            with self.subTest(op=op, kind=kind, a=a, b=b):
                before = mir(f"v0 = {op} {kind}, {a}, {b}\nret v0")
                after = mir(
                    f"icall panic_if<{expected[13:-1]}>, true\ninvalid"
                    if expected.startswith("revert")
                    else f"ret {expected}"
                )
                self.pair("mir", before, after)
                self.native("mir", before, after, expected)

    def test_broken_widened_modulus_is_replayed(self):
        a = mir(f"v0 = addmod {MASK}, {MASK}, {MASK - 1}\nret v0")
        b = mir("ret 0")
        result = self.pair("mir", a, b, status=1)
        self.assertIn("before = 2\n  after  = 0", result.stdout)

    def test_unavailable_mir_opcode_cannot_be_verified(self):
        with tempfile.TemporaryDirectory() as tmp:
            a, b = Path(tmp) / "before.mir", Path(tmp) / "after.mir"
            a.write_text(mir(f"v0 = clz {MASK}\nret v0"))
            b.write_text(mir("ret 0"))
            result = subprocess.run(
                [CLI, "mir", a, b, "--solar", SOLAR, "--evm-version=cancun"],
                text=True,
                capture_output=True,
                check=False,
                timeout=30,
            )
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("unavailable on cancun", result.stderr)

    def test_fork_prices_against_the_opcode_schema(self):
        from validator import gas

        catalog = json.loads(subprocess.check_output([REPLAY, "--catalog"], text=True))
        self.assertEqual(set(catalog["evmir"][0]["forks"]), gas.VERSIONS)
        examples = []
        for operation in catalog["evmir"]:
            for fork, schedule in operation["forks"].items():
                if not schedule["available"]:
                    continue
                if operation["pure"]:
                    examples.append(
                        f"example : EVM.Gas.pureCost .{operation['name']} .{fork} = {schedule['base_gas']} := by decide"
                    )
                if operation["name"] == "exp":
                    for exponent in (0, 1, 255, 256, SIGN, MASK):
                        price = schedule["base_gas"] + schedule["dynamic_gas"] * (
                            (exponent.bit_length() + 7) // 8
                        )
                        examples.append(
                            f"example : EVM.Gas.cost (.stack .exp) [0, {exponent}] ⟨[], 0⟩ .{fork} = {price} := by decide"
                        )
                if operation["name"] in {"push0", "push1"}:
                    value = int(operation["name"] == "push1")
                    examples.append(
                        f"example : EVM.Gas.pureCost (.push {value}) .{fork} = {schedule['base_gas']} := by decide"
                    )
                if operation["name"] == "sload":
                    examples.append(
                        f"example : Storage.readPrice .{fork} false = {schedule['base_gas']} := by decide"
                    )
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "Prices.lean"
            path.write_text(
                "import SolarTV\nopen SolarTV\nset_option maxRecDepth 4096\n"
                + "\n".join(examples)
            )
            result = subprocess.run(
                ["lake", "env", "lean", path],
                cwd=ROOT,
                text=True,
                capture_output=True,
                check=False,
                timeout=60,
            )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_select_and_extended_stack_forms(self):
        self.pair("mir", mir("v0 = select true, 7, 9\nret v0"), mir("ret 7"))
        with tempfile.TemporaryDirectory() as tmp:
            for depth, fork in [(16, "cancun"), (17, "amsterdam"), (235, "amsterdam")]:
                paths = [Path(tmp) / f"{name}.evmir" for name in ("before", "after")]
                for path, code in zip(paths, (f"dup {depth}\npop", ""), strict=True):
                    path.write_text(
                        f"@module Stack\nbb0 [stack={depth}]:\n{code}\nstop\n"
                    )
                result = subprocess.run(
                    [CLI, "evmir", *paths, "--solar", SOLAR, f"--evm-version={fork}"],
                    text=True,
                    capture_output=True,
                    check=False,
                    timeout=120,
                )
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            for fork in ("cancun", "amsterdam"):
                for path, code in zip(
                    paths, ("exchange 1, 2", "swap 1\nswap 2\nswap 1"), strict=True
                ):
                    path.write_text(
                        f"@module Stack\nbb0 [stack=3]:\n{code}\npush bb1\njump\nbb1 [stack=3]:\nstop\n"
                    )
                result = subprocess.run(
                    [CLI, "evmir", *paths, "--solar", SOLAR, f"--evm-version={fork}"],
                    text=True,
                    capture_output=True,
                    check=False,
                    timeout=120,
                )
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_operation_inventory_accounts_for_both_schemas(self):
        catalog = json.loads(subprocess.check_output([REPLAY, "--catalog"], text=True))
        inventory = json.loads((ROOT / "operations.json").read_text())
        for layer in ("mir", "evmir"):
            self.assertEqual(
                set(inventory[layer]), {entry["name"] for entry in catalog[layer]}
            )
            self.assertLessEqual(
                set(inventory[layer].values()), set(inventory["legend"])
            )
        for name in ("call", "callcode", "delegatecall", "staticcall"):
            self.assertEqual(inventory["evmir"][name], "world_entry")
            self.assertEqual(inventory["mir"][name], "partial_family")
        for name in ("create", "create2"):
            self.assertEqual(inventory["evmir"][name], "world_entry")
            self.assertEqual(inventory["mir"][name], "partial_family")
        self.assertEqual(inventory["mir"]["returndatasize"], "direct_state")
        self.assertEqual(inventory["evmir"]["selfdestruct"], "world_entry")
        for name in (
            "extcodecopy",
            "blockhash",
            "coinbase",
            "timestamp",
            "number",
            "prevrandao",
            "gaslimit",
            "chainid",
            "basefee",
            "blobhash",
            "blobbasefee",
            "slotnum",
        ):
            self.assertEqual(inventory["evmir"][name], "world_entry")
        for name in (
            "mapping_slot",
            "mapping_slot_bytes",
            "storage_array_data_slot",
            "storage_array_element_slot",
            "balance",
            "extcodesize",
            "extcodehash",
            "extcodecopy",
            "validate_storage_bytes",
            "load_storage_bytes",
            "store_storage_bytes",
            "store_storage_bytes_literal",
            "clear_storage_words",
            "insert_value",
            "extract_value",
            "make_memory_slice",
            "fmp",
            "memory_zero",
            "ptradd",
            "ptrtoint",
            "inttoptr",
            "log",
            "calldatasize",
            "timestamp",
        ):
            self.assertEqual(inventory["mir"][name], "direct_state")
        for name in ("icall", "alloc", "datacopy"):
            self.assertEqual(inventory["mir"][name], "partial_family")
        active = {
            str(address)
            for addresses in catalog["precompiles"].values()
            for address in addresses
        }
        self.assertEqual(set(inventory["precompiles"]), active)
