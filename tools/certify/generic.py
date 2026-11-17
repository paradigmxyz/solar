#!/usr/bin/env python3
"""The generic certification driver: one command for any contract inside the supported envelope.

  tools/certify/certify SOURCE.sol SPEC [--invariants FILE] [--tier T] [--record-dir DIR] ...

SPEC is natspec (the solar-verify annotations in SOURCE are the specification, requested tier
certified) or none (no specification, requested tier compiled). Nothing here selects by contract
name: every per-contract input scripts/certify reads (the source, shape.json, the SolidCore source
AST and the generic source check) is generated from SOURCE and this run's compile, committed as one
local input commit on top of the solidity-lean base, and checked by the kernel there.

Exit status and the last line printed:
  0  TIER <tier>: the requested tier holds (the record's strict predicate, every component true)
  2  CHECKED <tier reached>: the run proved less than the requested tier (each status in the record)
  3  BLOCKED: <reason>: the source is outside the supported envelope (README.md lists it; recursion
     is outside it); nothing was proved or claimed
  4  UNSUPPORTED-lifecycle: kernel: <reason>: --tier lifecycle observed every stage of the creation
     (the init code, the runtime bytes it returns, the storage after construction) and they agree,
     but the kernel proves neither the deployment nor its refutation: a constructor outside the
     generic creation frame (an opcode it does not evaluate, such as SLOAD, KECCAK256, a call, a log
     or CREATE; a payable constructor; a revert path), or a base without the frame. Nothing is
     certified
  1  ERROR: <message>: the driver or a step failed

--tier lifecycle (with --constructor-args W,... for a constructor with static word parameters) is
about the creation, not the calls: solar's init code with the ABI-encoded arguments appended is run
by EthLib, which must succeed and return the runtime bytes (solar's bin-runtime, with immutables
the constructor fills at their placeholders only), and SolidCore's constructor, called with the
same argument values, must leave the storage EthLib's run leaves. It states that as
Verify.Lifecycle.Statement (solidity-lean Verify/Lifecycle/Route.lean). A disagreement is
REJECTED with its stage. The kernel stage then writes Certified/NAME/Lifecycle.lean from the run's
own init code, runtime code, arguments and the probe's storage, and builds it in the run's clone of
the base (one Lean thread, -KleanMemory=6000): Deploys (initArgs init ws) code s by the generic
creation frame (Verify/Lifecycle/CreationFrame.lean, deploys_of_check, one kernel evaluation) and
exists d, DeployedAtWith fuel c (wordArgs ws) d s by kernel_rfl, composed as
Certified.NAME.Lifecycle.statement. Only that build passing is CERTIFIED (exit 0, tier lifecycle);
when it fails and the kernel proves the refutation (not_deploys_of_refutes) instead, the run is
REJECTED at the kernel stage.

The record (DIR/record.json) holds every pin: the solar commit and binary digest, the solidity-lean
base and input commits, the Lean toolchain digest, solc, the options, the source, spec and
invariant digests, the statuses, the strict predicate's components and the replay receipt.
"""

import argparse
import datetime
import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(HERE))
import keccak

SOLC_VERSION = "0.8.35"
ENVIRONMENT = "one frame, caller 0x2000, contract at 0x1000, the block and transaction of Target.message (fixed, not quantified)"
OPTIONS = ["-Ogas", "--evm-version", "osaka"]
# Lean's per-process memory cap for scripts/certify's builds, pinned to CI's: its own default (3000
# MB) is too small for a clean build of some base modules, whose kernel then reports excessive
# memory consumption rather than a proof failure.
LEAN_MEMORY = "6000"
# Build output that says Lean ran out of a resource rather than refuting a proof.
RESOURCE_MARKS = (
    "excessive memory consumption",
    "(deterministic) timeout",
    "maximum number of heartbeats",
    "maximum recursion depth",
    "out of memory",
    "Killed",
)
# Opcodes outside the envelope in the runtime bytecode: calls, creation, self-destruct, gas and
# return data reads. The loop checker's fragment rejects them too; this check only names them early.
BAD_OPS = {
    0xF0: "CREATE",
    0xF1: "CALL",
    0xF2: "CALLCODE",
    0xF4: "DELEGATECALL",
    0xF5: "CREATE2",
    0xFA: "STATICCALL",
    0xFF: "SELFDESTRUCT",
    0x5A: "GAS",
    0x3D: "RETURNDATASIZE",
    0x3E: "RETURNDATACOPY",
    0x31: "BALANCE",
    0x47: "SELFBALANCE",
    0x3B: "EXTCODESIZE",
    0x3C: "EXTCODECOPY",
    0x3F: "EXTCODEHASH",
}


class Blocked(Exception):
    def __init__(self, reasons):
        order = [
            "external call",
            "contract creation",
            "inline assembly",
            "value transfer",
            "loop",
            "recursion",
            "bytecode",
        ]
        groups = {}
        for r in reasons:
            groups.setdefault(r["feature"], []).append(r)
        feats = sorted(
            groups, key=lambda f: order.index(f) if f in order else len(order)
        )
        super().__init__(
            "; ".join(
                f"{f} ({len(groups[f])}x, first {(groups[f][0]['where'] + ': ') if groups[f][0]['where'] else ''}{groups[f][0]['detail']})"
                for f in feats
            )
        )
        self.reasons = reasons


class Failed(Exception):
    pass


def spec_rejection(lines):
    """spec_connect.py's REJECTED lines in scripts/certify's output: a theorem of SpecConnect that
    does not check, named."""
    return [
        l.split("spec_connect: ", 1)[1].strip()
        for l in lines
        if "spec_connect: REJECTED: spec:" in l
    ]


def resource_failures(lines):
    """The build output lines that say Lean ran out of memory, time or heartbeats."""
    return [l.strip() for l in lines if any(m in l for m in RESOURCE_MARKS)]


def sha256_file(p):
    return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()


def run(cmd, **kw):
    r = subprocess.run(
        cmd, capture_output=True, text=True, errors="replace", **kw, check=False
    )
    return r.returncode, r.stdout, r.stderr


def log(msg):
    print(msg, flush=True)


def stream(cmd, logf, **kw):
    """Run cmd, copying its merged output to logf and to stdout as it comes; return its exit.

    The output is decoded with replacement: a byte that is not UTF-8 (such as a message cut
    inside a multibyte character) must not end the driver.
    """
    with open(logf, "w") as lf:
        p = subprocess.Popen(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            errors="replace",
            **kw,
        )
        for line in p.stdout:
            lf.write(line)
            lf.flush()
            sys.stdout.write(line)
        return p.wait()


# --- the envelope, from the solc AST ---------------------------------------------------------


def solc_ast(solc, src):
    # The import closure is read from the source's directory for the analysis only (so that every
    # reason a source is outside the envelope is named); a source with imports is never certified.
    inp = {
        "language": "Solidity",
        "sources": {src.name: {"urls": [src.name]}},
        "settings": {"outputSelection": {"*": {"": ["ast"], "*": ["abi"]}}},
    }
    rc, out, err = run(
        [
            solc,
            "--standard-json",
            "--base-path",
            str(src.parent),
            "--allow-paths",
            str(src.parent),
        ],
        input=json.dumps(inp),
    )
    if rc != 0:
        raise Failed(f"solc --standard-json exits {rc}: {err.strip()[:400]}")
    res = json.loads(out)
    errs = [e for e in res.get("errors", []) if e.get("severity") == "error"]
    if errs:
        msg = errs[0].get("formattedMessage", errs[0].get("message", ""))
        if "not found" in msg and "import" in msg.lower():
            raise Blocked(
                [
                    {
                        "feature": "imports",
                        "where": "",
                        "detail": "the source imports other files; the driver takes one self-contained file and never flattens (a frozen import closure is not supported yet)",
                    }
                ]
            )
        raise Failed(f"solc rejects the source: {msg.strip()[:400]}")
    return res["sources"][src.name]["ast"], res["contracts"][src.name]


def walk(node):
    if isinstance(node, dict):
        yield node
        for v in node.values():
            yield from walk(v)
    elif isinstance(node, list):
        for v in node:
            yield from walk(v)


def line_of(text, node):
    try:
        off = int(node.get("src", "0:0:0").split(":")[0])
    except ValueError:
        return ""
    return f"line {text.count(chr(10), 0, off) + 1}"


ABI_INT = re.compile(r"^(u?)int(\d*)$")


def param_of(p, enums):
    """The SolidCore Abi.Param of an ABI parameter, or None outside the envelope."""
    t, it = p["type"], p.get("internalType", "")
    if it.startswith("enum "):
        n = enums.get(it[len("enum ") :])
        return None if n is None else f".enum {n}"
    if t == "bool":
        return ".bool"
    if t == "address":
        return ".uint 160"
    m = ABI_INT.match(t)
    if m:
        bits = int(m.group(2) or 256)
        return f".{'uint' if m.group(1) else 'int'} {bits}"
    return None


def envelope(
    text, ast, contracts, contract, invariants, natspec=False, lifecycle=False
):
    reasons = []

    def block(feature, node, detail):
        reasons.append(
            {
                "feature": feature,
                "where": line_of(text, node) if node else "",
                "detail": detail,
            }
        )

    for n in ast.get("nodes", []):
        if n.get("nodeType") == "ImportDirective":
            block(
                "imports",
                n,
                "the source imports other files; the driver takes one self-contained file and never flattens (a frozen import closure is not supported yet)",
            )
    defs = [
        n for n in ast.get("nodes", []) if n.get("nodeType") == "ContractDefinition"
    ]
    for d in defs:
        if d.get("contractKind") != "contract" or d.get("abstract"):
            block(
                "several contracts",
                d,
                f"{d.get('contractKind')} {d.get('name')}: only one concrete contract per file",
            )
        if d.get("baseContracts"):
            block(
                "inheritance",
                d,
                f"{d.get('name')} inherits; only one contract with no bases",
            )
    concrete = [
        d for d in defs if d.get("contractKind") == "contract" and not d.get("abstract")
    ]
    if contract is None:
        if len(concrete) != 1:
            block(
                "several contracts",
                None,
                f"{len(concrete)} concrete contracts; name one with --contract",
            )
            raise Blocked(reasons)
        contract = concrete[0]["name"]
    cdefs = [d for d in concrete if d["name"] == contract]
    if not cdefs:
        raise Failed(f"no contract {contract} in the source")
    cdef = cdefs[0]
    own = {n["id"] for n in walk(cdef) if "id" in n}
    enums = {}
    for n in walk(ast):
        if n.get("nodeType") == "EnumDefinition":
            enums[n["name"]] = len(n.get("members", []))
            enums[n.get("canonicalName", n["name"])] = len(n.get("members", []))
    for n in walk(cdef):
        t = n.get("nodeType")
        if t == "InlineAssembly":
            block(
                "inline assembly",
                n,
                "assembly blocks have no SolidCore source semantics here",
            )
        elif t == "TryStatement":
            block("external call", n, "try/catch makes an external call")
        elif t == "NewExpression":
            block("contract creation", n, "new creates a contract (CREATE)")
        elif t in ("ForStatement", "WhileStatement", "DoWhileStatement"):
            block(
                "loop",
                n,
                "loops need a kernel-checked invariant certificate; the generic source stage at this base is loop-free"
                + (
                    ", and the supplied invariants file is not consumed yet"
                    if invariants
                    else " (no --invariants given)"
                ),
            )
        elif t == "FunctionDefinition":
            if n.get("stateMutability") == "payable":
                block(
                    "value transfer",
                    n,
                    f"{n.get('kind')} {n.get('name') or ''} is payable",
                )
            if (
                n.get("kind") == "constructor"
                and n.get("parameters", {}).get("parameters")
                and not lifecycle
            ):
                block(
                    "constructor parameters",
                    n,
                    "the generic stage runs the constructor with no arguments (Wide.DeployedAt), so its storage is unknown; --tier lifecycle with --constructor-args takes static word arguments",
                )
            if n.get("kind") in ("receive", "fallback"):
                block(
                    "fallback",
                    n,
                    f"{n.get('kind')} function: calls outside the ABI's functions",
                )
        elif t == "MemberAccess" and n.get("memberName") in (
            "call",
            "delegatecall",
            "staticcall",
            "transfer",
            "send",
            "code",
            "codehash",
            "balance",
        ):
            block(
                "external call"
                if n["memberName"] not in ("code", "codehash", "balance")
                else "account read",
                n,
                f".{n['memberName']} on an address",
            )
        elif t == "FunctionCall":
            ty = (
                n.get("expression", {})
                .get("typeDescriptions", {})
                .get("typeString", "")
            )
            ref = n.get("expression", {}).get("referencedDeclaration")
            if " external" in ty and (
                ref is None
                or ref not in own
                or n.get("expression", {}).get("nodeType") == "MemberAccess"
            ):
                block("external call", n, f"a call of an external function ({ty[:80]})")
        elif t == "Identifier" and n.get("name") in (
            "selfdestruct",
            "gasleft",
            "blockhash",
        ):
            block(
                "environment",
                n,
                f"{n['name']} is outside the source semantics' environment",
            )
    # Recursion: the generic stage proofs need a cut point at every back edge, and a recursive call
    # site has none (the stage proofs have no measure to cut at), so the call graph must be acyclic.
    variables = {
        n["id"]
        for n in walk(ast)
        if n.get("nodeType") == "VariableDeclaration" and "id" in n
    }
    cycles, values = recursion_cycles(cdef, variables)
    for names, call in cycles:
        block(
            "recursion",
            call,
            f"unsupported: {' and '.join(names)} can call {'itself' if len(names) == 1 else 'each other'} "
            "(the generic stage needs an acyclic call graph; recursive calls have no cut points)",
        )
    for call in values:
        block(
            "recursion",
            call,
            "unsupported: a call through an internal function value, so the call graph is not known "
            "to be acyclic",
        )
    if natspec:
        # The published statement fixes the environment (one frame, Target.message's block and
        # transaction): a spec clause that reads the environment could be false there and make the
        # function's spec hold vacuously, so it is outside the envelope.
        env_re = re.compile(
            r"\b(block\.\w+|tx\.\w+|gasleft|blockhash|chainid|basefee|coinbase|prevrandao)\b"
        )
        for n in walk(cdef):
            if n.get("nodeType") in (
                "ContractDefinition",
                "FunctionDefinition",
                "VariableDeclaration",
            ):
                doc = n.get("documentation") or {}
                doc = doc.get("text", "") if isinstance(doc, dict) else str(doc)
                for line in doc.splitlines():
                    if "@custom:" in line and env_re.search(line):
                        block(
                            "environment",
                            n,
                            f"a spec clause reads the environment ({env_re.search(line).group(1)}), which the statement fixes: {line.strip()[:120]}",
                        )
    abi = contracts[contract]["abi"]
    fns = []
    for e in abi:
        if e["type"] != "function":
            continue
        sig = f"{e['name']}({','.join(p['type'] for p in e['inputs'])})"
        params = [param_of(p, enums) for p in e["inputs"]]
        outs = [param_of(p, enums) for p in e.get("outputs", [])]
        if None in params or None in outs:
            block(
                "ABI types",
                None,
                f"{sig}: only uintN, intN, bool, address and enum parameters and results",
            )
        fdef = next(
            (
                n
                for n in walk(cdef)
                if n.get("nodeType") == "FunctionDefinition"
                and n.get("name") == e["name"]
                and n.get("kind") == "function"
            ),
            None,
        )
        doc = ((fdef or {}).get("documentation") or {}).get("text", "") if fdef else ""
        fns.append(
            {
                "name": e["name"],
                "sig": sig,
                "params": params,
                "mutates": e.get("stateMutability") not in ("view", "pure"),
                "specified": "@custom:ensures" in doc or "@custom:requires" in doc,
                "succeeds": "@custom:succeeds-if" in doc
                or "@custom:succeeds_if" in doc,
                "sel": int.from_bytes(keccak.keccak256(sig.encode())[:4], "big"),
            }
        )
    events = []
    for e in abi:
        if e["type"] == "event":
            sig = f"{e['name']}({','.join(p['type'] for p in e['inputs'])})"
            events.append(
                {
                    "sig": sig,
                    "topic": int.from_bytes(keccak.keccak256(sig.encode()), "big"),
                }
            )
    if not fns:
        block("no functions", None, "the contract has no external functions")
    if reasons:
        raise Blocked(reasons)
    return contract, fns, events


def recursion_cycles(cdef, variables):
    """The contract's recursion: each strongly connected component of its internal call graph that
    has a cycle (functions or modifiers that can reach themselves), as (names, first call site), and
    the calls through internal function values, whose callee the source does not name. The graph's
    nodes are the contract's functions and modifiers; an edge is an internal call naming one of them
    (an Identifier, or a MemberAccess such as C.f) or a modifier invocation. VARIABLES holds the ids
    of the source's variable declarations (struct members included)."""
    callables = {
        n["id"]: n
        for n in walk(cdef)
        if n.get("nodeType") in ("FunctionDefinition", "ModifierDefinition")
        and "id" in n
    }
    edges, site, values = {}, {}, []
    for cid, c in callables.items():
        out = edges.setdefault(cid, [])
        for n in walk(c):
            t = n.get("nodeType")
            if t == "ModifierInvocation":
                ref = (n.get("modifierName") or {}).get("referencedDeclaration")
            elif t == "FunctionCall" and n.get("kind") == "functionCall":
                e = n.get("expression") or {}
                ty = e.get("typeDescriptions", {}).get("typeString", "")
                ref = e.get("referencedDeclaration")
                internal_value = ty.startswith("function (") and " external" not in ty
                # A named callee is a function value when it is a variable; a computed one (an index,
                # a conditional, a call's result) always is. new and call options are creation and
                # external calls, which the envelope refuses on their own.
                named = e.get("nodeType") in ("Identifier", "MemberAccess")
                other = (
                    "ElementaryTypeNameExpression",
                    "NewExpression",
                    "FunctionCallOptions",
                )
                if internal_value and (
                    ref in variables if named else e.get("nodeType") not in other
                ):
                    values.append(n)
            else:
                continue
            if ref in callables:
                out.append(ref)
                site.setdefault((cid, ref), n)
    # Tarjan's strongly connected components, iteratively.
    index, low, on, stack, sccs = {}, {}, set(), [], []
    for root in callables:
        if root in index:
            continue
        index[root] = low[root] = len(index)
        stack.append(root)
        on.add(root)
        work = [(root, iter(edges[root]))]
        while work:
            v, it = work[-1]
            w = next(it, None)
            if w is not None:
                if w not in index:
                    index[w] = low[w] = len(index)
                    stack.append(w)
                    on.add(w)
                    work.append((w, iter(edges[w])))
                elif w in on:
                    low[v] = min(low[v], index[w])
                continue
            work.pop()
            if work:
                low[work[-1][0]] = min(low[work[-1][0]], low[v])
            if low[v] == index[v]:
                comp = []
                while True:
                    w = stack.pop()
                    on.discard(w)
                    comp.append(w)
                    if w == v:
                        break
                sccs.append(comp)
    cycles = []
    for comp in sccs:
        members = set(comp)
        inner = [(x, y) for x in comp for y in edges[x] if y in members]
        if not inner:
            continue
        first = min(
            inner, key=lambda xy: int(site[xy].get("src", "0:0:0").split(":")[0])
        )
        names = sorted(
            callables[m].get("name") or callables[m].get("kind", "") for m in comp
        )
        cycles.append((names, site[first]))
    return cycles, values


def constructor_params(ast, contract):
    """The ABI types of CONTRACT's constructor parameters, [] without a constructor."""
    for d in ast.get("nodes", []):
        if d.get("nodeType") == "ContractDefinition" and d.get("name") == contract:
            for n in d.get("nodes", []):
                if (
                    n.get("nodeType") == "FunctionDefinition"
                    and n.get("kind") == "constructor"
                ):
                    return [
                        q.get("typeDescriptions", {}).get("typeString", "")
                        for q in n["parameters"]["parameters"]
                    ]
    return []


def immutables(ast, contract):
    """The names of CONTRACT's immutable state variables."""
    for d in ast.get("nodes", []):
        if d.get("nodeType") == "ContractDefinition" and d.get("name") == contract:
            return [
                n["name"]
                for n in d.get("nodes", [])
                if n.get("nodeType") == "VariableDeclaration"
                and n.get("mutability") == "immutable"
            ]
    return []


def constructor_words(types, text):
    """The argument words of --constructor-args TEXT (comma-separated, decimal or 0x) for the
    constructor parameter TYPES: static words only (uintN, address, bool), each within its type,
    else Blocked."""
    vals = [v.strip() for v in (text or "").split(",") if v.strip()]
    if len(vals) != len(types):
        raise Blocked(
            [
                {
                    "feature": "constructor parameters",
                    "where": "",
                    "detail": f"the constructor takes {len(types)} argument(s) ({', '.join(types) or 'none'}); --constructor-args gives {len(vals)}",
                }
            ]
        )
    ws = []
    for t, v in zip(types, vals):
        try:
            w = int(v, 0)
        except ValueError:
            raise Blocked(
                [
                    {
                        "feature": "constructor parameters",
                        "where": "",
                        "detail": f"{v!r} is not a number",
                    }
                ]
            ) from None
        m = re.fullmatch(r"uint(\d*)", t)
        bits = (
            int(m.group(1) or 256)
            if m
            else 160
            if t == "address"
            else 1
            if t == "bool"
            else None
        )
        if bits is None:
            raise Blocked(
                [
                    {
                        "feature": "constructor parameters",
                        "where": "",
                        "detail": f"{t}: the lifecycle route takes static uintN, address and bool arguments only",
                    }
                ]
            )
        if not 0 <= w < 2**bits:
            raise Blocked(
                [
                    {
                        "feature": "constructor parameters",
                        "where": "",
                        "detail": f"{v} is outside {t}",
                    }
                ]
            )
        ws.append(w)
    return ws


def runtime_problem(deployed, plain, imm):
    """Why the runtime bytes the init code returned are not solar's bin-runtime PLAIN, or None:
    equal, or with immutables IMM the same length and different only where PLAIN holds a zero
    byte (an immutable's placeholder the constructor fills)."""
    a, b = bytes.fromhex(deployed), bytes.fromhex(plain)
    if a == b:
        return None
    if not imm:
        return f"the init code returns {len(a)} bytes ({hashlib.sha256(a).hexdigest()[:12]}), not solar's bin-runtime ({len(b)} bytes, {hashlib.sha256(b).hexdigest()[:12]})"
    if len(a) != len(b):
        return f"the init code returns {len(a)} bytes, solar's bin-runtime has {len(b)}"
    bad = [i for i in range(len(b)) if a[i] != b[i] and b[i] != 0]
    return (
        f"the returned runtime differs from bin-runtime outside an immutable placeholder (offset {bad[0]})"
        if bad
        else None
    )


def storage_pairs(text):
    return sorted(
        (int(k), int(v)) for k, v in re.findall(r"\((\d+), (\d+)\)", text or "")
    )


def scan_bytecode(hexcode):
    code = bytes.fromhex(hexcode)
    found, i = {}, 0
    while i < len(code):
        op = code[i]
        if op == 0xFE:  # INVALID: the start of the metadata or data section
            break
        if op in BAD_OPS:
            found.setdefault(BAD_OPS[op], i)
        i += 1 + (op - 0x5F if 0x60 <= op <= 0x7F else 0)
    return found


# --- generated inputs -------------------------------------------------------------------------


def lean_str(s):
    return json.dumps(s)


def nested_splits(eqs):
    """The tactic lines of `funext; unfold` by one `split` per table entry, nested under bullets."""
    n, out = len(eqs), []
    for i, e in enumerate(eqs):
        out.append("  split" if i == 0 else "  " * i + "· split")
        out.append("  " * (i + 1) + f"· next h => subst h; exact {e}")
    out.append("  " * n + "· rfl")
    return "\n".join(out) + "\n"


def lowering_lean(name, contract, fns, events, core_lit, storage_lit, sym=False):
    ns = f"Certified.{name}.Lowering"
    sels = "\n".join(
        f"/-- The selector of §{f['sig']}§. -/\nabbrev sel{i} : Nat := {hex(f['sel'])}\n"
        for i, f in enumerate(fns)
    )
    fndefs = "".join(
        f"/-- §{f['sig']}§ and its parameters. -/\ndef fn{i} : Fn := ⟨sel{i}, [{', '.join(f['params'])}]⟩\n\n"
        for i, f in enumerate(fns)
    )
    fnlist = ", ".join(f"fn{i}" for i in range(len(fns)))
    kr = []
    for i, f in enumerate(fns):
        kr.append(
            f"def selSig{i} : SolidCore.Solidity.Shared.Word :=\n  SolidCore.Solidity.Source.ABI.selectorFromSignature {lean_str(f['sig'])}\n\n"
            f"kernel_rfl selSig{i}_eq selSig{i} sel{i}\n"
        )
    for i, e in enumerate(events):
        kr.append(
            f"def topicSig{i} : Nat := SolidCore.Solidity.Source.Keccak.digestWord {lean_str(e['sig'])}\n\n"
            f"kernel_rfl topicSig{i}_eq topicSig{i} {hex(e['topic'])}\n"
        )
    table = "".join(
        f"  if s = {lean_str(f['sig'])} then sel{i}\n  else" for i, f in enumerate(fns)
    )
    splits = nested_splits([f"selSig{i}_eq" for i in range(len(fns))])
    using = "selector_eq"
    digest = ""
    if events:
        dt = "".join(
            f"  if s = {lean_str(e['sig'])} then {hex(e['topic'])}\n  else"
            for e in events
        )
        dsplits = nested_splits([f"topicSig{i}_eq" for i in range(len(events))])
        digest = (
            f"/-- §Keccak.digestWord§, with the events' signatures by table. -/\n"
            f"def digestTable (s : String) : Nat :=\n{dt} SolidCore.Solidity.Source.Keccak.digestWord s\n\n"
            f"theorem digest_eq : SolidCore.Solidity.Source.Keccak.digestWord = digestTable := by\n"
            f"  funext s\n  unfold digestTable\n{dsplits}\n"
        )
        using += " digest_eq"
    # The check for every caller (caller_route, the any route only): Generic.checkSym and
    # checkValue ... .allSym, with the caller a symbolic word, by the kernel's evaluation as the
    # fixed-caller check is. The fixed route's module is unchanged.
    sym_import = "import SolidCore.Refinement.Generic.StageFrom\n" if sym else ""
    sym_thms = (
        f"""
set_option maxRecDepth 100000 in
/-- The kernel's evaluation: the generic check for every caller (the caller a symbolic word)
accepts stage0 for every function. -/
theorem check_sym_stage0 :
    Generic.checkSym fuel mirFuel coreLit SolarTV.Export.{contract}.stage0 fns = true := by
  decide +kernel

set_option maxRecDepth 100000 in
/-- The kernel's evaluation: stage0 reverts on a call with a value, whoever sends it. -/
theorem checkValueSym_stage0 :
    Generic.checkValue mirFuel coreLit SolarTV.Export.{contract}.stage0 fns .allSym = true := by
  decide +kernel
"""
        if sym
        else ""
    )
    return f"""import SolidCore.Refinement.Generic.Value
{sym_import}import SolidCore.Refinement.KernelCopies
import SolidCore.Refinement.KernelRfl
import {name}.Source
import {name}.gen.{contract}.Stage0

-- Generated by solar's tools/certify/generic.py from this run's source and export; do not edit.
-- Untrusted data and composition: every literal below is checked by the kernel.

/-!
# {contract}: the generic source check, generated

§coreLit§ and §storageLit§ are SolidCore's own evaluation of the checked contract and of its
constructor, printed by the driver's probe and read back; §shape_eq§ proves in the kernel that
they are what §contract?§ gives, so the probe is not trusted. §check_stage0§ and §checkValue_stage0§
are the generic check on solar's first lowered MIR, by the kernel's evaluation.
-/

namespace {ns}

open SolidCore.Refinement SolidCore.Refinement.Abi SolidCore.Solidity SolidCore.Solidity.Source
open EthLib.Base SolidCore.Refinement.Evm.Wide

/-- The interpreter fuel of SolidCore's runs. -/
def fuel : Nat := 4096

/-- The interpreter fuel of the symbolic run of the module: steps along each path. -/
def mirFuel : Nat := 4096

/-- SolidCore's checked §{contract}§. -/
def contract? : Except TypeCheck.TypeError TypeCheck.CheckedContract :=
  TypeCheck.CheckedInput.contract Certified.{name}.Src.importedSourceUnit {lean_str(contract)}

{sels}
{fndefs}/-- The external functions of §{contract}§, from its ABI. -/
def fns : List Fn := [{fnlist}]

/-- SolidCore's core of §{contract}§, as SolidCore's evaluation printed it (checked by §shape_eq§). -/
def coreLit : Contract :=
  {core_lit}

/-- What the constructor leaves in storage, as SolidCore's evaluation printed it (checked by §shape_eq§). -/
def storageLit : List (Nat × Nat) := {storage_lit}

/-! ## Selectors and topics, each by Keccak-256 once, then by table -/

{chr(10).join(kr)}
/-- §ABI.selectorFromSignature§, with the contract's signatures by table. -/
def selectorTable (s : String) : SolidCore.Solidity.Shared.Word :=
{table} SolidCore.Solidity.Source.ABI.selectorFromSignature s

theorem selector_eq : SolidCore.Solidity.Source.ABI.selectorFromSignature = selectorTable := by
  funext s
  unfold selectorTable
{splits}
{digest}/-- SolidCore's checked core, and whether the constructor leaves exactly §storageLit§ and no logs. -/
def shape? : Option (Contract × Bool) :=
  match contract? with
  | .ok c => some (c.core, Wide.deployedAt fuel c storageLit)
  | .error _ => none

kernel_rfl shape_eq shape? (some (coreLit, true)) using {using}

set_option maxRecDepth 100000 in
/-- The kernel's evaluation: the generic check accepts stage0 for every function. -/
theorem check_stage0 :
    Generic.check fuel mirFuel coreLit SolarTV.Export.{contract}.stage0 fns .all = true := by
  decide +kernel

set_option maxRecDepth 100000 in
/-- The kernel's evaluation: no function is payable, and stage0 reverts on a call with a value. -/
theorem checkValue_stage0 :
    Generic.checkValue mirFuel coreLit SolarTV.Export.{contract}.stage0 fns .all = true := by
  decide +kernel
{sym_thms}
theorem deployed_of {{c : TypeCheck.CheckedContract}} (hc : contract? = .ok c) :
    c.core = coreLit ∧ ∃ d, Wide.DeployedAt fuel c d storageLit := by
  have h := shape_eq
  unfold shape? at h
  rw [hc] at h
  obtain ⟨hk, hd⟩ := Prod.mk.inj (Option.some.inj h)
  exact ⟨hk, Wide.deployedAt_of hd⟩

end {ns}
""".replace("§§", "§§").replace("§", "\u0060")


def probe_lean(name, contract, fns):
    """The probe (never part of a proof): SolidCore's evaluation of the checked contract's core and
    of its constructor's storage, printed for the generated module to state as literals, and per
    function whether SolidCore's erased tree and the symbolic run of the first lowered MIR exist
    and agree. Run with lake env lean in a scratch clone; nothing it prints is trusted."""
    checks = "\n".join(
        f'  IO.println ("PROBE fn {f["sig"]}: " ++ (match core.findFunctionBySelector? {f["sel"]} with\n'
        f'    | none => "no source function"\n'
        f"    | some fn => match Generic.eraseFn (Generic.envOf core) fuel fn, "
        f"Generic.symTree SolarTV.Export.{contract}.stage0 {f['sel']} {len(f['params'])} mirFuel .all with\n"
        f'      | none, _ => "source tree undefined (erasure)"\n'
        f'      | _, none => "MIR symbolic run undefined"\n'
        f'      | some a, some b => if a.canon.beq b.canon then "agree" else ("trees differ: source " ++ (reprStr a.canon).replace "\\n" " " ++ " MIR " ++ (reprStr b.canon).replace "\\n" " ")))'
        for f in fns
    )
    # The same check for every caller (Generic.checkFnSym): the source erased with msg.sender a
    # symbolic word below 2^160, the module run with CALLER that word. caller_route reads it.
    checks += "\n" + "\n".join(
        f'  IO.println ("PROBE sym fn {f["sig"]}: " ++ (match core.findFunctionBySelector? {f["sel"]} with\n'
        f'    | none => "no source function"\n'
        f"    | some fn => match Generic.eraseFnSym (Generic.envOf core) fuel fn, "
        f"Generic.symTree SolarTV.Export.{contract}.stage0 {f['sel']} {len(f['params'])} mirFuel .allSym with\n"
        f'      | none, _ => "source tree undefined (erasure)"\n'
        f'      | _, none => "MIR symbolic run undefined"\n'
        f'      | some a, some b => if a.canon.beq b.canon then "agree" else ("trees differ: source " ++ (reprStr a.canon).replace "\\n" " " ++ " MIR " ++ (reprStr b.canon).replace "\\n" " ")))'
        for f in fns
    )
    return f"""import SolidCore.Refinement.Generic.Value
import SolidCore.Refinement.Generic.StageFrom
import {name}.Source
import {name}.gen.{contract}.Stage0

namespace Certified.{name}.Probe

open SolidCore.Refinement SolidCore.Refinement.Abi SolidCore.Solidity SolidCore.Solidity.Source
open EthLib.Base SolidCore.Refinement.Evm.Wide

def fuel : Nat := 4096
def mirFuel : Nat := 4096

def contract? : Except TypeCheck.TypeError TypeCheck.CheckedContract :=
  TypeCheck.CheckedInput.contract Certified.{name}.Src.importedSourceUnit {lean_str(contract)}

def main : IO Unit := do
  match contract? with
  | .error _ => IO.println "PROBE typecheck: error"
  | .ok c =>
  let core := c.core
  IO.println "PROBE typecheck: ok"
  IO.println ("PROBE core: " ++ ((repr core).pretty 100000000).replace "\\n" " ")
  match TypeCheck.CheckedContract.constructWithContext? fuel c (Wide.ctxOf c) State.empty
      SolidCore.Refinement.selfAddress 0 [] with
  | some (CallResult.returned d _) =>
    IO.println ("PROBE constructor: returns" ++ (if d.events.isEmpty then "" else ", with logs"))
    IO.println ("PROBE storage: " ++ reprStr d.storage.toList)
  | some _ => IO.println "PROBE constructor: does not return (reverts or fails) with no arguments"
  | none => IO.println "PROBE constructor: undefined with no arguments"
{checks}

end Certified.{name}.Probe

#eval Certified.{name}.Probe.main
"""


def lakefile_lib(name, contract):
    return f"""
-- {name}: the stages of Certified/{name}/{name}.sol's {contract} and their kernel proofs, which
-- solar's scripts/certify writes.
lean_lib «{name}» where
  srcDir := "Certified"
  roots := #[§{name}.Proofs]
  globs := #[.submodules §{name}]
""".replace("§", "\u0060")


def fresh_certified(clone, name, contract):
    """Make CLONE ready for this run's own Certified/NAME: a base that already holds the directory (an
    earlier record of the same source, published) has it removed here, so every file of the run is
    written by the run and none is reused, and the library stanza is appended only when the lakefile
    lacks it. A stanza for NAME other than the one this driver writes is refused. Returns whether the
    base held either."""
    d = clone / "Certified" / name
    held = d.exists()
    if held:
        shutil.rmtree(d)
    lf = clone / "lakefile.lean"
    text = lf.read_text()
    stanza = lakefile_lib(name, contract)
    if stanza in text:
        held = True
    elif re.search(rf"^lean_lib («{re.escape(name)}»|{re.escape(name)})( |$)", text, re.M):
        raise Failed(f"the base's lakefile declares lean_lib {name} other than as this driver writes it")
    else:
        with open(lf, "a") as f:
            f.write(stanza)
    return held


def run_probe(lean_dir, work, name, contract, fns, source_text, cache):
    """The probe, before the cold run, in a separate clone of the base (LEAN_DIR before the inputs)
    with the build directory of CACHE when given (lake rebuilds what differs), never the run's own.
    It prints SolidCore's evaluation of the core and the constructor's storage, which the generated
    module states as literals the kernel checks, and per function whether the generic check's two
    trees agree. Nothing it prints is trusted: a disagreement only makes the run BLOCKED (which
    claims nothing), and an agreement only lets the cold run start, which re-proves everything."""
    pl = work / "probe-lean"
    git(work, "clone", "-q", "--shared", str(lean_dir), str(pl))
    git(pl, "submodule", "update", "-q", "--init", "--recursive", check=False)
    if cache:
        rc, _, err = run(
            ["cp", "-a", str(pathlib.Path(cache) / ".lake"), str(pl / ".lake")]
        )
        if rc != 0:
            raise Failed(f"cannot copy the probe cache: {err[:200]}")
    fresh_certified(pl, name, contract)
    d = pl / "Certified" / name
    d.mkdir(parents=True)
    shutil.copytree(work / "probe" / "gen", d / "gen")
    (d / "Source.lean").write_text(source_text)
    (d / "Probe.lean").write_text(probe_lean(name, contract, fns))
    env = dict(os.environ, LEAN_NUM_THREADS=os.environ.get("LEAN_NUM_THREADS", "2"))
    rc, out, err = run(
        [
            "lake",
            "build",
            f"{name}.Source",
            f"{name}.gen.{contract}.Stage0",
            "SolidCore.Refinement.Generic.Value",
            "SolidCore.Refinement.Generic.StageFrom",
            "SolidCore.Refinement.KernelCopies",
            "SolidCore.Refinement.KernelRfl",
        ],
        cwd=pl,
        env=env,
    )
    if rc != 0:
        raise Failed(
            "the probe's build fails: "
            + " | ".join(l for l in (out + err).splitlines() if l.startswith("error"))[
                :400
            ]
        )
    rc, out, err = run(["lake", "env", "lean", str(d / "Probe.lean")], cwd=pl, env=env)
    lines = [l for l in out.splitlines() if l.startswith("PROBE ")]
    res = {}
    for l in lines:
        what, _, verdict = l[len("PROBE ") :].partition(": ")
        res[what] = verdict
    if rc != 0 or "typecheck" not in res:
        raise Failed(
            "the probe fails: "
            + " | ".join(l for l in (out + err).splitlines() if "error" in l)[:400]
        )
    reasons = []
    if res["typecheck"] != "ok":
        reasons.append(
            {
                "feature": "source type check",
                "where": "",
                "detail": "SolidCore's type checker rejects the source",
            }
        )
    for what, verdict in res.items():
        if what.startswith("fn ") and verdict != "agree":
            reasons.append(
                {
                    "feature": "generic source check",
                    "where": what[3:],
                    "detail": f"{what[3:]}: {verdict[:600]}",
                }
            )
        if what == "constructor" and not verdict.startswith("returns"):
            reasons.append({"feature": "constructor", "where": "", "detail": verdict})
        if what == "constructor" and "with logs" in verdict:
            reasons.append(
                {
                    "feature": "constructor",
                    "where": "",
                    "detail": "the constructor emits logs; the generic stage needs none",
                }
            )
    missing = [f["sig"] for f in fns if f"fn {f['sig']}" not in res]
    if not reasons and (missing or "core" not in res or "storage" not in res):
        raise Failed(
            f"the probe printed no verdict for {missing or 'the core or storage'}"
        )
    shutil.rmtree(pl, ignore_errors=True)
    summary = [
        f"PROBE {k}: {v if k != 'core' else '<' + str(len(v)) + ' chars>'}"
        for k, v in res.items()
    ]
    return res, summary, reasons


# The word SolidCore's core literal spells msg.sender with.
SENDER_ATOM = re.compile(r"(?<![\w.])(?:SolidCore\.Solidity\.Source\.)?Expr\.caller\b")


def caller_route(res, fns):
    """Which callers the generic source stage is about, from the probe's two checks: ("any", [])
    when the check for every caller (PROBE sym fn: the source erased with msg.sender a symbolic
    word, the module's CALLER that word) agrees for every function; otherwise the fixed caller
    0x2000 of the fixed-caller check. A contract that reads msg.sender and fails the check for
    every caller gets no route at all: a module that returns a constant for msg.sender, or reads
    ADDRESS, ORIGIN or CALLER's low bits for it, passes the fixed check and fails this one, so the
    run is BLOCKED (exit 3) naming each refused function. A contract that does not read msg.sender
    keeps the fixed route whatever this check says (its module may read CALLER where the source
    does not; the fixed check alone, at 0x2000, is then what its statement claims). Nothing here
    is trusted: "any" only makes the run state check_sym_stage0, which the kernel decides."""
    sym = {f["sig"]: res.get(f"sym fn {f['sig']}") for f in fns}
    if fns and all(v == "agree" for v in sym.values()):
        return "any", []
    if not SENDER_ATOM.search(res.get("core", "")):
        return "0x2000", []
    return "0x2000", [
        {
            "feature": "msg.sender",
            "where": sig,
            "detail": f"{sig}: the generic check for every caller refuses it ({(v or 'no verdict')[:500]}); "
            "a contract that reads msg.sender needs the check for every caller",
        }
        for sig, v in sym.items()
        if v != "agree"
    ]


def ctx_route(tmp, contract):
    """The gas bound of the checker with contexts and gas (AssembleCtxGas.checkXL) on this run's
    export, for the caller route "any": (bound, None), or (None, why) when the checker cannot
    certify the whole final EVM IR (a block outside its fragment, memory at offsets it does not
    know, or an environment read it does not bind to every caller). Inferred from the probe's
    export, as gas_need is; shape_gen.py infers it again from the run's own export and the kernel
    checks it there (gas_le), so nothing here is trusted."""
    import gen_ctxgas
    import shape_gen

    stages = shape_gen.last_stage((tmp / "Proofs" / "Stages.lean").read_text())
    if stages["first"] != "stage0":
        return None, f"the backend composition starts at {stages['first']}, not stage0"
    try:
        _, info = gen_ctxgas.generate(
            tmp / "gen" / contract, f"probe.gen.{contract}", contract, stages["last"]
        )
    except gen_ctxgas.CtxGasError as e:
        return (
            None,
            f"the checker with contexts and gas: {str(e).removeprefix('error: ')}",
        )
    if info["reads"]:
        return (
            None,
            "the final EVM IR reads environment words the checker with contexts does not bind to the caller",
        )
    if info["F"]:
        return None, f"the checker with contexts leaves blocks {info['F']} unchecked"
    return gen_ctxgas.gas_bound(info, stages["fuel"]), None


def decide_route(res, fns, reasons, witness, probe_dir, contract):
    """The run's caller route: (caller, any_gas, reasons, note). "any" (with the checker with
    contexts' gas bound) only when the probe agreed on the check for every caller for every
    function, the run publishes the Expected statement (a spec witness) and the checker with
    contexts certifies the probe's export; otherwise the fixed caller 0x2000, or, for a contract
    that reads msg.sender and fails the check for every caller, msg.sender reasons (BLOCKED)."""
    if reasons:
        return (
            "0x2000",
            None,
            [],
            "caller route: the fixed caller 0x2000 (the probe refused)",
        )
    caller, creasons = caller_route(res, fns)
    if creasons:
        return "0x2000", None, creasons, "caller route: BLOCKED (msg.sender)"
    if caller != "any":
        return (
            "0x2000",
            None,
            [],
            "caller route: the fixed caller 0x2000 (the check for every caller refuses it)",
        )
    if witness is None:
        why = "no specification, so no published Expected statement to add it to"
    else:
        any_gas, why = ctx_route(probe_dir, contract)
        if why is None:
            return (
                "any",
                any_gas,
                [],
                f"caller route: any (gas bound of the checker with contexts: {any_gas})",
            )
    return "0x2000", None, [], f"caller route: the fixed caller 0x2000 ({why})"


def lifecycle_probe_lean(name, contract, creation, ws):
    """The lifecycle probe (never part of a proof): EthLib's run of the creation code CREATION (the
    init code with the encoded arguments) in the creation frame, and SolidCore's constructor on the
    argument words WS, printed for the driver to compare."""
    byts = ", ".join(str(b) for b in creation)
    return f"""import SolidCore.Refinement.Wide.Refines
import {name}.Source

namespace Certified.{name}.LifeProbe

open SolidCore.Refinement SolidCore.Solidity SolidCore.Solidity.Source EthLib.Base

def fuel : Nat := 4096

def contract? : Except TypeCheck.TypeError TypeCheck.CheckedContract :=
  TypeCheck.CheckedInput.contract Certified.{name}.Src.importedSourceUnit {lean_str(contract)}

def creation : ByteArray := ⟨#[{byts}]⟩

def args : List TypeCheck.CoreValue := ({ws} : List Nat).map Value.word

def hex (l : List Nat) : String :=
  String.join (l.map fun b => (if b < 16 then "0" else "") ++ String.ofList (Nat.toDigits 16 b))

def main : IO Unit := do
  match contract? with
  | .error _ => IO.println "PROBE typecheck: error"
  | .ok c =>
  IO.println "PROBE typecheck: ok"
  let keys : List Nat := match TypeCheck.CheckedContract.constructWithContext? fuel c (Wide.ctxOf c)
      State.empty SolidCore.Refinement.selfAddress 0 args with
    | some (CallResult.returned d _) => d.storage.toList.map (·.1)
    | _ => []
  match TypeCheck.CheckedContract.constructWithContext? fuel c (Wide.ctxOf c) State.empty
      SolidCore.Refinement.selfAddress 0 args with
  | some (CallResult.returned d _) =>
    IO.println ("PROBE constructor: returns" ++ (if d.events.isEmpty then "" else ", with logs"))
    IO.println ("PROBE storage: " ++ reprStr d.storage.toList)
  | some _ => IO.println "PROBE constructor: does not return (reverts or fails)"
  | none => IO.println "PROBE constructor: undefined"
  -- The creation frame of SolidCore.Refinement.Deploy: no call data, no value, the empty storage.
  match Target.run creation ({{ calldata := .empty, callvalue := 0, storage := [] }} : Call) with
  | none => IO.println "PROBE ethlib: none"
  | some o =>
    IO.println ("PROBE ethlib: success " ++ toString o.success)
    IO.println ("PROBE runtime: " ++ hex o.output)
    IO.println ("PROBE ethlib storage: " ++ reprStr (keys.map fun k => (k, o.storage k)))
    IO.println ("PROBE ethlib logs: " ++ toString o.logs.length)

end Certified.{name}.LifeProbe

#eval Certified.{name}.LifeProbe.main
"""


def run_lifecycle_probe(
    lean_dir, work, name, contract, source_text, creation, ws, cache
):
    """The lifecycle probe in a separate clone of the base, as run_probe: nothing it prints is
    trusted, and it only decides whether the stages agree."""
    pl = work / "life-probe"
    git(work, "clone", "-q", "--shared", str(lean_dir), str(pl))
    git(pl, "submodule", "update", "-q", "--init", "--recursive", check=False)
    if cache:
        rc, _, err = run(
            ["cp", "-a", str(pathlib.Path(cache) / ".lake"), str(pl / ".lake")]
        )
        if rc != 0:
            raise Failed(f"cannot copy the probe cache: {err[:200]}")
    fresh_certified(pl, name, contract)
    d = pl / "Certified" / name
    d.mkdir(parents=True)
    (d / "Source.lean").write_text(source_text)
    (d / "LifeProbe.lean").write_text(
        lifecycle_probe_lean(name, contract, creation, ws)
    )
    env = dict(os.environ, LEAN_NUM_THREADS=os.environ.get("LEAN_NUM_THREADS", "2"))
    rc, out, err = run(
        ["lake", "build", f"{name}.Source", "SolidCore.Refinement.Wide.Refines"],
        cwd=pl,
        env=env,
    )
    if rc != 0:
        raise Failed(
            "lifecycle: the probe's build fails: "
            + " | ".join(l for l in (out + err).splitlines() if l.startswith("error"))[
                :400
            ]
        )
    rc, out, err = run(
        ["lake", "env", "lean", str(d / "LifeProbe.lean")], cwd=pl, env=env
    )
    res = {}
    for l in out.splitlines():
        if l.startswith("PROBE "):
            what, _, verdict = l[len("PROBE ") :].partition(": ")
            res[what] = verdict
    if rc != 0 or "typecheck" not in res:
        raise Failed(
            "lifecycle: the probe fails: "
            + " | ".join(l for l in (out + err).splitlines() if "error" in l)[:400]
        )
    shutil.rmtree(pl, ignore_errors=True)
    return res


def lifecycle_stages(res, plain, imm):
    """The lifecycle stages from the probe's result RES and solar's bin-runtime PLAIN, in creation
    order, each {"stage", "ok", "detail"}; the first that fails is the run's REJECTED stage."""
    st = []
    ok = res.get("typecheck") == "ok"
    st.append(
        {
            "stage": "source",
            "ok": ok,
            "detail": "SolidCore's type checker accepts the source"
            if ok
            else "SolidCore's type checker rejects the source",
        }
    )
    eth = res.get("ethlib", "")
    ok = eth == "success true"
    st.append(
        {
            "stage": "initcode",
            "ok": ok,
            "detail": "EthLib runs the init code with the encoded arguments to success"
            if ok
            else f"EthLib's run of the init code with the encoded arguments: {eth or 'no result'}",
        }
    )
    why = (
        runtime_problem(res.get("runtime", ""), plain, imm)
        if ok
        else "no runtime bytes"
    )
    st.append(
        {
            "stage": "runtime bytes",
            "ok": why is None,
            "detail": why
            or (
                "the returned runtime bytes are solar's bin-runtime"
                + (" with its immutables filled" if res.get("runtime") != plain else "")
            ),
        }
    )
    cons = res.get("constructor", "")
    src, tgt = (
        storage_pairs(res.get("storage")),
        storage_pairs(res.get("ethlib storage")),
    )
    if not cons.startswith("returns"):
        why = f"SolidCore's constructor {cons or 'gave no result'}"
    elif "with logs" in cons or res.get("ethlib logs", "0") != "0":
        why = "the constructor emits logs; the lifecycle statement has none"
    elif src != tgt:
        why = f"SolidCore's constructor leaves {src[:4]}, EthLib's run {tgt[:4]}"
    else:
        why = None
    st.append(
        {
            "stage": "storage",
            "ok": why is None,
            "detail": why
            or f"SolidCore's constructor and EthLib's run leave the same {len(src)} slot(s) (compared on SolidCore's slots)",
        }
    )
    return st


# The opcodes the generic creation frame (solidity-lean Verify/Lifecycle/CreationFrame.lean, run)
# evaluates: PUSH0..PUSH32, DUP, SWAP, POP, the stack's one- and two-operand opcodes, CALLVALUE,
# CODESIZE, CODECOPY, MLOAD, MSTORE, SSTORE, JUMP, JUMPI, JUMPDEST and RETURN. REVERT and INVALID
# end the failure paths, which a deployment never runs; a creation that reaches one is no
# deployment, and the frame's check is then false.
FRAME_OPS = set(range(0x5F, 0xA0)) | {
    0x01,
    0x02,
    0x03,
    0x04,
    0x05,
    0x06,
    0x07,
    0x0B,
    0x10,
    0x11,
    0x12,
    0x13,
    0x14,
    0x15,
    0x16,
    0x17,
    0x18,
    0x19,
    0x1A,
    0x1B,
    0x1C,
    0x1D,
    0x34,
    0x38,
    0x39,
    0x50,
    0x51,
    0x52,
    0x55,
    0x56,
    0x57,
    0x5B,
    0xF3,
    0xFD,
    0xFE,
}
OP_NAMES = {
    0x00: "STOP",
    0x08: "ADDMOD",
    0x09: "MULMOD",
    0x0A: "EXP",
    0x20: "KECCAK256",
    0x30: "ADDRESS",
    0x31: "BALANCE",
    0x32: "ORIGIN",
    0x33: "CALLER",
    0x35: "CALLDATALOAD",
    0x36: "CALLDATASIZE",
    0x37: "CALLDATACOPY",
    0x3A: "GASPRICE",
    0x3B: "EXTCODESIZE",
    0x3C: "EXTCODECOPY",
    0x3D: "RETURNDATASIZE",
    0x3E: "RETURNDATACOPY",
    0x3F: "EXTCODEHASH",
    0x40: "BLOCKHASH",
    0x41: "COINBASE",
    0x42: "TIMESTAMP",
    0x43: "NUMBER",
    0x44: "PREVRANDAO",
    0x45: "GASLIMIT",
    0x46: "CHAINID",
    0x47: "SELFBALANCE",
    0x48: "BASEFEE",
    0x49: "BLOBHASH",
    0x4A: "BLOBBASEFEE",
    0x53: "MSTORE8",
    0x54: "SLOAD",
    0x58: "PC",
    0x59: "MSIZE",
    0x5A: "GAS",
    0x5C: "TLOAD",
    0x5D: "TSTORE",
    0x5E: "MCOPY",
    0xA0: "LOG0",
    0xA1: "LOG1",
    0xA2: "LOG2",
    0xA3: "LOG3",
    0xA4: "LOG4",
    0xF0: "CREATE",
    0xF1: "CALL",
    0xF2: "CALLCODE",
    0xF4: "DELEGATECALL",
    0xF5: "CREATE2",
    0xFA: "STATICCALL",
    0xFF: "SELFDESTRUCT",
}


def creation_prefix(init, plain):
    """The constructor's code: the init code before the runtime copy it carries (solar appends
    bin-runtime, with zero placeholders for immutables, to the constructor)."""
    return init[: len(init) - len(plain)] if plain and init.endswith(plain) else init


def frame_outside(prefix):
    """The opcodes of the constructor's code PREFIX the generic creation frame does not evaluate,
    each with its first offset; PUSH data is skipped."""
    found, i = {}, 0
    while i < len(prefix):
        op = prefix[i]
        if op not in FRAME_OPS:
            found.setdefault(OP_NAMES.get(op, f"0x{op:02x}"), i)
        i += 1 + (op - 0x5F if 0x60 <= op <= 0x7F else 0)
    return found


def creation_halt(creation, steps=100000):
    """Where the creation CREATION (init code and argument words) stops, run without value over
    the opcodes of the generic creation frame (FRAME_OPS), as (halt, pc): "return", "revert"
    (REVERT), "invalid" (INVALID, a bad jump or a stack fault), or "outside" (an opcode the frame
    does not evaluate, or more than STEPS steps). It decides nothing the kernel relies on: it
    only lets a constructor whose creation reverts end UNSUPPORTED before any Lean build, since
    REVERT also ends the guard paths of every constructor solar compiles and is not refused where
    the run never reaches it."""
    M, code = 1 << 256, creation
    dests, i = set(), 0
    while i < len(code):
        if code[i] == 0x5B:
            dests.add(i)
        i += 1 + (code[i] - 0x5F if 0x60 <= code[i] <= 0x7F else 0)

    def sgn(x):
        return x - M if x >> 255 else x

    def mem_at(mem, off, n):
        if n and off + n > len(mem):
            mem.extend(bytes(-(-(off + n) // 32) * 32 - len(mem)))

    bin_ops = {
        0x01: lambda a, b: (a + b) % M,
        0x02: lambda a, b: a * b % M,
        0x03: lambda a, b: (a - b) % M,
        0x04: lambda a, b: a // b if b else 0,
        0x06: lambda a, b: a % b if b else 0,
        0x05: lambda a, b: (
            (abs(sgn(a)) // abs(sgn(b)) * (-1 if (sgn(a) < 0) != (sgn(b) < 0) else 1))
            % M
            if b
            else 0
        ),
        0x07: lambda a, b: (
            (abs(sgn(a)) % abs(sgn(b)) * (-1 if sgn(a) < 0 else 1)) % M if b else 0
        ),
        0x0B: lambda a, b: (
            b
            if a >= 31
            else (
                (b | (M - (1 << (8 * a + 8))))
                if (b >> (8 * a + 7)) & 1
                else b & ((1 << (8 * a + 8)) - 1)
            )
        ),
        0x10: lambda a, b: int(a < b),
        0x11: lambda a, b: int(a > b),
        0x12: lambda a, b: int(sgn(a) < sgn(b)),
        0x13: lambda a, b: int(sgn(a) > sgn(b)),
        0x14: lambda a, b: int(a == b),
        0x16: lambda a, b: a & b,
        0x17: lambda a, b: a | b,
        0x18: lambda a, b: a ^ b,
        0x1A: lambda a, b: (b >> (8 * (31 - a))) & 0xFF if a < 32 else 0,
        0x1B: lambda a, b: (b << a) % M if a < 256 else 0,
        0x1C: lambda a, b: b >> a if a < 256 else 0,
        0x1D: lambda a, b: (sgn(b) >> min(a, 256)) % M,
    }
    pc, st, mem = 0, [], bytearray()
    for _ in range(steps):
        if pc >= len(code):
            return "invalid", pc
        op = code[pc]
        try:
            if op == 0xFD:
                return "revert", pc
            if op not in FRAME_OPS or op == 0xFE:
                return ("invalid" if op == 0xFE else "outside"), pc
            if op == 0xF3:
                return "return", pc
            if op in bin_ops:
                a, b = st.pop(), st.pop()
                st.append(bin_ops[op](a, b))
            elif op == 0x15:
                st.append(int(st.pop() == 0))
            elif op == 0x19:
                st.append(M - 1 - st.pop())
            elif op in (0x5F, 0x34):
                st.append(0)
            elif 0x60 <= op <= 0x7F:
                n = op - 0x5F
                st.append(
                    int.from_bytes(code[pc + 1 : pc + 1 + n].ljust(n, b"\0"), "big")
                )
                pc += n
            elif 0x80 <= op <= 0x8F:
                st.append(st[-(op - 0x7F)])
            elif 0x90 <= op <= 0x9F:
                k = op - 0x8E
                st[-1], st[-k] = st[-k], st[-1]
            elif op == 0x38:
                st.append(len(code))
            elif op == 0x50:
                st.pop()
            elif op == 0x5B:
                pass
            elif op == 0x56:
                d = st.pop()
                if d not in dests:
                    return "invalid", pc
                pc = d
                continue
            elif op == 0x57:
                d, c = st.pop(), st.pop()
                if c:
                    if d not in dests:
                        return "invalid", pc
                    pc = d
                    continue
            elif op == 0x51:
                off = st.pop()
                mem_at(mem, off, 32)
                st.append(int.from_bytes(mem[off : off + 32], "big"))
            elif op == 0x52:
                off, v = st.pop(), st.pop()
                mem_at(mem, off, 32)
                mem[off : off + 32] = v.to_bytes(32, "big")
            elif op == 0x39:
                dst, off, n = st.pop(), st.pop(), st.pop()
                mem_at(mem, dst, n)
                mem[dst : dst + n] = (
                    code[off : off + n].ljust(n, b"\0") if off < len(code) else bytes(n)
                )
            elif op == 0x55:
                st.pop(), st.pop()
            if len(st) > 1024:
                return "invalid", pc
        except IndexError, MemoryError, OverflowError:
            return "invalid", pc
        pc += 1
    return "outside", pc


def constructor_payable(ast, contract):
    for d in ast.get("nodes", []):
        if d.get("nodeType") == "ContractDefinition" and d.get("name") == contract:
            for n in d.get("nodes", []):
                if (
                    n.get("nodeType") == "FunctionDefinition"
                    and n.get("kind") == "constructor"
                ):
                    return n.get("stateMutability") == "payable"
    return False


def immutable_patches(deployed, plain):
    """The words a constructor wrote over PLAIN's immutable placeholders to give DEPLOYED, as
    (offset, word) pairs: each run of differing bytes is the low end of one big-endian word.
    None when no such patching gives DEPLOYED (then the run's code is DEPLOYED as a literal)."""
    if len(deployed) != len(plain):
        return None
    out, i = [], 0
    while i < len(plain):
        if deployed[i] == plain[i]:
            i += 1
            continue
        j = i
        while j < len(plain) and deployed[j] != plain[j]:
            j += 1
        start = max(0, j - 32)
        if out and start < out[-1][0] + 32:
            start = out[-1][0] + 32
        out.append((start, int.from_bytes(deployed[start:j], "big")))
        i = j
    got = bytearray(plain)
    for off, w in out:
        got[off : off + 32] = w.to_bytes(32, "big")
    got = bytes(got[: len(plain)])
    return (
        out
        if got == deployed and all(off + 32 <= len(plain) for off, _ in out)
        else None
    )


def lean_bytes(b):
    return (
        f"NatCode.ofNat {int.from_bytes(b, 'big')} {len(b)}" if b else "ByteArray.empty"
    )


def lifecycle_lean(
    ns,
    src_import,
    unit,
    contract,
    sigs,
    init,
    plain,
    deployed,
    ws,
    storage,
    refute=False,
):
    """The lifecycle module of a run (or, with REFUTE, its refutation): solar's init code INIT and
    runtime PLAIN as this run compiled them, the argument words WS the run was given and the storage
    STORAGE of SolidCore's constructor as the probe printed it. Untrusted text: the kernel checks
    every literal. Deploys is Creation.deploys_of_check, one kernel evaluation of the generic
    creation frame; the source half is Route.deployedAtWith by kernel_rfl, as the generic source
    check's DeployedAt is. The runtime code is PLAIN with the immutable words the constructor
    patched in (DEPLOYED's differences), so an unpatched immutable is refuted."""
    patches = immutable_patches(deployed, plain)
    if patches is None:
        code = lean_bytes(deployed)
    else:
        code = "runtime"
        for off, w in patches:
            code = f"patchWord ({code}) {off} {hex(w)}"
    sel = []
    for i, g in enumerate(sigs):
        sel.append(
            f"def selSig{i} : SolidCore.Solidity.Shared.Word :=\n  SolidCore.Solidity.Source.ABI.selectorFromSignature {lean_str(g['sig'])}\n\n"
            f"kernel_rfl selSig{i}_eq selSig{i} {hex(g['sel'])}\n"
        )
    table = "".join(
        f"  if s = {lean_str(g['sig'])} then {hex(g['sel'])}\n  else" for g in sigs
    )
    splits = nested_splits([f"selSig{i}_eq" for i in range(len(sigs))])
    head = f"""import Verify.Lifecycle.CreationFrame
import Verify.Lifecycle.Route
import Verify.Lifecycle.KernelIndex
import SolidCore.Refinement.KernelRfl
import {src_import}

-- Generated by solar's tools/certify/generic.py from this run's init code, arguments and storage;
-- do not edit. Untrusted data and composition: every literal below is checked by the kernel.

namespace {ns}

open EthLib.Base EthLib.Execution EthLib.Execution.Evm EthLib.Execution.Evm.Logic
open SolidCore.Solidity SolidCore.Refinement Verify.Lifecycle Verify.Lifecycle.Creation

/-- The interpreter fuel of SolidCore's runs. -/
def fuel : Nat := 4096

/-- The steps of the generic creation frame's evaluator. -/
def frameFuel : Nat := 4096

/-- solar's init code for §{contract}§, as this run compiled it. -/
def init : ByteArray := {lean_bytes(init)}

/-- solar's runtime code (§bin-runtime§), immutables as placeholders. -/
def runtime : ByteArray := {lean_bytes(plain)}

/-- The runtime code the creation installs: solar's, with each immutable the constructor patches. -/
def code : ByteArray := {code}

/-- The constructor's argument words, as this run was given them. -/
def ws : List Nat := [{", ".join(str(w) for w in ws)}]

/-- What the constructor leaves in storage, as the probe printed it (checked below). -/
def storageLit : List (Nat × Nat) := [{", ".join(f"({k}, {v})" for k, v in storage)}]
"""
    if refute:
        return (
            head
            + f"""
/-- The generic creation frame's refutation of this creation. -/
def refutesB : Bool := refutes frameFuel (initArgs init ws) code storageLit

-- The kernel's evaluation, added as a declaration: a false check is a kernel error, never a
-- reduction outside the kernel.
kernel_decide refutesB_eq refutesB

/-- This creation does not install §code§ with §storageLit§. -/
theorem refuted : ¬ Deploys (initArgs init ws) code storageLit :=
  not_deploys_of_refutes frameFuel refutesB_eq

/-- info: '{ns}.refuted' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms refuted

end {ns}
""".replace("§", "\u0060")
        )
    return (
        head
        + f"""
/-- SolidCore's checked §{contract}§. -/
def contract? : Except TypeCheck.TypeError TypeCheck.CheckedContract :=
  TypeCheck.CheckedInput.contract {unit} {lean_str(contract)}

/-! ## Selectors, each by Keccak-256 once, then by table -/

{chr(10).join(sel)}
/-- §ABI.selectorFromSignature§, with the contract's signatures by table. -/
def selectorTable (s : String) : SolidCore.Solidity.Shared.Word :=
{table} SolidCore.Solidity.Source.ABI.selectorFromSignature s

theorem selector_eq : SolidCore.Solidity.Source.ABI.selectorFromSignature = selectorTable := by
  funext s
  unfold selectorTable
{splits}
/-- The generic creation frame's check of this creation. -/
def checkB : Bool := check frameFuel (initArgs init ws) code storageLit

-- The kernel's evaluation, added as a declaration: a false check is a kernel error, never a
-- reduction outside the kernel.
kernel_decide checkB_eq checkB

/-- EthLib's creation of §init§ with §ws§ installs §code§ and leaves §storageLit§. -/
theorem deploys : Deploys (initArgs init ws) code storageLit :=
  deploys_of_check frameFuel checkB_eq

/-- Whether the contract checks and its constructor, called with §ws§, leaves §storageLit§ and no logs. -/
def life? : Bool :=
  match contract? with
  | .ok c => deployedAtWith fuel c (wordArgs ws) storageLit
  | .error _ => false

kernel_rfl life_eq life? true using selector_eq mkIdx_eq

/-- **The lifecycle statement**: the checked contract's constructor on §ws§ leaves §storageLit§,
and solar's init code with §ws§ deploys §code§ with the same storage. -/
theorem statement : ∃ c, contract? = .ok c ∧ Statement fuel c init code ws storageLit := by
  have h := life_eq
  unfold life? at h
  split at h
  next c hc => exact ⟨c, hc, deploys, deployedAtWith_of h⟩
  next => simp at h

/-- info: '{ns}.statement' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms statement

end {ns}
""".replace("§", "\u0060")
    )


def storage_list(text):
    """The probe's storage entries in the probe's order (SolidCore's d.storage.toList, which
    DeployedAtWith states exactly)."""
    return [(int(k), int(v)) for k, v in re.findall(r"\((\d+), (\d+)\)", text or "")]


def lifecycle_kernel(
    build,
    ns,
    src_import,
    unit,
    contract,
    sigs,
    init,
    plain,
    deployed,
    ws,
    storage,
    outside,
):
    """The kernel stage: {"stage", "ok", "detail", "verdict"}. verdict "proved" when BUILD accepts
    the lifecycle module (Deploys by the generic creation frame, and DeployedAtWith), "refuted"
    when it accepts the refutation instead, "outside" for a constructor outside the frame's shape
    or one neither module proves. BUILD(module, text) is (ok, log)."""
    if outside:
        return {
            "stage": "kernel",
            "ok": False,
            "verdict": "outside",
            "detail": "the constructor is outside the generic creation frame: "
            + ", ".join(outside),
        }
    text = lifecycle_lean(
        ns, src_import, unit, contract, sigs, init, plain, deployed, ws, storage
    )
    ok, out = build("Lifecycle", text)
    if ok:
        return {
            "stage": "kernel",
            "ok": True,
            "verdict": "proved",
            "lean": text,
            "detail": "the kernel proves Deploys (initArgs init ws) code s by the generic creation frame and "
            "∃ d, DeployedAtWith fuel c (wordArgs ws) d s (Verify.Lifecycle.Statement)",
        }
    rtext = lifecycle_lean(
        ns,
        src_import,
        unit,
        contract,
        sigs,
        init,
        plain,
        deployed,
        ws,
        storage,
        refute=True,
    )
    rok, _ = build("LifecycleRefute", rtext)
    if rok:
        return {
            "stage": "kernel",
            "ok": False,
            "verdict": "refuted",
            "lean": rtext,
            "detail": "the kernel proves the creation does not deploy this code and storage (not_deploys_of_refutes)",
        }
    errs = " | ".join(l for l in out.splitlines() if "error" in l)[:400]
    return {
        "stage": "kernel",
        "ok": False,
        "verdict": "outside",
        "detail": f"the kernel proves neither the deployment nor its refutation (a revert path, or a frame outside the shape): {errs}",
    }


def lifecycle_run(
    a, record, solar, src, src_sha, name, contract, ast, work, finish, fns, rec_dir
):
    """--tier lifecycle: the creation stages, ending UNSUPPORTED-lifecycle (exit 4) at the first
    stage with no kernel proof, REJECTED (exit 1) at a stage that disagrees or a creation the
    kernel refutes, CERTIFIED (exit 0) only when the kernel proves the generated lifecycle module."""
    types = constructor_params(ast, contract)
    ws = constructor_words(types, a.constructor_args)
    imm = immutables(ast, contract)
    rc, out, err = run([str(solar), str(src), *OPTIONS, "--emit=bin,bin-runtime"])
    if rc != 0:
        raise Failed(f"solar does not compile the source: {err.strip()[:400]}")
    j = next(
        (
            v
            for k, v in json.loads(out)["contracts"].items()
            if k.rsplit(":", 1)[1] == contract
        ),
        None,
    )
    if not j or not j.get("bin"):
        raise Failed(f"no init code for {contract} in solar's output")
    init, plain = bytes.fromhex(j["bin"]), j.get("bin-runtime", "")
    creation = init + b"".join(w.to_bytes(32, "big") for w in ws)
    halt, at = creation_halt(creation)
    if halt in ("revert", "invalid"):
        record["lifecycle"] = {
            "args": [hex(w) for w in ws],
            "halt": halt,
            "halt_pc": at,
        }
        finish(
            "UNSUPPORTED-lifecycle",
            4,
            f"UNSUPPORTED-lifecycle: kernel: a revert path: the creation with these arguments reaches "
            f"{halt.upper()} at {at}, outside the generic creation frame; nothing is built",
            tier_reached="none",
        )
    life = record["lifecycle"] = {
        "constructor_params": types,
        "args": [hex(w) for w in ws],
        "immutables": imm,
        "initcode_sha256": hashlib.sha256(init).hexdigest(),
        "initcode_size": len(init),
        "creation_sha256": hashlib.sha256(creation).hexdigest(),
        "bin_runtime_sha256": hashlib.sha256(bytes.fromhex(plain)).hexdigest(),
        "statement": "Verify.Lifecycle.Statement fuel c initcode code ws s (solidity-lean Verify/Lifecycle/Route.lean, 03ae48d9)",
    }
    log(
        f"lifecycle: init code {len(init)} bytes, {len(ws)} argument word(s), immutables {imm or 'none'}"
    )
    lean_src = pathlib.Path(a.solidity_lean).resolve()
    base = git(
        lean_src,
        "rev-parse",
        (a.base or (HERE / "solidity-lean.rev").read_text().strip()) + "^{commit}",
    )
    record["solidity_lean"] = {"base": base}
    lean_dir = work / "solidity-lean"
    if lean_dir.exists():
        raise Failed(f"{lean_dir} exists: the work directory must be fresh")
    work.mkdir(parents=True, exist_ok=True)
    git(work, "clone", "-q", "--no-checkout", "--shared", str(lean_src), str(lean_dir))
    git(lean_dir, "checkout", "-q", "--detach", base)
    git(lean_dir, "submodule", "update", "-q", "--init", "--recursive", check=False)
    rc, body, err = frontend(lean_dir, src, src_sha, name, contract, a.solc)
    if rc != 0:
        raise Blocked(
            [
                {
                    "feature": "source frontend",
                    "where": "",
                    "detail": "the solc-AST front end refuses the source: "
                    + err.strip()[-300:],
                }
            ]
        )
    res = run_lifecycle_probe(
        lean_dir,
        work,
        name,
        contract,
        source_lean(body),
        creation,
        ws,
        a.probe_cache or None,
    )
    if res.get("runtime"):
        life["runtime_sha256"] = hashlib.sha256(
            bytes.fromhex(res["runtime"])
        ).hexdigest()
    life["storage"] = storage_pairs(res.get("storage"))
    life["stages"] = stages = lifecycle_stages(res, plain, imm)
    for q in stages:
        log(f"lifecycle {q['stage']}: {'ok' if q['ok'] else 'NO'}: {q['detail']}")
    first = next((q for q in stages if not q["ok"]), None)
    if first is not None:
        finish(
            "REJECTED",
            1,
            f"REJECTED: lifecycle: {first['stage']}: {first['detail']}",
            tier_reached="none",
        )
    plain_b = bytes.fromhex(plain)
    outside = [
        f"{op} at {off}"
        for op, off in frame_outside(creation_prefix(init, plain_b)).items()
    ]
    if constructor_payable(ast, contract):
        outside.append(
            "a payable constructor (the frame runs the creation with no value)"
        )
    if not (lean_dir / "Verify" / "Lifecycle" / "CreationFrame.lean").exists():
        outside.append(
            f"the base {base[:12]} has no generic creation frame (Verify/Lifecycle/CreationFrame.lean)"
        )
    sigs = [{"sig": f["sig"], "sel": f["sel"]} for f in fns]
    kq = lifecycle_kernel(
        lambda mod, t: lifecycle_build(
            lean_dir, name, source_lean(body), mod, t, a.probe_cache or None
        ),
        f"Certified.{name}.Lifecycle",
        f"{name}.Source",
        f"Certified.{name}.Src.importedSourceUnit",
        contract,
        sigs,
        init,
        plain_b,
        bytes.fromhex(res["runtime"]),
        ws,
        storage_list(res.get("storage")),
        outside,
    )
    stages.append({k: kq[k] for k in ("stage", "ok", "detail")})
    log(f"lifecycle kernel: {'YES' if kq['ok'] else 'NO'}: {kq['detail']}")
    if "lean" in kq:
        f = rec_dir / f"{name}.{'Lifecycle' if kq['ok'] else 'LifecycleRefute'}.lean"
        f.write_text(kq["lean"])
        life["lean_sha256"] = hashlib.sha256(kq["lean"].encode()).hexdigest()
    if kq["verdict"] == "proved":
        life["statement"] = (
            f"Certified.{name}.Lifecycle.statement : ∃ c, contract? = .ok c ∧ Verify.Lifecycle.Statement fuel c init code ws storageLit"
        )
        finish(
            "CERTIFIED",
            0,
            f"CERTIFIED: lifecycle: {kq['detail']}",
            tier_reached="lifecycle",
        )
    if kq["verdict"] == "refuted":
        finish(
            "REJECTED",
            1,
            f"REJECTED: lifecycle: kernel: {kq['detail']}",
            tier_reached="none",
        )
    finish(
        "UNSUPPORTED-lifecycle",
        4,
        f"UNSUPPORTED-lifecycle: kernel: {kq['detail']}",
        tier_reached="none",
    )


def lifecycle_build(lean_dir, name, source_text, mod, text, cache):
    """Build the run's module Certified/NAME/MOD.lean (with Source.lean) in the run's clone of the
    base, one Lean thread at a time: (ok, output)."""
    d = lean_dir / "Certified" / name
    d.mkdir(parents=True, exist_ok=True)
    (d / "Source.lean").write_text(source_text)
    (d / f"{mod}.lean").write_text(text)
    lf = lean_dir / "lakefile.lean"
    if f"lean_lib «{name}»" not in lf.read_text():
        with open(lf, "a") as f:
            f.write(
                f'\nlean_lib «{name}» where\n  srcDir := "Certified"\n  globs := #[.submodules `{name}]\n'
            )
    if cache and not (lean_dir / ".lake").exists():
        rc, _, err = run(
            ["cp", "-a", str(pathlib.Path(cache) / ".lake"), str(lean_dir / ".lake")]
        )
        if rc != 0:
            raise Failed(f"cannot copy the probe cache: {err[:200]}")
    env = dict(os.environ, LEAN_NUM_THREADS="1")
    rc, out, err = run(
        ["lake", "build", "-KleanMemory=6000", f"{name}.{mod}"], cwd=lean_dir, env=env
    )
    return rc == 0, out + err


def frontend(lean_dir, src, src_sha, name, contract, solc):
    """The solc-AST front end's Lean body of the source. The front end records the absolute path it
    read the source from (importedSourceName), so it reads a copy at a path fixed by the source's
    digest: the body is then the same wherever the source sits, as a replay from another directory
    needs."""
    fe = pathlib.Path("/tmp/solar-certify-src") / src_sha
    fe.mkdir(parents=True, exist_ok=True)
    shutil.copy(src, fe / f"{name}.sol")
    return run(
        [
            sys.executable,
            str(pathlib.Path(lean_dir) / "scripts/solc_ast_to_lean_source.py"),
            str(fe / f"{name}.sol"),
            "--contract",
            contract,
            "--solc",
            solc,
            "--namespace",
            f"Certified.{name}.Src",
            "--body-only",
        ]
    )


def source_lean(body):
    return (
        "-- Generated by solar's tools/certify/generic.py with scripts/solc_ast_to_lean_source.py; do not edit.\n"
        "\nimport SolidCore.Solidity.Checked\n\nopen SolidCore.Solidity\n\n" + body
    )


def shape_json(
    name, contract, fns, gas, spec_desc, spec_witness=None, events=(), any_gas=None
):
    ns = f"Certified.{name}.Lowering"
    valid = f"SolidCore.Refinement.Shapes.genericValid ({ns}.fns) ({ns}.storageLit)"
    sigs = ", ".join(f["sig"] for f in fns)
    shape = {
        "kind": "compiled",
        "contract": f"{ns}.contract?",
        "fuel": f"{ns}.fuel",
        "valid": valid,
        "gas": f"fun _ => {gas}",
        "env": [],
        "spec": spec_desc,
        "spec_modules": [f"{name}.Lowering", "Verify.ShapeGeneric"],
        "proof_module": f"{name}.Shape",
        "proof": f"Certified.{name}.Shape.published",
        "generic": {
            "lowering": f"{name}.Lowering",
            "ns": ns,
            "style": "core",
            "check": "check_stage0",
            "check_value": "checkValue_stage0",
            "fns": f"{ns}.fns",
            "storage": f"{ns}.storageLit",
        },
        "excluded": [
            "deployment: the init code is not covered; the statement is about the runtime bytecode from any storage that holds the slots the constructor writes, as SolidCore's constructor run with no arguments leaves them",
            "source model trusted: SolidCore's checked contract (the solc AST through scripts/solc_ast_to_lean_source.py and the type checker) is the meaning of the source (frontend trust boundary)",
            "calls: one frame, caller 0x2000, contract at 0x1000, the block and transaction of Target.message; no external calls, GAS reads or return data in the code (the loop checker's fragment)",
            "gas: the bound is the loop checker's (22100 per item times the final EVM IR's block fuel, plus memory), coarse; below it the bytecode may fail without effects, which equivalence admits",
            f"valid calls: {sigs} ABI-encoded with clean argument words, any value below 2^256 (no function is payable: the value reverts on both sides), any storage covering the constructor's slots; other call data is outside the statement",
        ],
        "source_ast": {
            "file": f"Certified/{name}/Source.lean",
            "namespace": f"Certified.{name}.Src",
            "contract": contract,
        },
    }
    if spec_witness is not None:
        # The specification is solar-verify's export of this run's source, proved of SolidCore's
        # contract by tools/certify/spec_connect.py and composed with the bytecode by shape_gen.py:
        # the statement's postcondition and allowed statuses are read off that export, and its
        # non-vacuity witness is a call of SPEC_WITNESS the spec requires to succeed.
        shape.update(
            {
                "kind": "certified",
                "allowed_export": True,
                "spec_export": {
                    "ctx": "SolidCore.Refinement.Shapes.ctxOfCall",
                    "step": "SolidCore.Refinement.Shapes.idStep",
                },
                "nonvacuous": f"Certified.{name}.Shape.nonvacuous",
            }
        )
        shape["spec_modules"] += ["Verify.Spec.TreeMap", "Verify.ShapeSpecExport"]
        shape["generic"]["spec_connect"] = {
            "witness": spec_witness,
            "ghost_fixed": True,
            "using": [f"{ns}.digest_eq"] if events else [],
        }
        shape["excluded"].append(
            "spec: solar-verify's export of the source's NatSpec, with its succeeds-if conditions; events are covered by equivalence"
        )
        # The published statement: the driver-fixed Generic.Expected (tools/certify/final.py).
        shape.update(
            {
                "statement": "expected",
                "source_unit": f"Certified.{name}.Src.importedSourceUnit",
                "source_contract": contract,
                "storage": f"{ns}.storageLit",
                "entries": [
                    {
                        "signature": f["sig"],
                        "fn": f"{ns}.fn{i}",
                        "sel": f["sel"],
                        "arity": len(f["params"]),
                    }
                    for i, f in enumerate(fns)
                ],
            }
        )
    if any_gas is not None:
        # The caller route "any" (caller_route, ctx_route): Lowering.lean proves the check for every
        # caller, shape_gen.py writes NAME.CtxGas and NAME.AnyCaller from this run's export, and
        # final.py adds the equivalence in every real context to the published statement. Only on
        # the published Expected statement, which is where final.py composes it.
        if spec_witness is None:
            raise Failed("the caller route any needs the published Expected statement")
        shape["generic"].update(
            {
                "caller": "any",
                "check_sym": "check_sym_stage0",
                "check_value_sym": "checkValueSym_stage0",
            }
        )
        # The spec from every caller at every value (spec_connect.py --any-caller, composed with
        # the bytecode by shape_gen.py's published_spec_any): when the statement has the spec.
        if "spec_connect" in shape["generic"]:
            shape["generic"]["spec_connect"]["any_caller"] = True
        shape["any_caller"] = {
            "module": f"{name}.AnyCaller",
            "proof": f"Certified.{name}.AnyCaller.published_any",
            "gas": f"fun _ => {any_gas}",
        }
        shape["excluded"].append(
            "callers: the equivalence of the exact bytecode with the source also holds from every caller "
            "below 2^160 other than the contract, at the gas bound of the checker with contexts; the "
            "specification, its allowed statuses and the rest of the statement keep the one frame above"
        )
    return shape


# --- the run ----------------------------------------------------------------------------------


def git(d, *a, check=True):
    rc, out, err = run(["git", "-C", str(d), *a])
    if check and rc != 0:
        raise Failed(f"git {' '.join(a)} in {d}: {err.strip()[:300]}")
    return out.strip()


def build_solar():
    env = dict(os.environ, CARGO_BUILD_JOBS=os.environ.get("CARGO_BUILD_JOBS", "2"))
    rc, _out, err = run(
        ["cargo", "build", "--locked", "--bin", "solar"], cwd=ROOT, env=env
    )
    if rc != 0:
        raise Failed(f"cargo cannot build solar: {err.strip()[-400:]}")
    return ROOT / os.environ.get("CARGO_TARGET_DIR", "target") / "debug" / "solar"


def solar_runtime(solar, src, contract, extra=()):
    rc, out, err = run([str(solar), str(src), *OPTIONS, "--emit=bin-runtime", *extra])
    if rc != 0:
        return None, err
    j = json.loads(out)
    for k, v in j["contracts"].items():
        if k.rsplit(":", 1)[1] == contract:
            return v.get("bin-runtime"), err
    return None, f"no contract {contract} in solar's output"


def gas_need(solar, src, name, contract, tmp):
    """The loop checker's gas bound for this run's export, as shape_gen.py computes it."""
    import asm_cert
    import shape_gen

    gen = tmp / "gen"
    shutil.rmtree(tmp, ignore_errors=True)
    tmp.mkdir(parents=True)
    plain, err = solar_runtime(solar, src, contract, [f"-Zemit-lean-stages={gen}"])
    if plain is None:
        raise Blocked(
            [
                {
                    "feature": "exporter",
                    "where": "",
                    "detail": "the stage exporter rejects the contract (outside the fragment the stages have Lean semantics for): "
                    + err.strip()[:300],
                }
            ]
        )
    rc, _out, err = run(
        [
            sys.executable,
            str(HERE / "certify.py"),
            str(gen / contract),
            "--gen-module",
            f"{name}.gen.{contract}",
            "--proof-dir",
            str(tmp / "Proofs"),
            "--proof-module",
            f"{name}.Proofs",
            "--namespace",
            "SolarTV.Milestone.X",
            "--label",
            name,
            "--loops",
            "0",
            "--heartbeats",
            "2000000",
        ]
    )
    if rc != 0:
        raise Failed(f"certify.py cannot write the stage proofs: {err.strip()[-400:]}")
    stages = shape_gen.last_stage((tmp / "Proofs" / "Stages.lean").read_text())
    last_module = "Evm" + stages["last"][3:]
    last_text = (gen / contract / f"{last_module}.lean").read_text()
    outside = asm_cert.outside(last_text)
    if outside:
        raise Blocked(
            [
                {
                    "feature": "assembly fragment",
                    "where": "",
                    "detail": f"the final EVM IR is outside the loop checker's fragment: {outside}",
                }
            ]
        )
    inferred = asm_cert.infer(last_text, bytes.fromhex(plain))
    bmax = max(inferred["costs"]) if inferred["costs"] else 0
    words = inferred["top"] // 32
    return 22100 * (stages["fuel"] * bmax) + 3 * words + words * words // 512


def loop_resources(record, src, ast, contract):
    """The fixed-fuel resource obligation of a source the envelope blocks on its loop
    (tools/certify/fixed_fuel.py, Verify/Gas/FixedFuel). Out of gas is a resource rejection, apart
    from a revert (a result the bound covers) and from a loop outside the counted shape (which
    stays BLOCKED, with its reason). A bounded obligation is recorded and the run stays BLOCKED:
    the loop still needs its invariant certificate."""
    import fixed_fuel

    try:
        contract = fixed_fuel.contract_of(ast, contract)
        with tempfile.TemporaryDirectory() as tmp:
            ob = fixed_fuel.obligation(build_solar(), src, contract, ast, tmp)
    except fixed_fuel.Unsupported as e:
        record["resource_obligation"] = {
            "kind": "fixed-fuel",
            "verdict": "unsupported",
            "why": str(e),
        }
        return
    record["resource_obligation"] = ob
    log(f"resource obligation (fixed fuel): {fixed_fuel.verdict_line(ob)[1]}")
    if ob["verdict"] == "out-of-gas":
        record["rejected_for"] = "resource: out of gas"
        raise Failed(fixed_fuel.verdict_line(ob)[1][len("REJECTED: ") :])


def source_bound(record):
    """A positive binding verdict only: source_bind.sh's line for this run's own generated source
    AST, its contract and source file, under the pinned solc. An empty, unknown or other value is
    not a binding."""
    name, contract = record.get("name"), record.get("contract")
    v = (record.get("statuses") or {}).get("source")
    if not (isinstance(v, str) and isinstance(name, str) and isinstance(contract, str)):
        return False
    want = (
        f"Certified/{name}/Source.lean is the front end's AST of {contract} in "
        f"Certified/{name}/{name}.sol (solc {SOLC_VERSION})"
    )
    return v == want and record.get("source_ast") == want


def strict(record, tier):
    """The typed strict predicate of the requested tier: each component an explicit boolean read
    from the record's typed fields (the replay receipt's counts, the published statement's
    statuses), never from caveat wording."""
    st = record.get("statuses", {}) or {}
    rp = record.get("replay") or {}
    proved = lambda k: isinstance(st.get(k), str) and st[k].startswith("proved")
    c = {
        "published_statement": bool(record.get("final")),
        "equivalence": proved("equivalence"),
        "termination": proved("termination"),
        "resources": proved("resources"),
        "spec": proved("spec"),
        "source_bound": source_bound(record),
        "provenance_clean": st.get("provenance") == "clean",
        "replay_full": (
            isinstance(rp.get("closure"), int)
            and rp.get("closure", 0) > 0
            and rp.get("replayed_count") == rp.get("closure")
            and rp.get("trusted_count") == 0
            and rp.get("unaccounted") == []
        ),
        "strongest": st.get("strongest") is True,
    }
    need = [
        "published_statement",
        "equivalence",
        "termination",
        "resources",
        "source_bound",
        "provenance_clean",
        "replay_full",
    ]
    if tier in ("functional", "certified"):
        need.append("spec")
    if tier == "certified":
        need.append("strongest")
    return all(c[k] for k in need), {k: c[k] for k in need}, c


REPLAY_KEYS = [
    ("source", "sha256"),
    ("spec",),
    ("options",),
    ("functions",),
    ("environment",),
    ("bytecode_sha256",),
    ("gas_bound",),
    ("requested_tier",),
    ("solidity_lean", "base"),
    ("solidity_lean", "inputs_sha256"),
    ("solar", "commit"),
    ("solar", "binary_sha256"),
    ("solc", "sha256"),
    ("lean_toolchain", "recorded_digest"),
    ("published_statement",),
    ("status",),
    ("strict",),
    ("tier_reached",),
    ("solidity_lean", "executed_tree"),
    ("packages", "digest"),
    ("packages", "cache_sha256"),
]


def dig(d, path):
    for k in path:
        d = d.get(k) if isinstance(d, dict) else None
    return d


# The record's claims, which --replay holds to the hashed certify-record.json (scripts/certify's
# record, in run_files) and to the fresh run: record.json itself is not hashed, so a claim it
# makes is believed only where certify-record.json or the fresh run says the same.
CLAIM_KEYS = ("final", "replay", "statuses", "caveats")
# NOTE: replay.replayed is compared by module, in order, not by olean digest: 28 of the 3966
# oleans of acc4-flag's closure were not bit-identical between its run and its fresh replay.
FRESH_KEYS = [
    ("final",),
    ("statuses",),
    ("replay", "closure"),
    ("replay", "replayed_count"),
    ("replay", "trusted_count"),
    ("replay", "trusted"),
    ("replay", "unaccounted"),
    ("replay", "replayed_beyond_plan"),
    ("replay", "checker", "sha256"),
]


def first_diff(x, y, path):
    """The first path at which x and y differ, or None."""
    if isinstance(x, dict) and isinstance(y, dict):
        for k in sorted(set(x) | set(y), key=str):
            if k not in x or k not in y:
                return f"{path}.{k}"
            if (q := first_diff(x[k], y[k], f"{path}.{k}")) is not None:
                return q
        return None
    if isinstance(x, list) and isinstance(y, list) and len(x) == len(y):
        for i, (u, v) in enumerate(zip(x, y)):
            if (q := first_diff(u, v, f"{path}[{i}]")) is not None:
                return q
        return None
    return None if x == y else path


def reached_tier(comps):
    if comps.get("strongest"):
        return "certified"
    if comps["spec"] and comps["equivalence"]:
        return "functional"
    return "compiled" if comps["equivalence"] and comps["resources"] else "none"


def fresh_caveats(rec):
    """The caveats with the run's own solidity-lean inputs commit named generically: a fresh run
    commits the same inputs at another time, so only that hash may differ between two runs."""
    c = dig(rec, ("solidity_lean", "inputs_commit"))
    cav = rec.get("caveats")
    return (
        [
            x.replace(c, "<inputs commit>") if c and isinstance(x, str) else x
            for x in cav
        ]
        if isinstance(cav, list)
        else cav
    )


def inputs_problem(rec, files):
    """Why the archived inputs/ files are not the generated inputs the record pins
    (solidity_lean.inputs and inputs_sha256, which the fresh run must reproduce), or None."""
    sl = rec.get("solidity_lean") if isinstance(rec.get("solidity_lean"), dict) else {}
    pinned, listed = sl.get("inputs_sha256"), sl.get("inputs")
    if (
        not isinstance(pinned, dict)
        or not isinstance(listed, list)
        or sorted(listed) != sorted(pinned)
    ):
        return "solidity_lean.inputs and inputs_sha256 do not name the same generated inputs"
    archived = {
        r[len("inputs/") :]: v for r, v in files.items() if r.startswith("inputs/")
    }
    for q in sorted(set(archived) | set(pinned)):
        if archived.get(q) != pinned.get(q):
            return f"inputs/{q} is not the generated input solidity_lean.inputs_sha256 pins"
    return None


def inputs_tree(repo, base, files_dir, paths):
    """The git tree of BASE with each repository path of PATHS set to the regular file
    FILES_DIR/PATH: what the run's input commit holds, computed from the base and the listed
    generated files alone, so a replay needs neither that commit nor the run's clone. Objects go to
    a scratch directory, never into REPO."""
    repo = pathlib.Path(repo)
    rc, objs, err = run(
        [
            "git",
            "-C",
            str(repo),
            "rev-parse",
            "--path-format=absolute",
            "--git-common-dir",
        ]
    )
    if rc != 0:
        raise Failed(f"{repo} is not a solidity-lean checkout: {err.strip()[:200]}")
    with tempfile.TemporaryDirectory() as t:
        # git refuses a repository whose object directory does not exist.
        (pathlib.Path(t) / "objects").mkdir()
        env = dict(
            os.environ,
            GIT_INDEX_FILE=str(pathlib.Path(t) / "index"),
            GIT_OBJECT_DIRECTORY=str(pathlib.Path(t) / "objects"),
            GIT_ALTERNATE_OBJECT_DIRECTORIES=str(
                pathlib.Path(objs.strip()) / "objects"
            ),
        )
        rc, _, err = run(["git", "-C", str(repo), "read-tree", base], env=env)
        if rc != 0:
            raise Failed(f"cannot read the base {base} in {repo}: {err.strip()[:200]}")
        for q in paths:
            f = pathlib.Path(files_dir) / q
            if f.is_symlink() or not f.is_file():
                raise Failed(f"the generated input {q} is not a regular file")
            rc, blob, err = run(
                ["git", "-C", str(repo), "hash-object", "-w", "--no-filters", str(f)],
                env=env,
            )
            if rc == 0:
                rc, _, err = run(
                    [
                        "git",
                        "-C",
                        str(repo),
                        "update-index",
                        "--add",
                        "--cacheinfo",
                        f"100644,{blob.strip()},{q}",
                    ],
                    env=env,
                )
            if rc != 0:
                raise Failed(f"cannot add {q} to the base's tree: {err.strip()[:200]}")
        rc, tree, err = run(["git", "-C", str(repo), "write-tree"], env=env)
        if rc != 0:
            raise Failed(f"cannot write the executed tree: {err.strip()[:200]}")
    return tree.strip()


def pinned_packages(repo, rev):
    """The toolchain and package set a solidity-lean commit pins: its lean-toolchain and each git
    package of its lake-manifest.json by name and revision, and that set's digest."""
    try:
        man = json.loads(git(repo, "show", f"{rev}:lake-manifest.json"))
    except ValueError as e:
        raise Failed(f"{rev}:lake-manifest.json is not JSON: {e}") from e
    pin = {
        "lean-toolchain": git(repo, "show", f"{rev}:lean-toolchain"),
        "packages": sorted(
            [q["name"], q["rev"]]
            for q in man.get("packages", [])
            if q.get("type") == "git"
        ),
    }
    return pin, hashlib.sha256(
        json.dumps(pin, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def packages_problem(pk, pin):
    """Why the package cache PK (a .lake/packages) is not the pinned set PIN, or None: exactly the
    pinned git packages, each its own checkout at its pinned revision with no edited or untracked
    file. Ignored files (build outputs) are not git's to judge: cache_manifest and foreign_oleans
    hold them."""
    pk = pathlib.Path(pk)
    if not pk.is_dir():
        return f"{pk} is not a package cache"
    want = dict(pin["packages"])
    have = {q.name for q in pk.iterdir()}
    if have != set(want):
        return f"the cache's packages are not the pinned set (extra {sorted(have - set(want))}, missing {sorted(set(want) - have)})"
    for n, rev in sorted(want.items()):
        rc, top, _ = run(["git", "-C", str(pk / n), "rev-parse", "--show-toplevel"])
        if rc != 0 or pathlib.Path(top.strip()).resolve() != (pk / n).resolve():
            return f"package {n} is not a git checkout of its own"
        rc, head, _ = run(["git", "-C", str(pk / n), "rev-parse", "HEAD"])
        if rc != 0 or head.strip() != rev:
            return f"package {n} is at {head.strip()[:12] or 'no commit'}, not the pinned {rev[:12]}"
        rc, st, _ = run(
            ["git", "-C", str(pk / n), "status", "--porcelain", "--untracked-files=all"]
        )
        if rc != 0 or st.strip():
            return f"package {n} has edited or untracked files: {st.strip().splitlines()[:3]}"
    return None


def foreign_oleans(pk, pin):
    """The oleans of the cache's packages with no source module at the package's pinned revision
    (PACKAGE/.lake/build/lib/lean/M.olean without a tracked M.lean): nothing the pinned set builds."""
    pk = pathlib.Path(pk)
    bad = []
    for n, _ in pin["packages"]:
        lib = pk / n / ".lake" / "build" / "lib" / "lean"
        if not lib.is_dir():
            continue
        rc, ls, _ = run(["git", "-C", str(pk / n), "ls-files"])
        if rc != 0:
            bad.append(f"{n}: cannot list its sources")
            continue
        srcs = set(ls.splitlines())
        for q in sorted(lib.rglob("*.olean")):
            mod = str(q.relative_to(lib))[: -len(".olean")]
            if f"{mod}.lean" not in srcs:
                bad.append(f"{n}/{mod}.olean")
    return bad


def file_digest(q):
    h = hashlib.sha256()
    with open(q, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def cache_manifest(pk):
    """Every file of a package cache outside its packages' .git directories, as sha256sum lines
    sorted by path: the sources and every build output (olean, ilean, trace) a run starts from, so
    a file added to, removed from or changed in the cache after the run is a difference."""
    pk = pathlib.Path(pk)
    rows = []
    for top, dirs, names in os.walk(pk):
        dirs[:] = sorted(x for x in dirs if x != ".git")
        for n in names:
            q = pathlib.Path(top) / n
            if q.is_symlink():
                h = hashlib.sha256(("symlink:" + os.readlink(q)).encode()).hexdigest()
            elif q.is_file():
                h = file_digest(q)
            else:
                continue
            rows.append((f"./{q.relative_to(pk)}", h))
    return "".join(f"{h}  {rel}\n" for rel, h in sorted(rows))


def manifest_diff(old, new):
    """Paths added, removed and changed between two cache manifests."""
    a = {l[66:]: l[:64] for l in old.splitlines() if l}
    b = {l[66:]: l[:64] for l in new.splitlines() if l}
    return (
        sorted(set(b) - set(a)),
        sorted(set(a) - set(b)),
        sorted(k for k in set(a) & set(b) if a[k] != b[k]),
    )


def executed_problem(rec, d, repo):
    """Why the record's executed solidity-lean tree is not its base with the listed generated inputs
    (the archived inputs/ files) and nothing else, rebuilt from REPO, or None."""
    sl = rec.get("solidity_lean") if isinstance(rec.get("solidity_lean"), dict) else {}
    want, base, paths = sl.get("executed_tree"), sl.get("base"), sl.get("inputs")
    if not (isinstance(want, str) and re.fullmatch(r"[0-9a-f]{40}", want)):
        return "the record pins no executed tree (solidity_lean.executed_tree), so the solidity-lean tree it ran is unchecked"
    if not (isinstance(base, str) and isinstance(paths, list)):
        return "the record names no base and generated inputs for its executed tree"
    if not (repo and pathlib.Path(repo).is_dir()):
        return f"no solidity-lean checkout at {repo!r} to rebuild the executed tree from its base"
    try:
        got = inputs_tree(repo, base, d / "inputs", paths)
        base_tree = git(repo, "rev-parse", base + "^{tree}")
    except Failed as e:
        return f"cannot rebuild the executed tree: {e}"
    if sl.get("base_tree") != base_tree:
        return f"solidity_lean.base_tree is not the tree of base {base[:12]}"
    if got != want:
        return f"the executed tree {want[:12]} is not base {base[:12]} + the listed generated inputs (rebuilt {got[:12]})"
    return None


def packages_replay_problem(rec, d, repo, cache):
    """Why the record's packages are not its base's pinned toolchain and package set, or the
    --packages-from cache of this replay not the run's (its manifest, packages-cache.sha256), or
    None."""
    pk = rec.get("packages") if isinstance(rec.get("packages"), dict) else {}
    base = dig(rec, ("solidity_lean", "base"))
    if not pk.get("digest"):
        return "packages: the record pins no package set (packages.digest)"
    try:
        pin, digest = pinned_packages(repo, base)
    except Failed as e:
        return f"packages: cannot read base's pinned package set: {e}"
    if pk.get("digest") != digest or pk.get("pinned") != pin:
        return f"packages: the record's package set is not base {str(base)[:12]}'s pinned toolchain and packages"
    if not cache:
        return (
            "packages: the run started from a package cache; replay with --packages-from"
            if pk.get("cache_sha256")
            else None
        )
    if not pk.get("cache_sha256"):
        return "packages: the record pins no package cache manifest, so the --packages-from cache cannot be checked against the run's"
    c = pathlib.Path(cache) / ".lake" / "packages"
    if (why := packages_problem(c, pin)) is not None:
        return f"packages: {why}"
    if bad := foreign_oleans(c, pin):
        return f"packages: {len(bad)} olean(s) of the cache have no source at their package's pinned revision: {bad[:3]}"
    try:
        old = (d / "packages-cache.sha256").read_text()
    except OSError:
        return "packages: the record's cache manifest packages-cache.sha256 is missing"
    if hashlib.sha256(old.encode()).hexdigest() != pk["cache_sha256"]:
        return "packages: packages-cache.sha256 is not the manifest packages.cache_sha256 pins"
    new = cache_manifest(c)
    if new != old:
        added, removed, changed = manifest_diff(old, new)
        first = (added or removed or changed)[:3]
        return (
            f"packages: the --packages-from cache is not the run's: {len(added)} added, {len(removed)} removed, "
            f"{len(changed)} changed (first {first})"
        )
    return None


def replayed_modules(rec):
    return [
        m.get("module") if isinstance(m, dict) else m
        for m in (dig(rec, ("replay", "replayed")) or [])
    ]


def claims_problem(rec, d):
    """Why record.json's claims are not certify-record.json's (or its verdict not the one strict
    gives that record), or None."""
    if "certify-record.json" not in rec.get("run_files", {}):
        claimed = [
            k
            for k in (*CLAIM_KEYS, "strict", "tier_reached", "published_statement")
            if rec.get(k)
        ]
        return (
            f"the record claims {', '.join(claimed)} without a certify-record.json"
            if claimed
            else None
        )
    try:
        cr = json.loads((d / "certify-record.json").read_text())
    except (OSError, ValueError) as e:
        return f"certify-record.json is unreadable: {e}"
    for k in CLAIM_KEYS:
        if (q := first_diff(rec.get(k), cr.get(k), k)) is not None:
            return f"{q} differs between record.json and certify-record.json"
    if (
        q := first_diff(
            rec.get("published_statement"),
            cr.get("end_to_end_statement"),
            "published_statement",
        )
    ) is not None:
        return f"{q} differs between record.json and certify-record.json (end_to_end_statement)"
    tier = rec.get("requested_tier")
    if tier not in ("compiled", "functional", "certified"):
        return f"requested_tier {tier!r} is not a tier"
    ok, need_c, comps = strict(
        dict(cr, name=rec.get("name"), contract=rec.get("contract")), tier
    )
    want = {
        "strict": {"tier": tier, "holds": ok, "components": need_c},
        "tier_reached": reached_tier(comps),
        "status": tier.upper() if ok else "REJECTED",
    }
    for k, v in want.items():
        if (q := first_diff(rec.get(k), v, k)) is not None:
            return f"{q} is not what certify-record.json gives (record {str(rec.get(k))[:120]}, expected {str(v)[:120]})"
    return None


def replay_main(a):
    """--replay DIR: refuse a finished run's directory unless its files are the record's and a
    fresh, cold run from its source reproduces every pin of the record."""

    def out(code, line):
        print(line, flush=True)
        sys.exit(code)

    d = pathlib.Path(a.replay).resolve()
    try:
        rec = json.loads((d / "record.json").read_text())
    except (OSError, ValueError) as e:
        out(1, f"REJECTED: replay: no readable record.json in {d}: {e}")
    if rec.get("status") == "BLOCKED":
        out(1, "REJECTED: replay: the record is BLOCKED: nothing to replay")
    files = rec.get("run_files")
    if not isinstance(files, dict) or not files:
        out(1, "REJECTED: replay: the record lists no run files")
    for rel, sha in sorted(files.items()):
        f = d / rel
        if not f.is_file() or sha256_file(f) != sha:
            out(
                1,
                f"REJECTED: replay: {rel} differs from the record (stale or edited run file)",
            )
    # Every file and link of the directory, anywhere under it, is the record or in its manifest.
    on_disk = {
        str(q.relative_to(d)) for q in d.rglob("*") if q.is_file() or q.is_symlink()
    }
    extra = sorted(on_disk - set(files) - {"record.json"})
    if extra:
        out(
            1,
            f"REJECTED: replay: files the record's manifest does not list: {extra[:3]}",
        )
    srcs = [r for r in files if r.endswith(".sol") and "/" not in r]
    if len(srcs) != 1 or files[srcs[0]] != dig(rec, ("source", "sha256")):
        out(1, "REJECTED: replay: the record's source digest is not its source file's")
    if (why := inputs_problem(rec, files)) is not None:
        out(1, f"REJECTED: replay: {why}")
    if (why := claims_problem(rec, d)) is not None:
        out(1, f"REJECTED: replay: {why}")
    repo = a.solidity_lean or rec.get("solidity_lean_source", "")
    if (why := executed_problem(rec, d, repo)) is not None:
        out(1, f"REJECTED: replay: {why}")
    if (why := packages_replay_problem(rec, d, repo, a.packages_from)) is not None:
        out(1, f"REJECTED: replay: {why}")
    stamp = datetime.datetime.now(datetime.UTC).strftime("%Y%m%dT%H%M%SZ")
    fresh_dir = d.parent / f"{d.name}.replay-{stamp}"
    work = (
        pathlib.Path(a.work).resolve()
        if a.work
        else d.parent / f"{d.name}-replay-work-{stamp}"
    )
    cmd = [
        sys.executable,
        str(HERE / "generic.py"),
        str(d / srcs[0]),
        dig(rec, ("spec", "kind")) or "none",
        "--tier",
        rec.get("requested_tier", "certified"),
        "--solidity-lean",
        a.solidity_lean or rec.get("solidity_lean_source", ""),
        "--base",
        dig(rec, ("solidity_lean", "base")) or "",
        "--work",
        str(work),
        "--record-dir",
        str(fresh_dir),
    ]
    if rec.get("contract"):
        cmd += ["--contract", rec["contract"]]
    for flag, val in (
        ("--packages-from", a.packages_from),
        ("--probe-cache", a.probe_cache),
    ):
        if val:
            cmd += [flag, val]
    print(f"== replay: {' '.join(cmd)}", flush=True)
    p = subprocess.run(cmd, check=False)
    try:
        new = json.loads((fresh_dir / "record.json").read_text())
    except OSError, ValueError:
        out(1, f"REJECTED: replay: the fresh run wrote no record (exit {p.returncode})")
    for path in REPLAY_KEYS:
        if dig(rec, path) != dig(new, path):
            out(
                1,
                f"REJECTED: replay: {'.'.join(path)} differs between the record and the fresh run "
                f"(record {str(dig(rec, path))[:120]}, fresh {str(dig(new, path))[:120]})",
            )
    for path in FRESH_KEYS:
        if (
            q := first_diff(dig(rec, path), dig(new, path), ".".join(path))
        ) is not None:
            out(
                1,
                f"REJECTED: replay: {q} differs between the record and the fresh run "
                f"(record {str(dig(rec, path))[:120]}, fresh {str(dig(new, path))[:120]})",
            )
    if (q := first_diff(fresh_caveats(rec), fresh_caveats(new), "caveats")) is not None:
        out(
            1,
            f"REJECTED: replay: {q} differs between the record and the fresh run "
            f"(record {str(fresh_caveats(rec))[:120]}, fresh {str(fresh_caveats(new))[:120]})",
        )
    if replayed_modules(rec) != replayed_modules(new):
        out(
            1,
            "REJECTED: replay: replay.replayed (its modules) differs between the record and the fresh run",
        )
    last = new.get("status", "")
    out(
        p.returncode,
        f"{last}: replay of {d}: every pin reproduced by a fresh cold run ({fresh_dir / 'record.json'})"
        if p.returncode == 0
        else f"REJECTED: replay: the fresh run's verdict is {last} (exit {p.returncode})",
    )


def main():
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("source", nargs="?")
    ap.add_argument("spec", nargs="?", help="natspec or none")
    ap.add_argument(
        "--replay",
        metavar="DIR",
        help="check a finished run's directory against a fresh cold run",
    )
    ap.add_argument("--invariants")
    ap.add_argument("--contract")
    ap.add_argument(
        "--tier", choices=["compiled", "functional", "certified", "lifecycle"]
    )
    ap.add_argument(
        "--solidity-lean", help="a solidity-lean git checkout to clone the base from"
    )
    ap.add_argument(
        "--base",
        help="the solidity-lean base commit (default tools/certify/solidity-lean.rev)",
    )
    ap.add_argument(
        "--constructor-args",
        help="--tier lifecycle: the constructor's static argument words, comma-separated",
    )
    ap.add_argument("--work", help="a fresh work directory")
    ap.add_argument("--record-dir")
    ap.add_argument(
        "--packages-from",
        help="a checkout whose .lake/packages to copy (still replayed in full)",
    )
    ap.add_argument(
        "--solc",
        default=os.environ.get(
            "SOLC", str(pathlib.Path.home() / ".cache/solc" / f"solc-{SOLC_VERSION}")
        ),
    )
    ap.add_argument("--no-negatives", action="store_true")
    ap.add_argument(
        "--probe-cache",
        help="a checkout whose .lake the probe clone starts from (lake rebuilds what differs)",
    )
    ap.add_argument(
        "--prepare-only",
        action="store_true",
        help="stop after the input commit (development)",
    )
    a = ap.parse_args()
    if a.replay:
        replay_main(a)
    if not (a.source and a.spec and a.solidity_lean and a.work and a.record_dir):
        ap.error(
            "SOURCE, SPEC, --solidity-lean, --work and --record-dir are required (or --replay DIR)"
        )

    src = pathlib.Path(a.source).resolve()
    rec_dir = pathlib.Path(a.record_dir).resolve()
    rec_dir.mkdir(parents=True, exist_ok=True)
    work = pathlib.Path(a.work).resolve()
    if a.spec not in ("natspec", "none"):
        raise SystemExit("certify: SPEC must be natspec or none")
    tier = a.tier or ("certified" if a.spec == "natspec" else "compiled")
    if a.constructor_args is not None and tier != "lifecycle":
        ap.error("--constructor-args needs --tier lifecycle")
    text = src.read_text()
    src_sha = hashlib.sha256(text.encode()).hexdigest()
    base_name = re.sub(r"[^a-z0-9_]", "_", src.stem.lower()).strip("_") or "c"
    if not base_name[0].isalpha():
        base_name = "c_" + base_name
    name = f"{base_name}_{src_sha[:8]}"
    record = {
        "schema": "solar-certify-generic/1",
        "name": name,
        "started": datetime.datetime.now(datetime.UTC).isoformat(timespec="seconds"),
        "command": [str(HERE / "certify"), *sys.argv[1:]],
        "requested_tier": tier,
        "source": {"file": str(src), "sha256": src_sha},
        "spec": {"kind": a.spec, "sha256": src_sha if a.spec == "natspec" else None},
        "invariants": {
            "file": a.invariants,
            "sha256": sha256_file(a.invariants) if a.invariants else None,
        },
        "options": OPTIONS,
        "solar": {
            "commit": git(ROOT, "rev-parse", "HEAD"),
            "dirty": bool(
                git(
                    ROOT,
                    "status",
                    "--porcelain",
                    "--",
                    "crates",
                    "scripts",
                    "tools/certify",
                    "Cargo.toml",
                    "Cargo.lock",
                )
            ),
        },
        "solc": {
            "version": SOLC_VERSION,
            "path": a.solc,
            "sha256": sha256_file(a.solc) if os.path.exists(a.solc) else None,
        },
        "lean_toolchain": {
            "recorded_digest": (HERE / "toolchain.sha256").read_text().strip()
        },
    }

    def finish(status, code, line, **extra):
        record.update(extra)
        # The manifest: every file the driver leaves in the record directory but record.json.
        if record.get("run_files") is not None:
            for q in sorted(rec_dir.rglob("*")):
                rel = str(q.relative_to(rec_dir))
                if (
                    q.is_file()
                    and rel != "record.json"
                    and rel not in record["run_files"]
                ):
                    record["run_files"][rel] = sha256_file(q)
        record["status"] = status
        record["finished"] = datetime.datetime.now(datetime.UTC).isoformat(
            timespec="seconds"
        )
        (rec_dir / "record.json").write_text(json.dumps(record, indent=2) + "\n")
        log(line)
        sys.exit(code)

    try:
        log(f"== generic: {src} ({src_sha}) as {name}, requested tier {tier}")
        ast, contracts = solc_ast(a.solc, src)
        try:
            contract, fns, events = envelope(
                text,
                ast,
                contracts,
                a.contract,
                a.invariants,
                a.spec == "natspec",
                tier == "lifecycle",
            )
        except Blocked as b:
            if any(r["feature"] == "loop" for r in b.reasons):
                loop_resources(record, src, ast, a.contract)
            raise
        record["contract"] = contract
        record["functions"] = [
            {
                "signature": f["sig"],
                "selector": f"0x{f['sel']:08x}",
                "params": f["params"],
            }
            for f in fns
        ]
        record["environment"] = ENVIRONMENT
        log(f"envelope (source): inside; functions {', '.join(f['sig'] for f in fns)}")
        witness = None
        if a.spec == "natspec" and tier != "lifecycle":
            spec_desc_natspec = "solar-verify's export of the source's NatSpec in this run, read by the published statement (bundlePost, bundleAllowed)"
            # The statement's non-vacuity needs a call the spec requires to succeed: a function with
            # @custom:succeeds-if. Without one the allowed statuses reject nothing, and the certified
            # tier cannot be stated non-vacuously: the run is refused before any proof.
            witness = next(
                (f["name"] for f in fns if f.get("mutates") and f.get("succeeds")), None
            ) or next((f["name"] for f in fns if f.get("succeeds")), None)
            if witness is None:
                raise Failed(
                    "spec: no function has @custom:succeeds-if, so the spec's allowed statuses reject nothing and "
                    "the certified statement's non-vacuity (a revert the spec rejects) cannot hold; add succeeds-if "
                    "conditions, or run with SPEC none"
                )
        log("== generic: build solar")
        solar = build_solar()
        record["solar"]["binary_sha256"] = sha256_file(solar)
        rc, vout, _ = run([str(solar), "--version"])
        record["solar"]["version"] = vout.strip().splitlines()[0] if vout else ""
        # The commit the binary embeds (its build's), recorded apart from the driver's checkout.
        m = re.search(r"Commit SHA: ([0-9a-f]{40})", vout or "")
        record["solar"]["binary_commit"] = m.group(1) if m else None
        if tier == "lifecycle":
            lifecycle_run(
                a,
                record,
                solar,
                src,
                src_sha,
                name,
                contract,
                ast,
                work,
                finish,
                fns,
                rec_dir,
            )
        plain, err = solar_runtime(solar, src, contract)
        if plain is None:
            raise Failed(f"solar does not compile the source: {err.strip()[:400]}")
        bad = scan_bytecode(plain)
        if bad:
            raise Blocked(
                [
                    {
                        "feature": "bytecode",
                        "where": f"pc {pc}",
                        "detail": f"the runtime bytecode has {op}",
                    }
                    for op, pc in bad.items()
                ]
            )
        record["bytecode_sha256"] = hashlib.sha256(bytes.fromhex(plain)).hexdigest()
        log("envelope (bytecode): no call, creation, gas or return-data opcode")
        need = gas_need(solar, src, name, contract, work / "probe")
        record["gas_bound"] = need
        log(f"gas bound (the loop checker's, from this export): {need}")
        # The source AST through the solidity-lean front end; a refusal is outside the envelope.
        lean_src = pathlib.Path(a.solidity_lean).resolve()
        base = a.base or (HERE / "solidity-lean.rev").read_text().strip()
        base = git(lean_src, "rev-parse", base + "^{commit}")
        lean_dir = work / "solidity-lean"
        if lean_dir.exists():
            raise Failed(f"{lean_dir} exists: the work directory must be fresh")
        git(
            work,
            "clone",
            "-q",
            "--no-checkout",
            "--shared",
            str(lean_src),
            str(lean_dir),
        ) if work.exists() else None
        if not lean_dir.exists():
            work.mkdir(parents=True)
            git(
                work,
                "clone",
                "-q",
                "--no-checkout",
                "--shared",
                str(lean_src),
                str(lean_dir),
            )
        git(lean_dir, "checkout", "-q", "--detach", base)
        git(lean_dir, "submodule", "update", "-q", "--init", "--recursive", check=False)
        rc, body, err = frontend(lean_dir, src, src_sha, name, contract, a.solc)
        if rc != 0:
            raise Blocked(
                [
                    {
                        "feature": "source frontend",
                        "where": "",
                        "detail": "the solc-AST front end refuses the source: "
                        + err.strip()[-300:],
                    }
                ]
            )
        d = lean_dir / "Certified" / name
        source_text = source_lean(body)
        log(
            "== generic: probe SolidCore's evaluation and the generic check (separate clone)"
        )
        res, summary, reasons = run_probe(
            lean_dir, work, name, contract, fns, source_text, a.probe_cache or None
        )
        # The caller route: a msg.sender contract the check for every caller refuses fails closed.
        caller, any_gas, creasons, note = decide_route(
            res, fns, reasons, witness, work / "probe", contract
        )
        reasons += creasons
        record["probe"] = summary
        for l in summary:
            log(l)
        if reasons:
            raise Blocked(reasons)
        log(note)
        if caller == "any":
            # Only on the any route: the fixed route's record is the fixed-frame record.
            record["caller"] = caller
        if fresh_certified(lean_dir, name, contract):
            # The base already holds this source's record: regenerated here in full, never reused.
            record["base_held_record"] = True
            log(f"base holds Certified/{name}: removed in this run's clone and regenerated in full")
        d.mkdir(parents=True)
        shutil.copy(src, d / f"{name}.sol")
        (d / "options").write_text("\n".join(OPTIONS) + "\n")
        (d / "Source.lean").write_text(source_text)
        (d / "Lowering.lean").write_text(
            lowering_lean(
                name,
                contract,
                fns,
                events,
                res["core"],
                res["storage"],
                sym=caller == "any",
            )
        )
        # A placeholder of the proof module, which tools/certify/shape_gen.py regenerates in the run
        # (scripts/certify lists the statement's modules before step 6b writes it): its imports
        # are the generated module's, and nothing in it is read.
        (d / "Shape.lean").write_text(
            "".join(
                f"import {m}\n"
                for m in (
                    "Verify.ShapeGeneric",
                    f"{name}.Lowering",
                    f"{name}.Proofs.Stages",
                    f"{name}.gen.{contract}.Bytecode",
                )
            )
            + "\n-- Placeholder: tools/certify/shape_gen.py writes this module in the run.\n"
        )
        spec_desc = (
            "none: no specification requested"
            if a.spec == "none"
            else spec_desc_natspec
        )
        (d / "shape.json").write_text(
            json.dumps(
                shape_json(
                    name, contract, fns, need, spec_desc, witness, events, any_gas
                ),
                indent=2,
            )
            + "\n"
        )
        git(lean_dir, "add", "-A", "Certified", "lakefile.lean")
        git(
            lean_dir,
            "-c",
            "user.name=solar certify",
            "-c",
            "user.email=certify@localhost",
            "commit",
            "-q",
            "-m",
            f"certify: generated inputs for {name}",
        )
        inputs = git(lean_dir, "rev-parse", "HEAD")
        changed = git(lean_dir, "diff", "--name-only", base, inputs).splitlines()
        stray = [
            p
            for p in changed
            if not (p.startswith(f"Certified/{name}/") or p == "lakefile.lean")
        ]
        if stray:
            raise Failed(f"the input commit touches {stray}")
        record["solidity_lean_source"] = str(lean_src)
        record["solidity_lean"] = {
            "base": base,
            "inputs_commit": inputs,
            "inputs": changed,
            "inputs_sha256": {p: sha256_file(lean_dir / p) for p in changed},
        }
        # The run's directory keeps the source and every generated input, for --replay to check.
        shutil.copy(src, rec_dir / src.name)
        for q in changed:
            (rec_dir / "inputs" / q).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(lean_dir / q, rec_dir / "inputs" / q)
        record["run_files"] = {
            src.name: src_sha,
            **{f"inputs/{q}": sha256_file(lean_dir / q) for q in changed},
        }
        # The executed tree: the input commit's tree must be the base's with exactly the listed
        # generated inputs, which --replay rebuilds from the base and the archived inputs/ alone.
        tree = git(lean_dir, "rev-parse", inputs + "^{tree}")
        rebuilt = inputs_tree(lean_dir, base, rec_dir / "inputs", changed)
        if rebuilt != tree:
            raise Failed(
                f"executed tree: the input commit's tree {tree} is not base + the listed generated inputs ({rebuilt})"
            )
        record["solidity_lean"].update(
            base_tree=git(lean_dir, "rev-parse", base + "^{tree}"), executed_tree=tree
        )
        pin, pin_digest = pinned_packages(lean_dir, base)
        record["packages"] = {"pinned": pin, "digest": pin_digest, "cache_sha256": None}
        if a.prepare_only:
            finish("PREPARED", 1, f"PREPARED: {lean_dir} at {inputs} (no proof run)")
        if a.packages_from:
            pk = pathlib.Path(a.packages_from) / ".lake" / "packages"
            if (why := packages_problem(pk, pin)) is not None:
                raise Failed(f"packages: {why}")
            if bad := foreign_oleans(pk, pin):
                raise Failed(
                    f"packages: {len(bad)} olean(s) of the cache have no source at their package's pinned revision: {bad[:3]}"
                )
            man = cache_manifest(pk)
            (lean_dir / ".lake").mkdir(exist_ok=True)
            rc, _, err = run(
                ["cp", "-a", str(pk), str(lean_dir / ".lake" / "packages")]
            )
            if rc != 0:
                raise Failed(f"cannot copy the packages: {err[:200]}")
            if cache_manifest(lean_dir / ".lake" / "packages") != man:
                raise Failed("packages: the cache changed while it was copied")
            (rec_dir / "packages-cache.sha256").write_text(man)
            record["run_files"]["packages-cache.sha256"] = sha256_file(
                rec_dir / "packages-cache.sha256"
            )
            record["packages_from"] = str(pk)
            record["packages"]["cache_sha256"] = hashlib.sha256(
                man.encode()
            ).hexdigest()
        log(f"== generic: solidity-lean {base} + inputs {inputs}; scripts/certify")
        # scripts/certify builds solar itself from this checkout (no SOLAR, no CERTIFY_SKIP_BUILD: a
        # binary it is handed is a provenance caveat whatever its digest), and must get the binary
        # the driver built and compiled the source with: CERTIFY_SOLAR_SHA256 is that binary's digest,
        # so a different build is a caveat too.
        env = dict(
            os.environ,
            SOLIDITY_LEAN_DIR=str(lean_dir),
            CERTIFY_ALLOW_UNPINNED="1",
            CERTIFY_CLEAN_BUILD="1",
            SOLC=a.solc,
            CERTIFY_SOLAR_SHA256=record["solar"]["binary_sha256"],
            LEAN_MEMORY=LEAN_MEMORY,
        )
        for k in (
            "CERTIFY_REPLAY_TRUST_UPSTREAM",
            "SOLAR",
            "CERTIFY_SKIP_BUILD",
            "CERTIFY_ALLOW_FOREIGN_SOLAR",
        ):
            env.pop(k, None)
        cmd = [
            "bash",
            str(ROOT / "scripts" / "certify"),
            str(d / f"{name}.sol"),
            "--name",
            name,
            "--contract",
            contract,
            "--require-final",
        ]
        if a.no_negatives:
            cmd.append("--no-negatives")
        record["scripts_certify"] = {
            "command": cmd,
            "env": {
                k: env[k]
                for k in (
                    "SOLIDITY_LEAN_DIR",
                    "CERTIFY_ALLOW_UNPINNED",
                    "CERTIFY_CLEAN_BUILD",
                    "SOLC",
                    "CERTIFY_SOLAR_SHA256",
                    "LEAN_MEMORY",
                )
            },
        }
        logf = rec_dir / "certify.log"
        rc = stream(cmd, logf, env=env)
        record["scripts_certify"]["exit"] = rc
        record["lean_toolchain"]["lean-toolchain"] = (
            (lean_dir / "lean-toolchain").read_text().strip()
        )
        inner = d / "record.json"
        if rc != 0 or not inner.exists():
            lines = logf.read_text().strip().splitlines()
            tail = lines[-3:]
            hit = resource_failures(lines)
            spec_rej = spec_rejection(lines)
            if spec_rej and not hit:
                # spec_connect.py elaborated its module and a theorem of it does not check: the spec
                # is false of the code (or beyond the tactics), named by that theorem.
                finish("REJECTED", 1, spec_rej[0][:600])
            if hit or rc in (137, -9):
                # Not a refutation: a build ran out of memory, time or heartbeats.
                record["resources_exhausted"] = {
                    "count": len(hit),
                    "first": hit[:3],
                    "exit": rc,
                    "lean_memory": LEAN_MEMORY,
                }
                finish(
                    "REJECTED",
                    1,
                    f"REJECTED: resources: {len(hit)} resource failure(s) in the build, not a "
                    f"proof failure (LEAN_MEMORY={LEAN_MEMORY}): {(hit or tail)[0][:300]}",
                )
            finish(
                "REJECTED",
                1,
                f"REJECTED: scripts/certify: exits {rc}: {' | '.join(tail)}",
            )
        rec = json.loads(inner.read_text())
        shutil.copy(inner, rec_dir / "certify-record.json")
        # What scripts/certify ran: the input commit, with only this run's own directory changed
        # by the run, and the pinned packages unedited.
        ran = git(lean_dir, "rev-parse", "HEAD")
        moved = [
            l[3:]
            for l in git(
                lean_dir, "status", "--porcelain", "--untracked-files=all"
            ).splitlines()
            if l
        ]
        outside = [q for q in moved if not q.startswith(f"Certified/{name}/")]
        if ran != inputs or rec.get("solidity_lean") != inputs or outside:
            finish(
                "REJECTED",
                1,
                f"REJECTED: executed tree: scripts/certify ran at {rec.get('solidity_lean')} (checkout {ran}), "
                f"not the input commit {inputs}, or changed files outside Certified/{name}/: {outside[:3]}",
            )
        if (why := packages_problem(lean_dir / ".lake" / "packages", pin)) is not None:
            finish("REJECTED", 1, f"REJECTED: packages: after the run: {why}")
        ok, need_c, comps = strict(dict(rec, name=name, contract=contract), tier)
        reached = reached_tier(comps)
        res = {
            "statuses": rec.get("statuses"),
            "strict": {"tier": tier, "holds": ok, "components": need_c},
            "tier_reached": reached,
            "replay": rec.get("replay"),
            "caveats": rec.get("caveats"),
            "published_statement": rec.get("end_to_end_statement"),
            "final": rec.get("final"),
        }
        if ok:
            finish(
                tier.upper(),
                0,
                f"{tier.upper()}: {name}: the requested tier holds (record {rec_dir / 'record.json'})",
                **res,
            )
        missing = [k for k, v in need_c.items() if not v]
        finish(
            "REJECTED",
            2,
            f"REJECTED: strict tier: {name} reached {reached}, below the requested tier {tier} (false: {', '.join(missing)})",
            **res,
        )
    except Blocked as b:
        finish("BLOCKED", 3, f"BLOCKED: {b}", blocked=b.reasons)
    except Failed as f:
        finish("REJECTED", 1, f"REJECTED: {f}")
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as e:
        finish("REJECTED", 1, f"REJECTED: driver: {type(e).__name__}: {e}")


if __name__ == "__main__":
    main()
