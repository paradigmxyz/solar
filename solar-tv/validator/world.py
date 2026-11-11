"""Explicit initial-world fixtures and compiled-entry import.

This mode proves execution under a named world, not arbitrary-account equivalence.
Storage cells and entry stack words may be symbolic. The backend is an additional
trusted lowering boundary; its actual runtime image is hashed in the audit and
independently rebuilt by the native replay helper.
"""

import hashlib
import json
import re
import subprocess
from pathlib import Path

from . import native
from .common import Program, Unsupported

FORKS = {
    "istanbul",
    "berlin",
    "london",
    "paris",
    "shanghai",
    "cancun",
    "prague",
    "osaka",
}
ROOT = Path(__file__).resolve().parents[1]


def _keys(value: dict, allowed: set[str], context: str) -> None:
    if not isinstance(value, dict) or set(value) - allowed:
        raise Unsupported(f"unknown or malformed {context} fields")


def _number(value, bits=256) -> str:
    if type(value) is int:
        number = value
    elif isinstance(value, str) and re.fullmatch(r"0x[0-9a-fA-F]+|[0-9]+", value):
        number = int(value, 16 if value.startswith("0x") else 10)
    else:
        raise Unsupported("world numbers must be unsigned integers")
    if not 0 <= number < 2**bits:
        raise Unsupported(f"world number exceeds {bits} bits")
    return str(number)


def _bytes(value) -> list[int]:
    if not isinstance(value, str) or not re.fullmatch(r"0x(?:[0-9a-fA-F]{2})*", value):
        raise Unsupported("world byte strings require even-length 0x hex")
    result = list(bytes.fromhex(value[2:]))
    if len(result) > 65536:
        raise Unsupported("world byte string exceeds 64 KiB")
    return result


def read(path: Path, compiler: str, policy: str | None) -> dict:
    if path.stat().st_size > 262144:
        raise Unsupported("world fixture exceeds 256 KiB")
    try:
        value = json.loads(path.read_text())
    except (ValueError, RecursionError) as error:
        raise Unsupported("invalid world fixture JSON") from error
    _keys(
        value,
        {
            "version",
            "address",
            "caller",
            "gas",
            "static",
            "accounts",
            "calldata",
            "warm_slots",
            "fuel",
        },
        "world",
    )
    if type(value.get("version")) is not int or value["version"] != 1:
        raise Unsupported("world fixture requires version 1")
    symbols = {}

    def word(value):
        if isinstance(value, str) and value.startswith("$"):
            if not re.fullmatch(r"\$[a-zA-Z_][a-zA-Z0-9_]*", value):
                raise Unsupported("invalid symbolic world cell")
            if value not in symbols:
                symbols[value] = f"state{len(symbols)}"
            return "$" + symbols[value]
        return _number(value)

    address = _number(value.get("address", 65536), 160)
    caller = _number(value.get("caller", 131072), 160)
    budget = int(_number(value.get("gas", 1_000_000), 64))
    fuel = int(_number(value.get("fuel", 512), 32))
    if budget > 30_000_000 or not 1 <= fuel <= 2048:
        raise Unsupported("world limits: gas <= 30000000, fuel 1..2048")
    if type(value.get("static", False)) is not bool:
        raise Unsupported("world static permission must be boolean")
    accounts = []
    seen = set()
    balance = 0
    source = value.get("accounts", [])
    if not isinstance(source, list) or len(source) > 32:
        raise Unsupported("world supports up to 32 initial accounts")
    for entry in source:
        _keys(entry, {"address", "nonce", "balance", "code", "storage"}, "account")
        owner = _number(entry.get("address"), 160)
        if owner in seen:
            raise Unsupported("duplicate world account")
        seen.add(owner)
        slots = entry.get("storage", {})
        if not isinstance(slots, dict) or len(slots) > 64:
            raise Unsupported("account supports up to 64 initial storage cells")
        account = {
            "address": owner,
            "nonce": int(_number(entry.get("nonce", 1 if owner == address else 0), 64)),
            "balance": _number(entry.get("balance", 0)),
            "code": _bytes(entry.get("code", "0x")),
            "storage": [(word(key), word(data)) for key, data in slots.items()],
        }
        if (
            owner != address
            and account["nonce"] == 0
            and account["balance"] == "0"
            and not account["code"]
        ):
            raise Unsupported(
                "initial empty accounts are not valid post-Istanbul state"
            )
        if owner == address and account["code"]:
            raise Unsupported("root code comes from the IR, not the world fixture")
        balance += int(account["balance"])
        accounts.append(account)
    if address not in seen:
        accounts.append(
            {"address": address, "nonce": 1, "balance": "0", "code": [], "storage": []}
        )
    if balance >= 2**256:
        raise Unsupported("initial balances exceed a word-sized total supply")
    warm = value.get("warm_slots", [])
    if (
        not isinstance(warm, list)
        or len(warm) > 64
        or any(not isinstance(pair, list) or len(pair) != 2 for pair in warm)
    ):
        raise Unsupported("warm_slots requires up to 64 [address, slot] pairs")
    result = {
        "compiler": compiler,
        "address": address,
        "caller": caller,
        "gas": budget,
        "static": value.get("static", False),
        "calldata": _bytes(value.get("calldata", "0x")),
        "accounts": accounts,
        "warm_slots": [(_number(a, 160), word(k)) for a, k in warm],
        "gas_policy": policy,
        "fuel": fuel,
    }
    digest = hashlib.sha256(json.dumps(result, sort_keys=True).encode()).hexdigest()
    return {
        **result,
        "fixture_sha256": digest,
        "path": str(path.resolve()),
        "fuel": fuel,
        "symbols": list(symbols.values()),
    }


def runtime(text: str, kind: str, solar: str, version: str, timeout: int) -> bytes:
    import tempfile

    with tempfile.TemporaryDirectory(prefix="solar-tv-world-") as directory:
        path = Path(directory) / f"entry.{kind}"
        path.write_text(text)
        command = [
            solar,
            path,
            "-Onone",
            "--emit=bin-runtime",
            "--allow=2264",
            f"--evm-version={version}",
        ]
        if kind == "evmir":
            command.append("-Zevm-ir-pipeline=none")
        result = subprocess.run(
            command, text=True, capture_output=True, check=False, timeout=timeout
        )
    if result.returncode:
        raise Unsupported("cannot compile explicit-world entry\n" + result.stderr)
    try:
        # MIR inputs also print the final pipeline snapshot before the artifact.
        artifacts = json.loads(result.stdout.splitlines()[-1])["contracts"]
        if len(artifacts) != 1:
            raise ValueError("expected exactly one runtime")
        code = bytes.fromhex(next(iter(artifacts.values()))["bin-runtime"])
    except (ValueError, KeyError, TypeError) as error:
        raise Unsupported("invalid explicit-world compiler image") from error
    if len(code) > 24576:
        raise Unsupported("world runtime exceeds EIP-170")
    return code


def configuration(solar: str, version: str, root: str, timeout: int) -> str:
    catalog = native.catalog(native.default(solar), timeout)
    precompiles = catalog.get("precompiles")
    if not isinstance(precompiles, dict):
        raise Unsupported("rebuild the native replay helper for precompile activation")
    if root in precompiles.get(version, []):
        raise Unsupported(
            "world root must be a bytecode account, not an active precompile"
        )
    definitions = {op["opcode"]: op for op in catalog["evmir"]}
    rows = []
    for opcode in range(256):
        if opcode not in definitions:
            rows.append("⟨false, 0, 0, 0, 0⟩")
            continue
        entry = definitions[opcode]
        fork = entry["forks"][version]
        inputs, outputs = entry["stack"] or [0, 0]
        rows.append(
            f"⟨{'true' if fork['available'] else 'false'}, {fork['base_gas']}, {fork['dynamic_gas']}, {inputs}, {outputs}⟩"
        )
    return f"def tvWorldConfig : EVM.State.Config := ⟨.{version}, [{', '.join(rows)}], {root}⟩\n"


def translate(
    text: str,
    kind: str,
    solar: str,
    version: str,
    fixture: dict,
    timeout: int,
    function: str | None = None,
) -> Program:
    if version not in FORKS:
        raise Unsupported(
            "explicit-world execution currently supports Istanbul through Osaka"
        )
    if kind == "mir":
        entries = re.findall(
            r"^fn @([^ (]+)\(\)(?: ->[^\n]+?)? \[entry(?:, [^\n]*)?\] \{",
            text,
            re.MULTILINE,
        )
        if len(entries) != 1 or (function and function != entries[0]):
            raise Unsupported(
                "world MIR requires an actual dispatch entry, not an isolated internal function"
            )
        height = 0
    else:
        if function is not None:
            raise Unsupported("--function requires MIR")
        match = re.search(r"^bb\d+(?: \[([^\]]+)\])?:", text, re.MULTILINE)
        if match is None:
            raise Unsupported("world EVM IR requires an entry block")
        stack = re.search(r"\bstack=(\d+)", match[1] or "")
        height = int(stack[1]) if stack else 0
        if height > 1024:
            raise Unsupported("world entry stack exceeds 1024 words")
    code = runtime(text, kind, solar, version, timeout)
    config = configuration(solar, version, fixture["address"], timeout)
    stack = [f"stack{i}" for i in range(height)]
    inputs = [*fixture["symbols"], *stack]

    def word(value):
        return value.removeprefix("$")

    accounts = []
    for account in fixture["accounts"]:
        body = (
            "["
            + ", ".join(
                f"({word(key)}, {word(value)})" for key, value in account["storage"]
            )
            + "]"
        )
        slots = f"(({body} : EVM.State.Slots).foldl (fun slots entry => EVM.State.slotsWrite slots entry.1 entry.2) [])"
        image = (
            list(code) if account["address"] == fixture["address"] else account["code"]
        )
        accounts.append(
            f"⟨{account['address']}, {account['nonce']}, {account['balance']}, {image}, {slots}, [], false, false⟩"
        )
    warm = [fixture["address"], fixture["caller"], *map(str, range(1, 10))]
    if version in {"cancun", "prague", "osaka"}:
        warm.append("10")
    if version in {"prague", "osaka"}:
        warm.extend(map(str, range(11, 18)))
    if version == "osaka":
        warm.append("256")
    if version in {"shanghai", "cancun", "prague", "osaka"}:
        warm.append("0")
    warm_slots = (
        "["
        + ", ".join(
            f"({address}, {word(slot)})" for address, slot in fixture["warm_slots"]
        )
        + "]"
    )
    body = (
        f"let accounts : List EVM.State.Account := [{', '.join(accounts)}]\n"
        f"let world : EVM.State.Journal := ⟨accounts, accounts, [{', '.join(warm)}], {warm_slots}, [], 0⟩\n"
        f"let environment : World.Environment := ⟨{fixture['address']}, {fixture['address']}, {fixture['caller']}, {fixture['caller']}, 0, 0, {fixture['calldata']}, {'true' if fixture['static'] else 'false'}, 0⟩\n"
        f"EVM.State.execute {fixture['fuel']} tvWorldConfig world ⟨environment, {list(code)}, 0, [{', '.join(stack)}], [], [], {fixture['gas']}⟩"
    )
    return Program(
        ("world-entry", kind, str(height), fixture["fixture_sha256"]),
        inputs,
        "EVM.State.Run",
        body,
        (256,) * len(inputs),
        canonical_ir=text,
        gas=fixture["gas_policy"],
        evm_version=version,
        scope="world-entry",
        world={
            **fixture,
            "configuration": config,
            "image_sha256": hashlib.sha256(code).hexdigest(),
        },
    )


def replay_input(fixture: dict) -> dict:
    keys = {
        "compiler",
        "address",
        "caller",
        "gas",
        "static",
        "calldata",
        "accounts",
        "warm_slots",
        "gas_policy",
    }
    return {key: value for key, value in fixture.items() if key in keys}
