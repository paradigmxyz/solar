"""Typed immutable carriers, staging aliases, and signed runtime relocations."""

import unittest

import test_mir_abi as abi
import test_mir_layout as layout
import test_mir_state as state

MAX = 2**256 - 1


def module(body, ty):
    return layout.declarations(state.module(body), f"@immutables\n initial: {ty}")


class MirImmutableTests(unittest.TestCase):
    pair = state.MirStateTests.pair
    lower = abi.MirAbiTests.lower

    def test_scalar_runtime_relocations_keep_their_carriers(self):
        for ty, width, word, expected in (
            ("uint8", 8, 255, 255),
            ("int8", 8, MAX, 255),
            ("int8", 8, 255, 255),
            ("bool", 1, 1, 1),
            ("address", 160, 17, 17),
            ("bytes2", 256, (0xABCD << 240) | 7, (0xABCD << 240) | 7),
            ("function", 256, 4660, 4660),
        ):
            with self.subTest(ty=ty, word=word):
                body = "v0 = loadimmutable initial\n"
                body += (
                    f"v1 = zext i{width} v0 to i256\nret v1"
                    if width < 256
                    else "ret v0"
                )
                self.pair(
                    module(body, ty),
                    module(f"ret {expected}", ty),
                    fixture=layout.fixture(immutable_values={"0": str(word)}),
                )

    def test_staging_stores_and_aliases_use_zero_extended_words(self):
        for ty, width, value in (
            ("uint8", 8, 255),
            ("int8", 8, 255),
            ("bool", 1, 1),
            ("address", 160, 17),
        ):
            with self.subTest(ty=ty):
                body = (
                    f"v0 = trunc i256 {value} to i{width}\n"
                    "storeimmutable initial, v0\nmstore 256, 1\n"
                    f"v1 = loadimmutable initial\nv2 = zext i{width} v1 to i256\nret v2"
                )
                self.pair(
                    module(body, ty),
                    module(f"mstore 256, {value}\nmstore 256, 1\nret 1", ty),
                    fixture=layout.fixture(immutable_slots={"0": 256}),
                )

    def test_real_integer_lowering_cleans_signed_runtime_placeholders(self):
        before = module(
            "v0 = loadimmutable initial\nv1 = zext i8 v0 to i256\nret v1", "int8"
        )
        after = self.lower(before, "lower-integers")
        self.assertIn("and", after)
        initial = layout.fixture(immutable_values={"0": str(MAX)})
        self.pair(before, after, fixture=initial)
        # Keeping the signed placeholder without the carrier mask changes 255 to -1.
        broken = module("v0: i256 = loadimmutable initial\nret v0", "int8")
        self.pair(before, broken, fixture=initial, status=1)

    def test_raw_load_and_sign_extension_preserve_signed_bits(self):
        initial = layout.fixture(immutable_values={"0": str(MAX)})
        for body in (
            "v0: i256 = loadimmutable initial\nret v0",
            "v0 = loadimmutable initial\nv1 = sext i8 v0 to i256\nret v1",
        ):
            with self.subTest(body=body):
                self.pair(
                    module(body, "int8"), module(f"ret {MAX}", "int8"), fixture=initial
                )

    def test_invalid_relocation_values_are_inconclusive(self):
        for ty, word in (
            ("uint8", 256),
            ("int8", 256),
            ("bool", 2),
            ("address", 2**160),
        ):
            with self.subTest(ty=ty):
                source = module("v0: i256 = loadimmutable initial\nret v0", ty)
                result, _ = self.pair(
                    source,
                    source,
                    fixture=layout.fixture(immutable_values={"0": str(word)}),
                    status=2,
                )
                self.assertIn("non-canonical immutable", result.stderr)

    def test_signed_staging_does_not_accept_runtime_sign_extension(self):
        source = module("v0: i256 = loadimmutable initial\nret v0", "int8")
        initial = layout.fixture(immutable_slots={"0": 256})
        initial["memory"] = {
            "size": 288,
            "data": "0x" + (bytes(256) + bytes([255]) * 32).hex(),
        }
        result, _ = self.pair(source, source, fixture=initial, status=2)
        self.assertIn("non-canonical immutable", result.stderr)
