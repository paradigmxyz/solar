"""Pure Lean precompiles against evm2, plus real CLI entry counterexamples."""

import hashlib
import json
import subprocess
import tempfile
import unittest
from pathlib import Path

import test_world as world
from test_cli import ROOT, SOLAR

REPLAY = str(Path(SOLAR).with_name("solar-tv-replay"))
BLS_P = int(
    "1a0111ea397fe69a4b1ba7b6434bacd764774b84f38512bf6730d2a0f6b0f6241eabfffeb153ffffb9feffffffffaaab",
    16,
)
BLS_N = int("73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001", 16)
BN_P = 21888242871839275222246405745257275088696311157297823662689037894645226208583
G1 = (
    3685416753713387016781088315183077757961620795782546409894578378688607592378376318836054947676345821548104185464507,
    1339506544944476473020471379941921221584933875938349620426543736416511423956333506472724655353366534992391756441569,
)
G2 = (
    352701069587466618187139116011060144890029952792775240219908644239793785735715026873347600343865175952761926303160,
    3059144344244213709971259814753781636986470325476647558659373206291635324768958432433509563104347017837885763365758,
    1985150602287291935568054521177171638300868978215655730859378665066344726373823718423869104263333984641494340347905,
    927553665492332455747201965776037880757740193453592970025027978793976877002675564980949289727957565575433344219582,
)


def words(*values, width=32):
    return b"".join(value.to_bytes(width, "big") for value in values)


def case(address, data=b"", gas=1_000_000, fork="cancun"):
    return {"address": address, "input": list(data), "gas": gas, "evm_version": fork}


class PrecompileTests(unittest.TestCase):
    def compare(self, cases, *, timeout=180):
        native = subprocess.run(
            [REPLAY, "--precompiles"],
            input=json.dumps({"version": 1, "cases": cases}),
            text=True,
            capture_output=True,
            check=True,
            timeout=timeout,
        )
        output = json.loads(native.stdout)
        self.assertEqual(output["status"], "ok", output)
        statements = [
            "import SolarTV",
            "open SolarTV",
            "set_option maxRecDepth 16384",
            "set_option maxHeartbeats 8000000",
            "set_option exponentiation.threshold 8192",
        ]
        for entry in cases:
            data = entry["input"]
            literal = f"(List.replicate {len(data)} 0)" if not any(data) else str(data)
            statements.append(
                f"#eval IO.println (Precompiles.describe .{entry['evm_version']} {entry['address']} {literal} {entry['gas']})"
            )
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "Precompiles.lean"
            path.write_text("\n".join(statements) + "\n")
            lean = subprocess.run(
                ["lake", "env", "lean", path],
                cwd=ROOT,
                text=True,
                capture_output=True,
                check=False,
                timeout=timeout,
            )
        self.assertEqual(lean.returncode, 0, lean.stdout + lean.stderr)
        observations = [
            json.loads(line)
            for line in lean.stdout.splitlines()
            if line.startswith(("[", "{"))
        ]
        self.assertEqual(len(observations), len(cases), lean.stdout)
        for entry, model, actual in zip(
            cases, observations, output["observations"], strict=True
        ):
            with self.subTest(
                address=entry["address"],
                fork=entry["evm_version"],
                length=len(entry["input"]),
                gas=entry["gas"],
            ):
                self.assertEqual(model, actual)
        return observations

    def test_hashes_padding_and_gas(self):
        cases = []
        for address, base, per_word in [(2, 60, 12), (3, 600, 120), (4, 15, 3)]:
            for data in (b"", b"abc", bytes(range(64)), bytes(range(128))):
                cost = base + per_word * ((len(data) + 31) // 32)
                cases += [case(address, data, cost), case(address, data, cost - 1)]
        observations = self.compare(cases)
        self.assertEqual(bytes(observations[2][1]), hashlib.sha256(b"abc").digest())
        self.assertEqual(
            bytes(observations[8][1]).hex(),
            "00" * 12 + "9c1185a5c5e9fc54612808977ee8f548b2258d31",
        )

    def test_nontrivial_signature_is_kernel_checked(self):
        source = """import SolarTV
open SolarTV SolarTV.Precompiles
set_option maxRecDepth 16384
set_option maxHeartbeats 8000000
set_option exponentiation.threshold 8192
example : powMod 3 5 7 = 5 := by decide +kernel
theorem signature_vector : Signatures.recover
    (be 32 0 ++ be 32 27 ++
     be 32 0x79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798 ++
     be 32 0x79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798) =
    be 32 0x7e5f4552091a69125d5dfcb7b8c2659029395bdf := by decide +kernel
#print axioms signature_vector
"""
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "Signature.lean"
            path.write_text(source)
            result = subprocess.run(
                ["lake", "env", "lean", path],
                cwd=ROOT,
                text=True,
                capture_output=True,
                check=False,
                timeout=120,
            )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(
            result.stdout.strip(),
            "'signature_vector' depends on axioms: [propext, Quot.sound]",
        )

    def test_modexp_fork_prices_and_length_limits(self):
        data = words(1, 1, 1) + bytes([3, 5, 7])
        cases = [
            case(5, data, fork=fork)
            for fork in ("istanbul", "berlin", "cancun", "osaka")
        ]
        for fork, minimum in [("berlin", 200), ("osaka", 500)]:
            cases += [case(5, b"", minimum, fork), case(5, b"", minimum - 1, fork)]
        cases += [
            case(5, words(0, 0, 1025), fork="osaka"),
            case(5, words(0, 0, 1025), fork="cancun"),
            case(
                5,
                words(1, 33, 1) + bytes([3]) + bytes([0x80]) + bytes(32) + bytes([7]),
                fork="osaka",
            ),
        ]
        result = self.compare(cases)
        self.assertEqual(result[0][1], [pow(3, 5, 7)])
        self.assertEqual(result[-3][0], "exception")
        self.assertEqual(result[-2][1], [0] * 1025)

    def test_signatures_valid_invalid_and_high_s(self):
        kx = int("79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798", 16)
        korder = int(
            "fffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141", 16
        )
        px = int("6b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296", 16)
        py = int("4fe342e2fe1a7f9b8ee7eb4a7c0f9e162bce33576b315ececbb6406837bf51f5", 16)
        cases = [
            case(1, words(0, 27, kx, kx)),
            case(1, words(0, 28, kx, korder - kx)),
            case(1, b""),
            case(1, words(0, 29, kx, kx)),
            case(1, words(0, 27, kx, kx), 2999),
            case(256, words(0, px, px, px, py), 6900, "osaka"),
            case(256, b"", 6900, "osaka"),
            case(256, words(0, px, px, px, py), 6899, "osaka"),
            case(256, b"", fork="prague"),
        ]
        result = self.compare(cases)
        self.assertEqual(
            bytes(result[0][1]).hex(),
            "00" * 12 + "7e5f4552091a69125d5dfcb7b8c2659029395bdf",
        )
        self.assertEqual(result[1][1], result[0][1])
        self.assertEqual(result[5][1], [0] * 31 + [1])

    def test_bn254_add_mul_and_invalid_encodings(self):
        generator = words(1, 2)
        cases = [
            case(6, generator + bytes(64)),
            case(6, generator + generator),
            case(7, generator + words(2)),
            case(6, b""),
            case(6, words(BN_P, 2)),
            case(6, words(1, 1)),
            case(6, generator + bytes(64) + b"ignored"),
            case(7, generator + words(0)),
            case(8, b""),
            case(8, b"x"),
        ]
        result = self.compare(cases)
        self.assertEqual(result[1][1], result[2][1])
        self.assertEqual(result[0][1], list(generator))

    def test_bls_add_msm_and_subgroup_checks(self):
        g1, g2 = words(*G1, width=64), words(*G2, width=64)
        cases = [
            case(11, g1 + bytes(128), fork="prague"),
            case(11, g1 + g1, fork="prague"),
            case(12, g1 + words(2), fork="prague"),
            case(13, g2 + bytes(256), fork="prague"),
            case(13, g2 + g2, fork="prague"),
            case(14, g2 + words(2), fork="prague"),
            case(11, words(0, 2, width=64) + bytes(128), fork="prague"),
            case(12, words(0, 2, width=64) + words(1), fork="prague"),
            case(11, words(BLS_P, 2, width=64) + bytes(128), fork="prague"),
            case(12, b"", fork="prague"),
            case(14, b"", fork="prague"),
            case(15, bytes(384), fork="prague"),
            case(15, b"", fork="prague"),
        ]
        result = self.compare(cases)
        self.assertEqual(result[0][1], list(g1))
        self.assertEqual(result[1][1], result[2][1])
        self.assertEqual(result[4][1], result[5][1])
        self.assertEqual(result[7][0], "exception")

    def test_bls_field_to_curve_maps(self):
        cases = [
            case(16, words(value, width=64), fork="prague")
            for value in (0, 1, 42, BLS_P)
        ]
        cases += [
            case(17, words(*value, width=64), fork="prague")
            for value in ((0, 0), (1, 2), (42, 7), (BLS_P, 0))
        ]
        self.compare(cases)

    def test_kzg_infinity_constant_polynomial_and_invalid_proofs(self):
        infinity = bytes([0xC0]) + bytes(47)
        compressed = (G1[0] | (1 << 383) | (int(G1[1] > BLS_P // 2) << 381)).to_bytes(
            48, "big"
        )
        data = (
            b"\x01"
            + hashlib.sha256(infinity).digest()[1:]
            + words(0, 0)
            + infinity
            + infinity
        )
        constant = (
            b"\x01"
            + hashlib.sha256(compressed).digest()[1:]
            + words(0, 1)
            + compressed
            + infinity
        )
        bad = constant[:64] + words(2) + constant[96:]
        result = self.compare(
            [
                case(10, data),
                case(10, constant),
                case(10, bad),
                case(10, data, 49999),
                case(10, b""),
                case(10, bytes(192)),
            ]
        )
        self.assertEqual(result[0][1], list(words(4096, BLS_N)))
        self.assertEqual(result[1][1], result[0][1])

    def test_kzg_nonconstant_polynomial_uses_the_setup(self):
        # Ethereum ceremony monomial G1[1], from c-kzg's trusted_setup.txt.
        commitment = bytes.fromhex(
            "ad3eb50121139aa34db1d545093ac9374ab7bca2c0f3bf28e27c8dcd8fc7cb42d25926fc0c97b336e9f0fb35e5a04c81"
        )
        proof = (G1[0] | (1 << 383) | (int(G1[1] > BLS_P // 2) << 381)).to_bytes(
            48, "big"
        )
        data = (
            b"\x01"
            + hashlib.sha256(commitment).digest()[1:]
            + words(1, 1)
            + commitment
            + proof
        )
        wrong = data[:64] + words(2) + data[96:]
        result = self.compare([case(10, data), case(10, wrong)])
        self.assertEqual(result[0][1], list(words(4096, BLS_N)))
        self.assertEqual(result[1][0], "exception")

    def test_blake2_compression_and_encoding(self):
        iv = [
            0x6A09E667F3BCC908,
            0xBB67AE8584CAA73B,
            0x3C6EF372FE94F82B,
            0xA54FF53A5F1D36F1,
            0x510E527FADE682D1,
            0x9B05688C2B3E6C1F,
            0x1F83D9ABFB41BD6B,
            0x5BE0CD19137E2179,
        ]
        h = [iv[0] ^ 0x01010040, *iv[1:]]
        data = (
            (12).to_bytes(4, "big")
            + b"".join(v.to_bytes(8, "little") for v in h)
            + b"abc"
            + bytes(125)
            + (3).to_bytes(8, "little")
            + bytes(8)
            + b"\x01"
        )
        result = self.compare(
            [
                case(9, data, 12),
                case(9, data, 11),
                case(9, bytes(213), 0),
                case(9, data[:-1] + b"\x02"),
                case(9, b"\xff", 0),
            ]
        )
        self.assertEqual(bytes(result[0][1]), hashlib.blake2b(b"abc").digest())

    def test_nontrivial_pairings_and_cancellation(self):
        bn_g2 = words(
            11559732032986387107991004021392285783925812861821192530917403151452391805634,
            10857046999023057135944570762232829481370756359578518086990519993285655852781,
            4082367875863433681332203403145435568316851327593401208105741076214120093531,
            8495653923123431417604973247489272438418190587263600148770280649306958101930,
        )
        bn = words(1, 2) + bn_g2
        bls = words(*G1, width=64) + words(*G2, width=64)
        result = self.compare(
            [
                case(8, bn),
                case(8, bn + words(1, BN_P - 2) + bn_g2),
                case(15, bls, fork="prague"),
                case(
                    15,
                    bls + words(G1[0], BLS_P - G1[1], width=64) + words(*G2, width=64),
                    fork="prague",
                ),
            ]
        )
        self.assertEqual([r[1][-1] for r in result], [0, 1, 0, 1])

    def test_bls_bilinearity_with_independent_multiples(self):
        g1, g2 = words(*G1, width=64), words(*G2, width=64)
        native = json.loads(
            subprocess.check_output(
                [REPLAY, "--precompiles"],
                input=json.dumps(
                    {
                        "version": 1,
                        "cases": [
                            case(12, g1 + words(2), fork="prague"),
                            case(14, g2 + words(2), fork="prague"),
                        ],
                    }
                ),
                text=True,
            )
        )
        g12, g22 = [bytes(entry[1]) for entry in native["observations"]]
        pair = g12 + g2 + words(G1[0], BLS_P - G1[1], width=64) + g22
        result = self.compare([case(15, pair, fork="prague")])
        self.assertEqual(result[0][1], [0] * 31 + [1])

    def test_activation_and_gas_boundaries_on_all_forks(self):
        forks = (
            "istanbul",
            "berlin",
            "london",
            "paris",
            "shanghai",
            "cancun",
            "prague",
            "osaka",
        )
        cases = []
        for fork in forks:
            for address, data, cost in [
                (1, b"", 3000),
                (2, b"", 60),
                (3, b"", 600),
                (4, b"", 15),
                (5, b"", 0 if fork == "istanbul" else 500 if fork == "osaka" else 200),
                (6, b"", 150),
                (7, b"", 6000),
                (8, b"", 45000),
                (9, bytes(213), 0),
                (10, b"", 50000),
                (11, bytes(256), 375),
                (12, bytes(160), 12000),
                (13, bytes(512), 600),
                (14, bytes(288), 22500),
                (15, bytes(384), 70300),
                (16, bytes(64), 5500),
                (17, bytes(128), 23800),
                (256, b"", 6900),
            ]:
                cases.append(case(address, data, cost, fork))
                if cost:
                    cases.append(case(address, data, cost - 1, fork))
        self.compare(cases)

    def test_msm_discount_saturation_and_modexp_edge_encodings(self):
        cases = [
            case(address, bytes(width * k), 3_000_000, "prague")
            for address, width in [(12, 160), (14, 288)]
            for k in (1, 2, 128, 129)
        ]
        cases += [
            case(5, words(0, 0, 0)),
            case(5, words(0, 2**256 - 1, 0), fork="istanbul"),
            case(5, words(0, 0, 1025), 0, "osaka"),
            case(5, words(1, 1, 2) + b"\x03\x05\x00\x07"),
            case(5, words(1, 1, 1) + b"\x03\x05\x00"),
        ]
        self.compare(cases)


class PrecompileEntryTests(unittest.TestCase):
    pair = world.WorldTests.pair

    @staticmethod
    def invoke(address, gas=300000, output=32):
        # Copy fixture calldata, call, then return flag followed by exact returndata.
        return f"calldatasize\npush 0\npush 0\ncalldatacopy\npush {output}\npush 32\ncalldatasize\npush 0\npush 0\npush {address}\npush {gas}\ncall\npush 0\nmstore\nreturndatasize\npush 32\nadd\npush 0\nreturn"

    @staticmethod
    def result(data, success=True):
        payload = words(int(success)) + data
        body = []
        for i in range(0, len(payload), 32):
            body.extend(
                [
                    f"push {int.from_bytes(payload[i : i + 32].ljust(32, bytes(1)), 'big')}",
                    f"push {i}",
                    "mstore",
                ]
            )
        return "\n".join([*body, f"push {len(payload)}", "push 0", "return"])

    def test_sha_and_curve_calls_through_both_ir_clis(self):
        fixture = {"calldata": "0x616263"}
        self.pair(
            self.invoke(2),
            self.result(hashlib.sha256(b"abc").digest()),
            fixture=fixture,
        )
        _, record = self.pair(
            self.invoke(2), self.result(bytes(32)), fixture=fixture, status=1
        )
        self.assertEqual(
            record["witness"]["observations"][0][1],
            list(words(1) + hashlib.sha256(b"abc").digest()),
        )
        mir = "v0 = call 300000, 2, 0, 0, 0, 0, 32\nreturndata 0, 32"
        expected = int.from_bytes(hashlib.sha256(b"").digest(), "big")
        self.pair(mir, f"mstore 0, {expected}\nreturndata 0, 32", layer="mir")
        self.pair(self.invoke(6, output=64), self.result(bytes(64)))
        self.pair(
            self.invoke(11, output=128),
            self.result(bytes(128)),
            fixture={"calldata": "0x" + bytes(256).hex()},
            version="prague",
        )

    def test_fork_activation_runs_account_code_before_prague(self):
        fixture = {
            "accounts": [
                {
                    "address": 11,
                    "nonce": 1,
                    "code": world.bytecode("push 7\npush 0\nsstore\nstop"),
                }
            ]
        }
        before = world.call(11) + "\npop\nstop"
        self.pair(before, "stop", fixture=fixture, version="cancun", status=1)
        self.pair(before, "stop", fixture=fixture, version="prague")

    def test_precompile_work_limit_stays_inconclusive(self):
        data = (5000).to_bytes(4, "big") + bytes(209)
        _, record = self.pair(
            self.invoke(9),
            self.invoke(9),
            fixture={"calldata": "0x" + data.hex()},
            status=2,
        )
        self.assertEqual(record["status"], "unsupported")
        self.assertIn("model limit", record["diagnostic"])

    def test_root_is_a_bytecode_account(self):
        result, record = self.pair("stop", "stop", fixture={"address": 2}, status=2)
        self.assertEqual(record["status"], "unsupported")
        self.assertIn("bytecode account", result.stderr)
