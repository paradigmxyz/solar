"""Explicit initial-world fixtures and shared Lean constructor rendering.

MIR uses direct typed SSA semantics and its independent Rust interpreter.
EVM entries retain their explicit compiled-image boundary and native EVM replay.
Both modes prove only the stated world, not arbitrary-account equivalence.
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
    "amsterdam",
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


def read(path: Path, compiler: str, policy: str | None, *, kind: str = "evmir") -> dict:
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
            "block",
            "chain_id",
            "blob_hashes",
            "origin",
            "gas_price",
            "warm_addresses",
            "memory",
            "creation_gas",
            "builtin_gas",
            "debug_reverts",
            "mir_layout",
        },
        "world",
    )
    if type(value.get("version")) is not int or value["version"] != 1:
        raise Unsupported("world fixture requires version 1")
    if kind == "mir" and (policy is not None or "gas" in value):
        raise Unsupported("direct MIR state validation does not assign EVM gas costs")
    if kind != "mir" and "memory" in value:
        raise Unsupported("initial memory is an explicit MIR boundary input")
    initial_memory = value.get("memory", {})
    _keys(initial_memory, {"data", "size"}, "memory")
    memory_data = _bytes(initial_memory.get("data", "0x"))
    memory_size = int(
        _number(initial_memory.get("size", (len(memory_data) + 31) // 32 * 32), 32)
    )
    if memory_size % 32 or memory_size > 65536 or len(memory_data) > memory_size:
        raise Unsupported("MIR memory must be word-aligned and at most 64 KiB")
    creation_gas = value.get("creation_gas", [])
    if not isinstance(creation_gas, list) or len(creation_gas) > 128:
        raise Unsupported("creation_gas requires up to 128 child budgets")
    creation_gas = [int(_number(gas, 64)) for gas in creation_gas]
    if any(gas > 30_000_000 for gas in creation_gas):
        raise Unsupported("creation child gas exceeds 30000000")
    if kind != "mir" and creation_gas:
        raise Unsupported("creation_gas is an explicit MIR boundary input")
    builtin_gas = value.get("builtin_gas", [])
    if not isinstance(builtin_gas, list) or len(builtin_gas) > 128:
        raise Unsupported("builtin_gas requires up to 128 child budgets")
    builtin_gas = [int(_number(gas, 64)) for gas in builtin_gas]
    if any(gas > 30_000_000 for gas in builtin_gas):
        raise Unsupported("builtin child gas exceeds 30000000")
    if kind != "mir" and builtin_gas:
        raise Unsupported("builtin_gas is an explicit MIR boundary input")
    debug_reverts = value.get("debug_reverts", False)
    if type(debug_reverts) is not bool or kind != "mir" and debug_reverts:
        raise Unsupported("debug_reverts requires a direct MIR boolean setting")
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
    context = _block(value.get("block", {}))
    blobs = value.get("blob_hashes", [])
    addresses = value.get("warm_addresses", [])
    if not isinstance(blobs, list) or len(blobs) > 64:
        raise Unsupported("world supports up to 64 blob hashes")
    if not isinstance(addresses, list) or len(addresses) > 64:
        raise Unsupported("world supports up to 64 warm addresses")
    budget = None if kind == "mir" else int(_number(value.get("gas", 1_000_000), 64))
    fuel = int(_number(value.get("fuel", 512), 32))
    if (budget is not None and budget > 30_000_000) or not 1 <= fuel <= 2048:
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
        _keys(
            entry,
            {"address", "nonce", "balance", "code", "storage", "transient"},
            "account",
        )
        owner = _number(entry.get("address"), 160)
        if owner in seen:
            raise Unsupported("duplicate world account")
        seen.add(owner)
        slots = entry.get("storage", {})
        if not isinstance(slots, dict) or len(slots) > 64:
            raise Unsupported("account supports up to 64 initial storage cells")
        transient = entry.get("transient", {})
        if not isinstance(transient, dict) or len(transient) > 64:
            raise Unsupported("account supports up to 64 transient cells")
        if kind != "mir" and transient:
            raise Unsupported("EVM transactions start with empty transient storage")
        account = {
            "address": owner,
            "nonce": int(_number(entry.get("nonce", 1 if owner == address else 0), 64)),
            "balance": _number(entry.get("balance", 0)),
            "code": _bytes(entry.get("code", "0x")),
            "storage": [(word(key), word(data)) for key, data in slots.items()],
            "transient": [(word(key), word(data)) for key, data in transient.items()],
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
            {
                "address": address,
                "nonce": 1,
                "balance": "0",
                "code": [],
                "storage": [],
                "transient": [],
            }
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
        "origin": _number(value.get("origin", caller), 160),
        "gas_price": _number(value.get("gas_price", 0)),
        "chain_id": _number(value.get("chain_id", 1)),
        "blob_hashes": [_number(blob) for blob in blobs],
        "warm_addresses": [_number(a, 160) for a in addresses],
        "block": context,
        "gas": budget,
        "static": value.get("static", False),
        "calldata": _bytes(value.get("calldata", "0x")),
        "accounts": accounts,
        "warm_slots": [(_number(a, 160), word(k)) for a, k in warm],
        "gas_policy": policy,
        "fuel": fuel,
        "memory": {"data": memory_data, "size": memory_size},
        "creation_gas": creation_gas,
        "builtin_gas": builtin_gas,
        "debug_reverts": debug_reverts,
    }
    if "mir_layout" in value:
        if kind != "mir":
            raise Unsupported("mir_layout requires direct MIR execution")
        from .mir_layout import read as read_layout

        result["mir_layout"] = read_layout(value["mir_layout"], result["memory"])
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
        artifacts = json.loads(result.stdout.splitlines()[-1])["contracts"]
        if len(artifacts) != 1:
            raise ValueError("expected exactly one runtime")
        code = bytes.fromhex(next(iter(artifacts.values()))["bin-runtime"])
    except (ValueError, KeyError, TypeError) as error:
        raise Unsupported("invalid explicit-world compiler image") from error
    if len(code) > 24576:
        raise Unsupported("world runtime exceeds EIP-170")
    return code


def configuration(
    solar: str,
    version: str,
    fixture: dict,
    timeout: int,
    replay: str | None = None,
    *,
    root_is_mir: bool = False,
) -> str:
    catalog = native.catalog(replay or native.default(solar), timeout)
    precompiles = catalog.get("precompiles")
    if not isinstance(precompiles, dict):
        raise Unsupported("rebuild the native replay helper for precompile activation")
    if not root_is_mir and fixture["address"] in precompiles.get(version, []):
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
    context = context_expression(fixture)
    unknown = f"some {fixture['address']}" if root_is_mir else "none"
    return f"def tvWorldConfig : EVM.State.Config := ⟨.{version}, [{', '.join(rows)}], {fixture['address']}, {context}, {unknown}⟩\n"


def context_expression(fixture):
    block = fixture["block"]
    hashes = "[" + ", ".join(f"({n},{h})" for n, h in block["hashes"]) + "]"
    blobs = "[" + ", ".join(fixture["blob_hashes"]) + "]"
    return (
        f"⟨{block['number']},{block['coinbase']},{block['timestamp']},"
        f"{block['gas_limit']},{block['difficulty']},{block['prevrandao']},"
        f"{fixture['chain_id']},{block['base_fee']},{block['blob_base_fee']},"
        f"{block['slot']},{hashes},{blobs}⟩"
    )


def translate(
    text: str,
    kind: str,
    solar: str,
    version: str,
    fixture: dict,
    timeout: int,
    function: str | None = None,
    replay: str | None = None,
) -> Program:
    if version not in FORKS:
        raise Unsupported(
            "explicit-world execution supports Istanbul through Osaka, and Amsterdam's environment fragment"
        )
    if kind == "mir":
        from . import mir_state

        return mir_state.translate(
            text, solar, version, fixture, timeout, function, replay
        )
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
    config = configuration(solar, version, fixture, timeout, replay)
    stack = [f"stack{i}" for i in range(height)]
    inputs = [*fixture["symbols"], *stack]

    def word(value):
        return value.removeprefix("$")

    warm = warm_addresses(fixture, version)
    warm_slots = (
        "["
        + ", ".join(
            f"({address}, {word(slot)})" for address, slot in fixture["warm_slots"]
        )
        + "]"
    )
    body = (
        f"let accounts : List EVM.State.Account := {accounts_expression(fixture, code)}\n"
        f"let world : EVM.State.Journal := ⟨accounts, accounts, [{', '.join(warm)}], {warm_slots}, [], 0⟩\n"
        f"let environment : World.Environment := {environment_expression(fixture)}\n"
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
        "block",
        "origin",
        "gas_price",
        "chain_id",
        "blob_hashes",
        "warm_addresses",
        "memory",
        "creation_gas",
        "builtin_gas",
        "debug_reverts",
        "mir_layout",
    }
    return {key: value for key, value in fixture.items() if key in keys}


def accounts_expression(fixture, image=None):
    def slots(values):
        pairs = ",".join(
            f"({key.removeprefix('$')},{value.removeprefix('$')})"
            for key, value in values
        )
        return f"(([{pairs}] : Execution.Slots).foldl (fun slots entry => Execution.slotsWrite slots entry.1 entry.2) [])"

    accounts = []
    for account in fixture["accounts"]:
        code = (
            list(image)
            if image is not None and account["address"] == fixture["address"]
            else account["code"]
        )
        accounts.append(
            f"⟨{account['address']},{account['nonce']},{account['balance']},{code},{slots(account['storage'])},{slots(account['transient'])},false,false⟩"
        )
    return "[" + ",".join(accounts) + "]"


def environment_expression(fixture):
    return f"⟨{fixture['address']},{fixture['address']},{fixture['caller']},{fixture['origin']},0,{fixture['gas_price']},{fixture['calldata']},{'true' if fixture['static'] else 'false'},0⟩"


def _block(value: dict) -> dict:
    defaults = {
        "number": 0,
        "coinbase": 0,
        "timestamp": 1,
        "gas_limit": 2**64 - 1,
        "difficulty": 0,
        "prevrandao": 0,
        "base_fee": 0,
        "blob_base_fee": 1,
        "slot": 0,
    }
    _keys(value, {*defaults, "hashes"}, "block")
    result = {
        key: _number(
            value.get(key, default),
            160 if key == "coinbase" else 64 if key == "slot" else 256,
        )
        for key, default in defaults.items()
    }
    hashes = value.get("hashes", {})
    if not isinstance(hashes, dict) or len(hashes) > 256:
        raise Unsupported("block supports up to 256 history hashes")
    history = [(_number(n), _number(h)) for n, h in hashes.items()]
    if len({n for n, _ in history}) != len(history):
        raise Unsupported("duplicate block history number")
    return {**result, "hashes": history}


def warm_addresses(fixture, version):
    warm = [
        fixture["address"],
        fixture["caller"],
        *fixture["warm_addresses"],
        *map(str, range(1, 10)),
    ]
    if version in {"cancun", "prague", "osaka", "amsterdam"}:
        warm.append("10")
    if version in {"prague", "osaka", "amsterdam"}:
        warm.extend(map(str, range(11, 18)))
    if version in {"osaka", "amsterdam"}:
        warm.append("256")
    if version in {"shanghai", "cancun", "prague", "osaka", "amsterdam"}:
        warm.append(fixture["block"]["coinbase"])
    return warm
