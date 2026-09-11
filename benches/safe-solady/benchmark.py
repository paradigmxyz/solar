#!/usr/bin/env python3
# /// script
# requires-python = ">=3.11"
# dependencies = ["eth-abi==5.2.0", "eth-hash[pycryptodome]==0.7.1"]
# ///
"""Compare checked Solidity with pinned upstream Solady on identical ABI calls.

Every implementation is compiled by both compilers. solc runs through both code
paths; the primary comparison uses its cheaper upstream result for each call.
All measured cases must match a separate Python oracle, including revert bytes.
"""

from __future__ import annotations

import argparse
import base64
import gzip
import hashlib
import json
import posixpath
import random
import re
import socket
import subprocess
import time
import urllib.request
from pathlib import Path

from eth_abi import encode
from eth_hash.auto import keccak

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parents[1]
ARCHIVE = REPO / "testdata/projects/solady-0.1.26.json.gz"
MAX = (1 << 256) - 1


def mask(n):
    return (1 << n) - 1


def digest(data):
    return hashlib.sha256(data).hexdigest()


def compile_json(binary, payload):
    process = subprocess.run(
        [str(binary), "--standard-json"],
        input=json.dumps(payload),
        capture_output=True,
        text=True,
        timeout=180,
        check=False,
    )
    if process.returncode:
        raise RuntimeError(process.stderr or process.stdout)
    output = json.loads(process.stdout)
    errors = [x for x in output.get("errors", []) if x.get("severity") == "error"]
    if errors:
        raise RuntimeError("\n".join(x.get("formattedMessage", str(x)) for x in errors))
    return output


def walk(node):
    if isinstance(node, dict):
        yield node
        for value in node.values():
            yield from walk(value)
    elif isinstance(node, list):
        for value in node:
            yield from walk(value)


def safety_violations(output):
    return [
        (path, node["nodeType"])
        for path, source in output["sources"].items()
        for node in walk(source["ast"])
        if node.get("nodeType") in {"InlineAssembly", "UncheckedBlock"}
    ]


def type_name(parameter):
    return re.sub(
        r" (memory|calldata|storage)( (ref|pointer))?$",
        "",
        parameter["typeDescriptions"]["typeString"],
    )


def api_rows(ast):
    rows = []
    for contract in ast.get("nodes", []):
        if contract.get("nodeType") != "ContractDefinition":
            continue
        for fn in contract["nodes"]:
            if (
                fn.get("nodeType") != "FunctionDefinition"
                or fn["visibility"] == "private"
            ):
                continue
            params = [
                (type_name(p), p["storageLocation"])
                for p in fn["parameters"]["parameters"]
            ]
            returns = [
                (type_name(p), p["storageLocation"])
                for p in fn["returnParameters"]["parameters"]
            ]
            rows.append(
                {
                    "library": contract["name"],
                    "name": fn["name"],
                    "params": params,
                    "parameter_names": [
                        p["name"] for p in fn["parameters"]["parameters"]
                    ],
                    "returns": returns,
                    "visibility": fn["visibility"],
                    "mutability": fn["stateMutability"],
                    "signature": fn["name"]
                    + "("
                    + ",".join(p[0] for p in params)
                    + ")",
                }
            )
    return rows


def closure(sources, roots):
    result = {}
    pending = list(roots)
    while pending:
        path = pending.pop()
        if path in result:
            continue
        result[path] = sources[path]
        for imp in re.findall(
            r'\bimport\s+(?:[^;]*?from\s+)?["\']([^"\']+)["\']',
            sources[path]["content"],
        ):
            pending.append(
                posixpath.normpath(posixpath.join(posixpath.dirname(path), imp))
                if imp.startswith(".")
                else imp
            )
    return result


def prepare(solc, out):
    archive = json.loads(gzip.decompress(ARCHIVE.read_bytes()))
    safe = {
        p.relative_to(ROOT).as_posix(): {"content": p.read_text()}
        for p in sorted((ROOT / "src").rglob("*.sol"))
    }
    ast_settings = {"outputSelection": {"*": {"": ["ast"]}}}
    safe_ast = compile_json(
        solc, {"language": "Solidity", "sources": safe, "settings": ast_settings}
    )
    violations = safety_violations(safe_ast)
    if violations:
        raise ValueError(f"safe source audit failed: {violations}")
    # Parse the whole pinned archive for honest API coverage, without generating bytecode.
    original_ast = compile_json(
        solc,
        {
            "language": "Solidity",
            "sources": archive["sources"],
            "settings": ast_settings,
        },
    )
    inventory = []
    apis = []
    for path, v in original_ast["sources"].items():
        if not path.startswith("src/"):
            continue
        upstream = api_rows(v["ast"])
        rewritten = api_rows(safe_ast["sources"][path]["ast"]) if path in safe else []
        upstream_by_sig = {r["signature"]: r for r in upstream}
        for row in rewritten:
            if row != upstream_by_sig.get(row["signature"]):
                raise ValueError(f"API declaration mismatch: {path}: {row}")
            apis.append(row)
        inventory.append(
            {
                "path": path,
                "upstream_apis": len(upstream),
                "rewritten_apis": len(rewritten),
                "missing": [r["signature"] for r in upstream if r not in rewritten],
            }
        )
    apis.sort(key=lambda x: (x["library"], x["signature"]))
    harness = "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.20;\n"
    for path in safe:
        harness += f'import "{path}";\n'
    for library in sorted({r["library"] for r in apis}):
        harness += f"contract {library}Harness {{\n"
        for index, row in enumerate(apis):
            if row["library"] != library:
                continue
            row["wrapper"] = f"f{index}"
            params = ", ".join(
                t + ("" if loc == "default" else " " + loc) + f" a{i}"
                for i, (t, loc) in enumerate(row["params"])
            )
            returns = list(row["returns"])
            # Observe in-place updates through the identical wrapper in both sources.
            if not returns:
                returns = [row["params"][0]]
            row["outputs"] = [t for t, _ in returns]
            result_types = ", ".join(
                t + ("" if loc == "default" else " " + loc) for t, loc in returns
            )
            call = (
                row["library"]
                + "."
                + row["name"]
                + "("
                + ", ".join(f"a{i}" for i in range(len(row["params"])))
                + ")"
            )
            body = f"return {call};" if row["returns"] else f"{call}; return a0;"
            harness += f"function {row['wrapper']}({params}) external pure returns ({result_types}) {{ {body} }}\n"
        harness += "}\n"
    safe["Harness.sol"] = {"content": harness}
    upstream = closure(archive["sources"], safe.keys() - {"Harness.sol"})
    upstream["Harness.sol"] = {"content": harness}
    (out / "Harness.sol").write_text(harness)
    audit = {
        "archive": str(ARCHIVE.relative_to(REPO)),
        "archive_sha256": digest(ARCHIVE.read_bytes()),
        "policy": "no InlineAssembly or UncheckedBlock in any safe source or dependency",
        "source_sha256": {p: digest(v["content"].encode()) for p, v in safe.items()},
        "libraries": inventory,
        "implemented_apis": len(apis),
        "library_count": len(safe) - 1,
        "total_source_files": len(inventory),
    }
    (out / "api-coverage.json").write_text(json.dumps(audit, indent=2) + "\n")
    return safe, upstream, apis, audit


def test_vectors(row, rng):
    name = row["name"]
    types = [t for t, _ in row["params"]]
    scalars = [0, 1, 2, 3, 255, 256, 257, 1 << 128, (1 << 255) - 1, 1 << 255, MAX]
    scalars += [rng.getrandbits(256) for _ in range(8)]
    blobs = [
        bytes((i * 37 + 11) % 256 for i in range(n))
        for n in [0, 1, 2, 3, 15, 16, 31, 32, 33, 63, 64, 65, 256]
    ]
    blobs += [bytes(range(256)), bytes(64), bytes([255]) * 64]
    if row["library"] == "SafeCastLib":
        bits = int(re.search(r"\d+", name)[0])
        signed = name.startswith("toInt")
        low = -(1 << (bits - 1)) if signed else 0
        high = (1 << (bits - 1 if signed else bits)) - 1
        values = {low - 1, low, low + 1, high - 1, high, high + 1, -1, 0, 1, MAX}
        domain = (-(1 << 255), (1 << 255) - 1) if types[0] == "int256" else (0, MAX)
        for value in sorted(v for v in values if domain[0] <= v <= domain[1]):
            yield (
                [value],
                [value] if low <= value <= high else keccak(b"Overflow()")[:4],
            )
        return
    if row["library"] == "Base64":
        if name == "encode":
            for b in blobs:
                for mode in range(1 << (len(types) - 1)):
                    opts = [bool(mode & (1 << j)) for j in range(len(types) - 1)]
                    out = (
                        base64.urlsafe_b64encode
                        if opts and opts[0]
                        else base64.b64encode
                    )(b)
                    if len(opts) == 2 and opts[1]:
                        out = out.rstrip(b"=")
                    yield [b, *opts], [out.decode()]
        else:
            for b in blobs:
                for url in [False, True]:
                    encoded = (base64.urlsafe_b64encode if url else base64.b64encode)(
                        b
                    ).decode()
                    for v in sorted(
                        {encoded, encoded.rstrip("="), encoded.replace("/", ",")}
                    ):
                        yield [v], [b]
        return
    if row["library"] == "LibSort":
        t = types[0][:-2]
        arrays = []
        for n in [0, 1, 2, 15, 16, 17, 31, 32, 33, 64]:
            for shape in ["sorted", "reverse", "equal", "mixed"]:
                a = (
                    list(range(n))
                    if shape in ["sorted", "reverse"]
                    else [7] * n
                    if shape == "equal"
                    else [rng.randrange(0, 20) for _ in range(n)]
                )
                if shape == "reverse":
                    a.reverse()
                if t == "int256":
                    a = [x - 10 for x in a]
                arrays.append(a)
        low, high = (
            (-(1 << 255), (1 << 255) - 1)
            if t == "int256"
            else (0, mask(160) if t == "address" else MAX)
        )
        extremes = [low, high, low + 1, high - 1, 0, 1]
        arrays += [
            extremes,
            sorted(extremes),
            sorted(extremes, reverse=True),
            [high] * 33,
            [low, high] * 17,
            [high - i * (1 << 128) for i in range(33)],
        ]
        for a in arrays:
            if t == "address":
                a = ["0x" + x.to_bytes(20, "big").hex() for x in a]
            elif t == "bytes32":
                a = [x.to_bytes(32, "big") for x in a]
            if name in ["sort", "insertionSort"]:
                expected = sorted(a)
            elif name == "reverse":
                expected = list(reversed(a))
            elif name == "copy":
                expected = list(a)
            elif name == "hasDuplicate":
                expected = len(set(a)) != len(a)
            elif name == "isSortedAndUniquified":
                expected = all(a[i - 1] < a[i] for i in range(1, len(a)))
            else:
                expected = a == sorted(a)
            yield [a], [expected]
        return
    if row["library"] == "LibBit":
        if types == ["uint256"]:
            # Exercise every bit position, every population count and both
            # neighbours of powers of two, including the highest EVM bit.
            scalars += [
                v for bit in range(256) for v in [1 << bit, MAX ^ (1 << bit), mask(bit)]
            ]
            scalars = list(dict.fromkeys(scalars))
        if types == ["bool"] * len(types):
            for i in range(1 << len(types)):
                args = [bool(i & (1 << j)) for j in range(len(types))]
                expected = (
                    int(args[0])
                    if name in ["toUint", "rawToUint"]
                    else all(args)
                    if name in ["and", "rawAnd"]
                    else any(args)
                )
                yield args, [expected]
        elif types == ["bytes"]:
            for b in blobs + [bytes(33)]:
                yield (
                    [b],
                    [
                        bytes(y for x in b for y in (x >> 4, x & 15))
                        if name == "toNibbles"
                        else b.count(0)
                    ],
                )
        else:
            for x in scalars:
                if len(types) == 2:
                    for y in [x, 0, MAX, rng.getrandbits(256)]:
                        s = (x ^ y).bit_length()
                        width = (
                            1
                            if name == "commonBitPrefix"
                            else 4
                            if name == "commonNibblePrefix"
                            else 8
                        )
                        s = ((s + width - 1) // width) * width
                        yield [x, y], [(x >> s) << s]
                else:
                    result = {
                        "fls": x.bit_length() - 1 if x else 256,
                        "clz": 256 - x.bit_length(),
                        "ffs": (x & -x).bit_length() - 1 if x else 256,
                        "popCount": x.bit_count(),
                        "countZeroBytes": x.to_bytes(32, "big").count(0),
                        "isPo2": bool(x and not x & (x - 1)),
                        "reverseBytes": int.from_bytes(x.to_bytes(32, "big"), "little"),
                        "reverseBits": int(f"{x:0256b}"[::-1], 2),
                    }[name]
                    yield [x], [result]
        return
    if row["library"] == "LibString":
        if name == "toString":
            for x in scalars:
                if types[0] == "int256" and x >= 1 << 255:
                    x -= 1 << 256
                yield [x], [str(x)]
        elif name.startswith("to") and "HexString" in name:
            prefix = "" if "NoPrefix" in name else "0x"
            if types[0] == "bytes":
                for b in blobs:
                    yield [b], [prefix + b.hex()]
            else:
                for x in scalars:
                    if types[0] == "address":
                        x &= mask(160)
                    arg = (
                        "0x" + x.to_bytes(20, "big").hex()
                        if types[0] == "address"
                        else x
                    )
                    if len(types) == 2:
                        for n in [0, 1, 16, 20, 31, 32, 33]:
                            yield (
                                [arg, n],
                                [prefix + x.to_bytes(n, "big").hex()]
                                if x.bit_length() <= 8 * n
                                else keccak(b"HexLengthInsufficient()")[:4],
                            )
                    else:
                        h = format(x, "x")
                        if "Minimal" not in name:
                            h = h.zfill(
                                40 if types[0] == "address" else (len(h) + 1) // 2 * 2
                            )
                        yield [arg], [prefix + h]
        elif name == "runeCount":
            for s in ["", "hello", "é", "日本語", "🐱" * 33, "a" * 256]:
                yield [s], [len(s)]
        elif name in ["concat", "eq"]:
            for a, b in [("", ""), ("a", "b"), ("same", "same"), ("a" * 33, "b" * 64)]:
                yield [a, b], [a + b if name == "concat" else a == b]
        elif name in ["lower", "upper", "toCase"]:
            for s in ["", "Hello WORLD 0![]", "aBcD" * 16]:
                for upper in [False, True] if name == "toCase" else [name == "upper"]:
                    yield (
                        [s, upper] if name == "toCase" else [s],
                        [s.upper() if upper else s.lower()],
                    )
        else:
            for s in [
                "",
                "ASCII",
                "a" * 31,
                "a" * 32,
                "a" * 33,
                "a" * 256,
                "é",
                "a" * 32 + "é",
                "\0",
            ]:
                if name == "to7BitASCIIAllowedLookup":
                    yield (
                        [s],
                        [sum(1 << x for x in set(s.encode()))]
                        if s.isascii()
                        else keccak(b"StringNot7BitASCII()")[:4],
                    )
                elif len(types) == 2:
                    for allowed in [0, mask(128), 1 << 65]:
                        yield [s, allowed], [all(allowed >> x & 1 for x in s.encode())]
                else:
                    yield [s], [s.isascii()]
        return
    raise ValueError(f"no oracle for {row}")


def rpc(url, method, params):
    request = urllib.request.Request(
        url,
        json.dumps(
            {"jsonrpc": "2.0", "id": 1, "method": method, "params": params}
        ).encode(),
        {"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(request, timeout=90) as response:
        payload = json.load(response)
    if "error" in payload:
        raise RuntimeError(f"{method}: {payload['error']}")
    return payload["result"]


def launch_anvil(evm):
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        port = s.getsockname()[1]
    process = subprocess.Popen(
        [
            "anvil",
            "--port",
            str(port),
            "--hardfork",
            evm,
            "--steps-tracing",
            "--gas-limit",
            "100000000",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    url = f"http://127.0.0.1:{port}"
    for _ in range(100):
        if process.poll() is not None:
            raise RuntimeError("anvil exited before startup")
        try:
            accounts = rpc(url, "eth_accounts", [])
            return process, url, accounts[0]
        except (OSError, RuntimeError):
            time.sleep(0.1)
    process.terminate()
    process.wait(timeout=10)
    raise RuntimeError("anvil startup timed out")


def run(args):
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    safe, upstream, apis, audit = prepare(args.solc, out)
    variants = [
        (
            f"{compiler}-{source}"
            + ("" if compiler == "solar" else "-ir" if ir else "-legacy"),
            binary,
            sources,
            ir,
        )
        for compiler, binary in [("solc", args.solc), ("solar", args.solar)]
        for source, sources in [("upstream", upstream), ("safe", safe)]
        for ir in ([False, True] if compiler == "solc" else [True])
    ]
    binaries = {}
    artifacts = {}
    for label, binary, sources, ir in variants:
        print(f"Compiling {label}", flush=True)
        settings = {
            "optimizer": {"enabled": True, "runs": args.runs},
            "evmVersion": args.evm_version,
            "viaIR": ir,
            "metadata": {"bytecodeHash": "none", "appendCBOR": False},
            "outputSelection": {
                "Harness.sol": {
                    "*": ["abi", "evm.bytecode.object", "evm.deployedBytecode.object"]
                },
                "*": {"": ["ast"]},
            },
        }
        payload = {"language": "Solidity", "sources": sources, "settings": settings}
        start = time.monotonic()
        result = compile_json(binary, payload)
        elapsed = time.monotonic() - start
        if label.startswith("solc-safe") and safety_violations(result):
            raise ValueError(f"{label} safety audit failed")
        folder = out / label
        folder.mkdir()
        (folder / "input.json").write_text(json.dumps(payload, indent=2) + "\n")
        (folder / "output.json").write_text(json.dumps(result, indent=2) + "\n")
        binaries[label] = result["contracts"]["Harness.sol"]
        artifacts[label] = {
            "compiler": subprocess.check_output(
                [str(binary), "--version"], text=True
            ).strip(),
            "compiler_sha256": digest(Path(binary).read_bytes()),
            "input_sha256": digest(json.dumps(payload, sort_keys=True).encode()),
            "compile_seconds": elapsed,
            "contracts": {},
        }
    first = next(iter(binaries.values()))
    for label, contracts in binaries.items():
        for contract, artifact in contracts.items():
            if [x for x in artifact["abi"] if x["type"] != "error"] != [
                x for x in first[contract]["abi"] if x["type"] != "error"
            ]:
                raise ValueError(f"callable ABI mismatch for {label}: {contract}")
    proc, url, sender = launch_anvil(args.evm_version)
    cases = []
    failures = []
    try:
        addresses = {}
        for label, contracts in binaries.items():
            addresses[label] = {}
            for name, artifact in contracts.items():
                bytecode = artifact["evm"]["bytecode"]["object"]
                tx = rpc(
                    url,
                    "eth_sendTransaction",
                    [{"from": sender, "data": "0x" + bytecode, "gas": hex(80000000)}],
                )
                receipt = None
                for _ in range(200):
                    receipt = rpc(url, "eth_getTransactionReceipt", [tx])
                    if receipt is not None:
                        break
                    time.sleep(0.05)
                if receipt is None:
                    raise RuntimeError(f"deployment receipt timed out: {label}/{name}")
                if int(receipt["status"], 16) != 1:
                    raise RuntimeError(f"deployment failed: {label}/{name}")
                addresses[label][name] = receipt["contractAddress"]
                runtime = rpc(
                    url, "eth_getCode", [receipt["contractAddress"], "latest"]
                )
                artifacts[label]["contracts"][name] = {
                    "creation_bytes": len(bytecode) // 2,
                    "runtime_bytes": (len(runtime) - 2) // 2,
                    "deployment_gas": int(receipt["gasUsed"], 16),
                }
        for row in apis:
            rng = random.Random("20260910:" + row["library"] + "." + row["signature"])
            vectors = list(test_vectors(row, rng))
            if not vectors:
                raise ValueError(f"no test vectors for {row}")
            print(
                f"{row['library']}.{row['signature']}: {len(vectors)} cases", flush=True
            )
            for index, (values, expected) in enumerate(vectors):
                signature = (
                    row["wrapper"] + "(" + ",".join(t for t, _ in row["params"]) + ")"
                )
                calldata = keccak(signature.encode())[:4] + encode(
                    [t for t, _ in row["params"]], values
                )
                expected_revert = isinstance(expected, bytes)
                expected_bytes = (
                    expected if expected_revert else encode(row["outputs"], expected)
                )
                measurements = {}
                for label in binaries:
                    tx = {
                        "from": sender,
                        "to": addresses[label][row["library"] + "Harness"],
                        "data": "0x" + calldata.hex(),
                        "gas": hex(80000000),
                    }
                    trace = rpc(
                        url,
                        "debug_traceCall",
                        [
                            tx,
                            "latest",
                            {
                                "disableStorage": True,
                                "disableStack": True,
                                "enableMemory": False,
                            },
                        ],
                    )
                    actual = trace.get("returnValue", "").removeprefix("0x")
                    failed = trace.get("failed", False)
                    logs = trace["structLogs"]
                    measurement = {
                        "matches_oracle": (
                            failed == expected_revert
                            and actual.lower() == expected_bytes.hex()
                        ),
                        "opcode_gas": sum(int(x["gasCost"]) for x in logs),
                        "trace_gas": trace.get("gas"),
                        "failed": failed,
                        "return_data": "0x" + actual,
                    }
                    if not measurement["matches_oracle"]:
                        failures.append(
                            {
                                "library": row["library"],
                                "api": row["signature"],
                                "case": index,
                                "variant": label,
                                "expected_revert": expected_revert,
                                "expected": "0x" + expected_bytes.hex(),
                                "actual": measurement,
                                "calldata": "0x" + calldata.hex(),
                            }
                        )
                    measurements[label] = measurement
                cases.append(
                    {
                        "comparable": all(
                            m["matches_oracle"] for m in measurements.values()
                        ),
                        "library": row["library"],
                        "api": row["signature"],
                        "case": index,
                        "calldata": "0x" + calldata.hex(),
                        "expected_revert": expected_revert,
                        "expected": "0x" + expected_bytes.hex(),
                        "variants": measurements,
                    }
                )
    finally:
        proc.terminate()
        proc.wait(timeout=10)
    report = {
        "schema": "safe-solady-comparison@2",
        "evm_version": args.evm_version,
        "optimizer_runs": args.runs,
        "git_head": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=REPO, text=True
        ).strip(),
        "audit": audit,
        "artifacts": artifacts,
        "cases": cases,
        "failures": failures,
    }
    (out / "results.json").write_text(json.dumps(report, indent=2) + "\n")
    write_report(report, out / "report.md")
    write_api_report(report, out / "api-gas.md")
    print(
        f"{len(cases)} cases, {len(failures)} mismatches; {out / 'report.md'}",
        flush=True,
    )
    return int(bool(failures))


def comparison_delta(case):
    # An incorrect result must never earn a performance win, on either side.
    if not all(v["matches_oracle"] for v in case["variants"].values()):
        return None
    reference = min(
        case["variants"][x]["opcode_gas"]
        for x in ["solc-upstream-ir", "solc-upstream-legacy"]
    )
    return case["variants"]["solar-safe"]["opcode_gas"] - reference


def write_report(report, path):
    comparable = [r for r in report["cases"] if comparison_delta(r) is not None]
    lines = [
        "# Safe Solady comparison",
        "",
        "**The full API compatibility and equal-or-better gas target is not met.**",
        "",
        f"EVM: {report['evm_version']}; optimizer runs: {report['optimizer_runs']}; solc legacy and via-IR are both included.",
        "",
        f"{report['audit']['implemented_apis']} function signatures implemented in {report['audit']['library_count']} libraries; the pinned archive has {report['audit']['total_source_files']} source files. This is a partial port.",
        "",
        (
            f"{len(report['cases'])} deterministic differential cases; {len(report['failures'])} mismatching executions. "
            f"{len(report['cases']) - len(comparable)} cases are excluded from performance comparisons because at least one implementation disagrees with the oracle."
        ),
        "",
        (
            "Gas is the sum of executed opcode gas costs from debug_traceCall, excluding transaction intrinsic gas. "
            "Wrappers, inputs, optimizer runs and fork are identical. Calls do not share execution state. "
            "These library harness costs include ABI decoding, dispatch and encoding, and are not isolated instruction costs."
        ),
        "",
        (
            "The reference is the cheaper upstream solc result for each call (legacy or via-IR). "
            "This envelope is stricter than choosing one compiler pipeline for a whole deployment. "
            "All six raw legs, revert checks, input calldata and every per-call result are in results.json. "
            "Case counts are coverage counts, not a workload-weighted score."
        ),
        "",
        "| Library | Comparable / excluded cases | Safe Solar wins / ties / losses | Worst gas delta |",
        "|---|---:|---:|---:|",
    ]
    for library in sorted({r["library"] for r in report["cases"]}):
        cases = [r for r in report["cases"] if r["library"] == library]
        deltas = [d for r in cases if (d := comparison_delta(r)) is not None]
        lines.append(
            f"| {library} | {len(deltas)} / {len(cases) - len(deltas)} | "
            f"{sum(d < 0 for d in deltas)} / {deltas.count(0)} / {sum(d > 0 for d in deltas)} | "
            + (f"{max(deltas):+d}" if deltas else "n/a")
            + " |"
        )
    labels = list(report["artifacts"])
    lines += [
        "",
        "Runtime bytes for identical harnesses (partial libraries still omit the missing APIs):",
        "",
        "| Harness | " + " | ".join(labels) + " |",
        "|---|" + "---:|" * len(labels),
    ]
    for name in sorted(report["artifacts"]["solar-safe"]["contracts"]):
        sizes = [
            str(report["artifacts"][x]["contracts"][name]["runtime_bytes"])
            for x in labels
        ]
        lines.append(f"| {name} | " + " | ".join(sizes) + " |")
    lines += [
        "",
        "Compatibility failures (retained as failures; the runner exits nonzero):",
        "",
        "| Library | API | Variant | Mismatching cases |",
        "|---|---|---|---:|",
    ]
    groups = {}
    for f in report["failures"]:
        key = (f["library"], f["api"], f["variant"])
        groups[key] = groups.get(key, 0) + 1
    for (library, api, variant), count in sorted(groups.items()):
        lines.append(f"| {library} | `{api}` | {variant} | {count} |")
    if not groups:
        lines.append("| — | — | — | 0 |")
    lines += [
        "",
        (
            "No production safety claim or all-input equivalence proof is made by these tests. "
            "Missing APIs, malformed-input contracts, memory aliasing and storage layout require separate review."
        ),
    ]
    path.write_text("\n".join(lines) + "\n")


def write_api_report(report, path):
    lines = [
        "# Per-function gas comparison",
        "",
        (
            "Gas includes each identical library harness's dispatch and ABI handling. "
            "Reference gas is the per-call minimum of upstream solc legacy and via-IR. "
            "Mismatches are excluded; results.json retains their calldata and outcomes."
        ),
        "",
        "| Library | Function | Comparable / excluded | Wins / ties / losses | Best delta | Worst delta |",
        "|---|---|---:|---:|---:|---:|",
    ]
    groups = {}
    for case in report["cases"]:
        groups.setdefault((case["library"], case["api"]), []).append(case)
    for (library, api), cases in sorted(groups.items()):
        deltas = [d for c in cases if (d := comparison_delta(c)) is not None]
        lines.append(
            f"| {library} | `{api}` | {len(deltas)} / {len(cases) - len(deltas)} | "
            f"{sum(d < 0 for d in deltas)} / {deltas.count(0)} / {sum(d > 0 for d in deltas)} | "
            + (f"{min(deltas):+d} | {max(deltas):+d}" if deltas else "n/a | n/a")
            + " |"
        )
    path.write_text("\n".join(lines) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--solc", type=Path, required=True)
    parser.add_argument("--solar", type=Path, default=REPO / "target/debug/solar")
    parser.add_argument("--evm-version", default="cancun")
    parser.add_argument("--runs", type=int, default=200)
    parser.add_argument("--output", type=Path, required=True)
    raise SystemExit(run(parser.parse_args()))
