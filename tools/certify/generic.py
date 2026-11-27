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
import asm_cert
import export_ns
import keccak
import lakefile_inputs

SOLC_VERSION = "0.8.35"
ENVIRONMENT = "one frame, caller 0x2000, contract at 0x1000, the block and transaction of Target.message (fixed, not quantified)"
# The record's environment on the caller route "any" (AnyCaller's conjuncts quantify the caller;
# the Expected conjunct keeps the fixed frame).
ENVIRONMENT_ANY = (
    "caller quantified (every caller below 2^160 other than the contract) for the any-caller "
    "conjuncts; the contract address, block and transaction fixed; the Expected conjunct uses "
    "the fixed frame caller 0x2000, contract 0x1000"
)
# The excluded "calls" entry: the fixed frame, and on the caller route "any" the callers it covers.
CALLS_FIXED = (
    "calls: one frame, caller 0x2000, contract at 0x1000, the block and transaction of "
    "Target.message; no external calls, GAS reads or return data in the code (the loop "
    "checker's fragment)"
)
CALLS_ANY = (
    "calls: one frame; the equivalence, the non-payable revert and the specification hold from "
    "every caller below 2^160 other than the contract (ctx.Ok), with the contract address and "
    "the block and transaction fixed; no external calls, GAS reads or return data in the code "
    "(the loop checker's fragment)"
)


def callers_any(any_gas):
    """The excluded "callers" entry on the caller route "any", at the checker's gas bound."""
    return (
        "callers: from every caller below 2^160 other than the contract, at the gas bound of the "
        f"checker with contexts ({any_gas}): the exact bytecode runs as the source, a call with "
        "a value other than 0 reverts with empty output, no logs and unchanged storage, and every "
        "result meets the exported specification's allowed statuses and postcondition read in "
        "that caller's context (partial correctness); the Expected conjunct keeps the fixed frame"
    )


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


def linearization(ast, cdef):
    """CDEF's bases in solc's C3 order, most derived first (CDEF itself first), as
    (contract definitions, None), or (None, why) when the driver cannot certify the order: a base
    it cannot find, a cycle, C3 failing, or an order other than solc's linearizedBaseContracts. The
    order is computed here as solc does (NameAndTypeResolver: CDEF, then the merge of its direct
    bases' linearizations, last-listed base first, with the reversed list of direct bases), the same
    computation as the Lean frontend's ContractDecl.dispatchOrder?, and must equal the order solc
    reports for the contract it compiled; Lean then checks its own order against solc's in the
    kernel (linearization_module's linearization_eq, a module of its own that
    scripts/certify regenerates, builds and audits)."""
    byid = {
        n["id"]: n
        for n in walk(ast)
        if n.get("nodeType") == "ContractDefinition" and "id" in n
    }
    names = [d.get("name") for d in byid.values()]
    if len(set(names)) != len(names):
        return (
            None,
            "two contracts share a name, and the Lean frontend finds bases by name",
        )
    memo = {}

    def merge(seqs):
        seqs = [list(q) for q in seqs if q]
        out = []
        while seqs:
            for q in seqs:
                head = q[0]
                if not any(head in r[1:] for r in seqs):
                    break
            else:
                return None
            out.append(head)
            seqs = [[x for x in r if x != head] for r in seqs]
            seqs = [r for r in seqs if r]
        return out

    def lin(cid, stack):
        if cid in memo:
            return memo[cid]
        if cid in stack:
            raise ValueError("the inheritance graph has a cycle")
        c = byid[cid]
        bases = []
        for b in c.get("baseContracts", []):
            ref = (b.get("baseName") or {}).get("referencedDeclaration")
            if ref not in byid:
                raise ValueError(f"{c.get('name')}: a base outside the source unit")
            bases.append(ref)
        rev = bases[::-1]
        m = merge([lin(b, stack | {cid}) for b in rev] + [rev])
        if m is None:
            raise ValueError(f"{c.get('name')}: no C3 linearization of its bases")
        memo[cid] = [cid] + m
        return memo[cid]

    try:
        order = lin(cdef["id"], frozenset())
    except ValueError as e:
        return None, str(e)
    solc = cdef.get("linearizedBaseContracts")
    if solc != order:

        def shown(ids):
            return ", ".join(
                byid[i].get("name", "?") if i in byid else str(i) for i in ids or []
            )

        return None, (
            f"the C3 order {shown(order)} is not the order solc reports ({shown(solc)}): "
            "refused rather than flattened by either"
        )
    return [byid[i] for i in order], None


class Dispatch:
    """Internal calls resolved as they run in the most-derived contract of LIN (its contracts, most
    derived first), as solc's resolveVirtual does: an unqualified call or modifier invocation runs
    the first function (same name and parameter types) or modifier (same name) in LIN, super.f() in
    contract X the first such function after X in LIN (whether or not it overrides the one the call
    names), and a qualified C.f() or C.m a static call of the one named. Callables are every
    function and modifier of LIN's contracts, by id."""

    def __init__(self, lin):
        self.lin = lin
        self.callables, self.home, self.pos = {}, {}, {}
        for i, c in enumerate(lin):
            self.pos[c["id"]] = i
            for n in walk(c):
                if (
                    n.get("nodeType") in ("FunctionDefinition", "ModifierDefinition")
                    and "id" in n
                ):
                    self.callables[n["id"]] = n
                    self.home[n["id"]] = i
        self.bases = {}
        for k, n in self.callables.items():
            self.bases[k] = set(n.get("baseFunctions") or []) | set(
                n.get("baseModifiers") or []
            )
        changed = True
        while changed:
            changed = False
            for k in self.bases:
                more = (
                    set().union(*(self.bases.get(b, set()) for b in self.bases[k]))
                    - self.bases[k]
                )
                if more:
                    self.bases[k] |= more
                    changed = True

    @staticmethod
    def params(n):
        # Parameter types without data locations (solc compares asExternallyCallableFunction's).
        out = []
        for q in (n.get("parameters") or {}).get("parameters", []):
            t = (q.get("typeDescriptions") or {}).get("typeString", "")
            for loc in (" storage pointer", " storage ref", " memory", " calldata"):
                t = t.replace(loc, "")
            out.append(t)
        return out

    def first_after(self, ref, after):
        want = self.callables[ref]
        kind = want.get("nodeType")
        for c in self.lin[after:]:
            for n in c.get("nodes", []):
                if (
                    n.get("id") in self.callables
                    and n.get("nodeType") == kind
                    and n.get("kind", "function") in ("function", None)
                    and n.get("name") == want.get("name")
                    and (
                        kind == "ModifierDefinition"
                        or self.params(n) == self.params(want)
                    )
                ):
                    return n["id"]
        return None

    def target(self, ref, node=None, site=None):
        """The callable a call or invocation naming REF runs, or None when REF is not one. NODE is the
        call's callee expression or the invocation's modifier name, SITE the id of the callable
        the call is in."""
        if ref not in self.callables:
            return None
        e = node or {}
        if (
            e.get("nodeType") == "MemberAccess"
            and (e.get("expression") or {}).get("nodeType") == "Identifier"
            and (e.get("expression") or {}).get("name") == "super"
            and site in self.home
        ):
            return self.first_after(ref, self.home[site] + 1)
        if e.get("nodeType") == "MemberAccess" or (
            e.get("nodeType") == "IdentifierPath" and "." in (e.get("name") or "")
        ):
            return ref  # C.f() or C.m: a static call of the named function or modifier
        return self.first_after(ref, 0) or ref


def creation_reach(cdef, lin=None, extra=()):
    """The AST nodes creation can run in CDEF: the constructor (with its modifiers and base
    constructor invocations), every state variable's initializer, and, through internal calls and
    modifier invocations, every function and modifier they reach, those of CDEF and the inlined
    sibling code EXTRA (sibling_code's library and free functions). Returns (the ids of those nodes,
    whether some call or invocation could not be resolved to a function or modifier of CDEF or
    EXTRA, in which case nothing counts as runtime-only). With LIN (CDEF's linearization, most
    derived first), every base's constructor and initializers are roots too, and calls resolve as
    they run in CDEF (Dispatch)."""
    lin = lin or [cdef]
    disp = Dispatch(lin)
    defs = dict(disp.callables)
    for n in (m for f in extra for m in walk(f)):
        if n.get("nodeType") in ("FunctionDefinition", "ModifierDefinition") and "id" in n:
            defs.setdefault(n["id"], n)

    def resolve(ref, node, site):
        # The callable of LIN a call runs (Dispatch), else the inlined sibling function it names.
        k = disp.target(ref, node, site)
        return k if k is not None else (ref if ref in defs else None)
    roots = [
        (n, n["id"])
        for c in lin
        for n in walk(c)
        if n.get("nodeType") == "FunctionDefinition" and n.get("kind") == "constructor"
    ]
    roots += [
        (n["value"], None)
        for c in lin
        for n in c.get("nodes", [])
        if n.get("nodeType") == "VariableDeclaration"
        and n.get("stateVariable")
        and n.get("value")
    ]
    bases = {c["id"] for c in lin}
    unresolved = False
    seen, todo, reach = set(), list(roots), set()
    while todo:
        node, site = todo.pop()
        if id(node) in seen:
            continue
        seen.add(id(node))
        for m in walk(node):
            reach.add(id(m))
            t = m.get("nodeType")
            if t == "ModifierInvocation":
                ref = (m.get("modifierName") or {}).get("referencedDeclaration")
                k = resolve(ref, m.get("modifierName"), site)
                if k is not None:
                    todo.append((defs[k], k))
                elif ref in bases:
                    pass  # a base constructor specifier: every base constructor is a root
                else:
                    unresolved = True  # an unknown modifier
            elif t == "FunctionCall" and m.get("kind") == "functionCall":
                e = m.get("expression") or {}
                ref = e.get("referencedDeclaration")
                k = resolve(ref, e, site)
                if k is not None:
                    todo.append((defs[k], k))
                elif isinstance(ref, int) and ref < 0:
                    pass  # a builtin (require, keccak256, ...)
                elif " internal" in str(
                    (e.get("typeDescriptions") or {}).get("typeString", "")
                ):
                    unresolved = True  # an internal call not to a known function (a function value)
    return reach, unresolved


def sibling_code(ast, cdef, lin=None):
    """The code of the source's other units that CDEF runs inline, and the calls that would run a
    sibling's deployed code instead. A source may hold other contracts, interfaces and libraries
    beside CDEF: only what CDEF reaches is part of it. CDEF reaches a library's internal (or
    private) function and a free function by naming it (a call, a function value, a bound
    `using` function, or a user-defined operator): solar inlines that code, as it compiles
    CDEF's own internal functions, so the envelope checks it as CDEF's code. Nothing else of a
    sibling is compiled into CDEF's runtime bytecode. Returns (the reached function and modifier
    definitions, closed under what they reach in turn, each with the unit that declares it, None for
    a free function; the call sites that run a public or external library function, by DELEGATECALL
    into the deployed library, each as (call node, library name, function name)). With LIN (CDEF's
    linearization, most derived first), its bases' code is CDEF's own and reaches siblings too."""
    owner = {}
    for unit in ast.get("nodes", []):
        if unit.get("nodeType") == "ContractDefinition":
            for n in walk(unit):
                if n.get("nodeType") in ("FunctionDefinition", "ModifierDefinition"):
                    owner[n["id"]] = (n, unit)
        elif unit.get("nodeType") == "FunctionDefinition":
            owner[unit["id"]] = (unit, None)
    lin = lin or [cdef]
    own = {n["id"] for c in lin for n in walk(c) if "id" in n}

    def inlined(ref, here, expr):
        """Whether REF (a declaration a node in unit HERE names, through EXPR when it is a callee)
        is sibling code CDEF runs inline."""
        if ref not in owner or ref in own:
            return False
        node, unit = owner[ref]
        if unit is None:
            return True
        if unit.get("contractKind") != "library":
            return False  # a function of another contract or interface: an external call, or a base
        if node.get("visibility") in ("internal", "private") or node.get(
            "nodeType"
        ) == "ModifierDefinition":
            return True
        # A library's public function runs inline only when the library itself calls it by name.
        return unit is here and (expr or {}).get("nodeType") == "Identifier"

    reached, delegate, seen = [], [], set()
    todo = [(c, c) for c in lin]
    while todo:
        root, here = todo.pop()
        for n in walk(root):
            t = n.get("nodeType")
            refs = []
            if t == "FunctionCall" and n.get("kind") == "functionCall":
                e = n.get("expression") or {}
                ref = e.get("referencedDeclaration")
                if ref in owner and ref not in own:
                    node, unit = owner[ref]
                    if (
                        unit is not None
                        and unit.get("contractKind") == "library"
                        and node.get("visibility") in ("public", "external")
                        and not inlined(ref, here, e)
                    ):
                        delegate.append((n, unit.get("name"), node.get("name")))
            elif t in ("BinaryOperation", "UnaryOperation") and isinstance(
                n.get("function"), int
            ):
                refs.append((n["function"], None))
            if isinstance(n.get("referencedDeclaration"), int) and t in (
                "Identifier",
                "MemberAccess",
                "IdentifierPath",
            ):
                refs.append((n["referencedDeclaration"], n))
            for ref, expr in refs:
                if ref in seen or not inlined(ref, here, expr):
                    continue
                seen.add(ref)
                node, unit = owner[ref]
                reached.append((node, unit))
                todo.append((node, unit))
    return reached, delegate


def envelope(
    text,
    ast,
    contracts,
    contract,
    invariants,
    natspec=False,
    lifecycle=False,
    open_out=None,
):
    """The source envelope. With LIFECYCLE (--tier lifecycle, which states only the creation), a
    loop outside the constructor is not a block: it is appended to OPEN_OUT and the runtime's
    obligations for it (the fixed-fuel resource obligation among them) are recorded OPEN, never
    discharged; a loop in the constructor still blocks."""
    reasons = []
    ctor_ps, ctor_node = None, None

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
    # Inheritance: the contract is checked as the Lean frontend flattens it (CheckedInput.contract
    # translates the most-derived contract over its C3 order: functions with overrides resolved,
    # storage laid out base first, super calls as internal calls), so the checks below walk every
    # contract of the order. The order must be solc's (linearization), and what the flattening does
    # not cover yet is refused by name.
    lin, why = linearization(ast, cdef)
    if lin is None:
        block("inheritance", cdef, why)
        lin = [cdef]
    for c in lin:
        for spec in c.get("baseContracts", []):
            if spec.get("arguments"):
                block(
                    "inheritance: base constructor arguments",
                    spec,
                    f"{c.get('name')} passes arguments to the base constructor "
                    f"{(spec.get('baseName') or {}).get('name')}: base constructor arguments are not handled yet",
                )
        for n in c.get("nodes", []):
            if (
                n.get("nodeType") != "FunctionDefinition"
                or n.get("kind") != "constructor"
            ):
                continue
            for m in n.get("modifiers", []):
                ref = (m.get("modifierName") or {}).get("referencedDeclaration")
                if m.get("kind") == "baseConstructorSpecifier" or ref in {
                    x["id"] for x in lin
                }:
                    if m.get("arguments"):
                        block(
                            "inheritance: base constructor arguments",
                            m,
                            f"{c.get('name')}'s constructor passes arguments to the base constructor "
                            f"{(m.get('modifierName') or {}).get('name')}: base constructor arguments are not handled yet",
                        )
                elif c is not cdef:
                    block(
                        "inheritance: modifier on a base constructor",
                        m,
                        f"the constructor of base {c.get('name')} has the modifier "
                        f"{(m.get('modifierName') or {}).get('name')}: modifiers on base constructors are not handled",
                    )
    # Only the named contract is certified: its siblings (other contracts, interfaces, libraries)
    # are compiled into its bytecode only as the library and free functions it (or a base in its
    # linearization) runs inline (sibling_code), which are checked below as its own code.
    inline, delegate = sibling_code(ast, cdef, lin)
    for call, lib, fname in delegate:
        block(
            "external call",
            call,
            f"{lib}.{fname} is a public library function: the call runs the deployed library {lib} "
            "by DELEGATECALL, a sibling's code outside the certified contract",
        )
    inline_defs = [node for node, _ in inline]
    own = {n["id"] for c in lin for n in walk(c) if "id" in n}
    import loop_cuts

    # Each loop's function and why the source erasure (eraseLoopStmt) certainly does not cover it,
    # or None, by node.
    loop_why = {
        id(n): (fname, why) for c in lin for n, fname, why in loop_cuts.loop_shapes(c)
    }
    for node, unit in inline:
        where = f"{unit.get('name')}." if unit is not None else ""
        for n in walk(node):
            if n.get("nodeType") in ("ForStatement", "WhileStatement", "DoWhileStatement"):
                loop_why[id(n)] = (
                    f"{where}{node.get('name')}",
                    "a loop in inlined library or free function code, which the loop cut hook "
                    "does not read",
                )
    creation, creation_unresolved = creation_reach(cdef, lin, inline_defs)
    enums = {}
    for n in walk(ast):
        if n.get("nodeType") == "EnumDefinition":
            enums[n["name"]] = len(n.get("members", []))
            enums[n.get("canonicalName", n["name"])] = len(n.get("members", []))
    for n in [*(m for c in lin for m in walk(c)), *(m for node in inline_defs for m in walk(node))]:
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
        elif t in ("ForStatement", "WhileStatement", "DoWhileStatement") and (
            lifecycle
            and open_out is not None
            and not creation_unresolved
            and id(n) not in creation
        ):
            open_out.append(
                {
                    "feature": "loop",
                    "where": line_of(text, n),
                    "detail": "a runtime loop, outside the lifecycle tier's statement: its fixed-fuel "
                    "resource obligation and loop proofs are OPEN, not required here",
                }
            )
        elif t in ("ForStatement", "WhileStatement", "DoWhileStatement"):
            fname, why = loop_why.get(id(n), ("", "a loop outside every function"))
            block(
                "loop",
                n,
                (
                    f"{fname}: outside the source erasure's counted loop: {why}"
                    if why is not None
                    else f"{fname}: a while loop, the while route's shape (eraseWhile through an "
                    "invariant; bodyRuns_of_eraseWhile): the loop cut hook generates its source "
                    "premises and obligations, and the published statement does not apply them yet"
                    if t == "WhileStatement"
                    else f"{fname}: a counted for loop, the source erasure's shape (eraseLoopStmt; "
                    "bodyRuns_of_eraseLoop): the loop cut hook generates its source premises and "
                    "obligations, and the published statement does not apply them yet"
                )
                + (
                    "; the supplied invariants file is not consumed"
                    if invariants
                    else ""
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
                ctor_ps, ctor_why = ctor_word_params(n)
                for why in ctor_why:
                    block("constructor parameters", n, why)
                if not ctor_why:
                    ctor_node = n
            if n.get("kind") in ("receive", "fallback"):
                block(
                    "fallback",
                    n,
                    f"{n.get('kind')} function: calls outside the ABI's functions",
                )
        elif t == "MemberAccess" and n.get("memberName") in (
            "creationCode",
            "runtimeCode",
        ):
            block(
                "contract creation",
                n,
                f"type(...).{n['memberName']} embeds another contract's bytecode",
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
    # Recursion: the generic stage proofs need a cut point at every back edge. One function calling
    # itself, with its measure written (@custom:decreases), has them: the backend stages cut it at
    # its call frames (rec_cuts.py), bounded by the measure. Any other cycle has no cut points at
    # its call sites, so it stays refused.
    variables = {
        n["id"]
        for n in walk(ast)
        if n.get("nodeType") == "VariableDeclaration" and "id" in n
    }
    cycles, values = recursion_cycles(cdef, variables, lin, inline_defs)
    for names, call in cycles:
        fns = [
            n
            for n in walk(cdef)
            if n.get("nodeType") == "FunctionDefinition" and n.get("name") == names[0]
        ]
        doc = ((fns[0].get("documentation") or {}).get("text", "")) if len(fns) == 1 else ""
        if len(names) == 1 and re.search(r"@custom:decreases\s+\S", doc):
            continue
        block(
            "recursion",
            call,
            f"unsupported: {' and '.join(names)} can call "
            f"{'itself' if len(names) == 1 else 'each other'} "
            + (
                "(only one function calling itself, with @custom:decreases, has cut points)"
                if len(names) > 1
                else "(no @custom:decreases measure for its recursive call sites)"
            ),
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
        for n in (m for c in lin for m in walk(c)):
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
        # The definition the selector dispatches to: the most derived one in the C3 order.
        fsel = keccak.keccak256(sig.encode())[:4].hex()
        fdef = next(
            (
                n
                for c in lin
                for n in c.get("nodes", [])
                if n.get("nodeType") == "FunctionDefinition"
                and n.get("kind") == "function"
                and n.get("functionSelector") == fsel
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
    fns = Fns(fns)
    if ctor_node is not None:
        # A constructor with static word parameters: the statement is for every argument list
        # (Verify/Generic/CtorWords.lean), on the compiled tier only, and the runtime bytes must
        # not depend on the arguments.
        if natspec:
            block(
                "constructor parameters",
                ctor_node,
                "with a specification: the spec connection (SpecConnect) and the Expected statement run "
                "the constructor with no arguments; constructor arguments are stated for every word "
                "on the compiled tier (no specification) only",
            )
        for v in immutables(ast, contract):
            block(
                "constructor parameters",
                ctor_node,
                f"immutable {v} with constructor arguments: the runtime bytes the constructor deploys "
                "depend on the arguments, and the statement binds one runtime bytecode",
            )
        fns.ctor = {"params": ctor_ps}
    if reasons:
        raise Blocked(reasons)
    return contract, fns, events


# Decided NatSpec checks (natspec_checks): a small fragment of spec clauses the driver can evaluate
# exactly, so a clause it refutes is REJECTED by name before any proof. A clause outside the
# fragment is left to the proofs (or recorded OPEN), never accepted on the driver's word.
SPEC_ATOM = re.compile(
    r"^\(?\s*([A-Za-z_]\w*|\d+)\s*(<=|>=|==|!=|<|>)\s*([A-Za-z_]\w*|\d+)\s*\)?$"
)
SPEC_FLIP = {"<": ">", ">": "<", "<=": ">=", ">=": "<=", "==": "==", "!=": "!="}


def spec_atoms(clause):
    """The conjuncts of CLAUSE as (name, op, value) with the name on the left, or None when CLAUSE
    is not a conjunction (&&) of comparisons between one name and a decimal literal."""
    if "||" in clause or "!" in clause.replace("!=", ""):
        return None
    out = []
    for part in clause.split("&&"):
        m = SPEC_ATOM.match(part.strip())
        if not m:
            return None
        a, op, b = m.groups()
        if a.isdigit() and not b.isdigit():
            a, op, b = b, SPEC_FLIP[op], a
        if a.isdigit() or not b.isdigit():
            return None
        out.append((a, op, int(b)))
    return out


def spec_holds(atoms, env):
    """Whether every atom holds in ENV (a name to value map), or None when an atom's name is not in ENV."""
    ops = {
        "<": int.__lt__,
        ">": int.__gt__,
        "<=": int.__le__,
        ">=": int.__ge__,
        "==": int.__eq__,
        "!=": int.__ne__,
    }
    for n, op, v in atoms:
        if n not in env:
            return None
        if not ops[op](env[n], v):
            return False
    return True


def natspec_clauses(node, tag):
    doc = node.get("documentation") or {}
    doc = doc.get("text", "") if isinstance(doc, dict) else str(doc)
    return [
        l.split(f"@custom:{tag}", 1)[1].strip()
        for l in doc.splitlines()
        if f"@custom:{tag}" in l
    ]


def literal_assignments(body):
    """The state assignments of BODY when it is straight-line NAME = DECIMAL statements only (an
    empty body included), as a map, else None."""
    out = {}
    for st in (body or {}).get("statements", []):
        e = (
            st.get("expression")
            if st.get("nodeType") == "ExpressionStatement"
            else None
        )
        if not (e and e.get("nodeType") == "Assignment" and e.get("operator") == "="):
            return None
        lhs, rhs = e.get("leftHandSide", {}), e.get("rightHandSide", {})
        if (
            lhs.get("nodeType") != "Identifier"
            or rhs.get("nodeType") != "Literal"
            or rhs.get("kind") != "number"
        ):
            return None
        try:
            out[lhs["name"]] = int(rhs["value"], 0)
        except KeyError, ValueError:
            return None
    return out


def vacuous_requires(f):
    """Why F's @custom:requires clauses, taken together, are unsatisfiable, or None: decided only for
    comparisons of one argument or state variable with decimal literals, over the uint range."""
    atoms = []
    for text in natspec_clauses(f, "requires"):
        a = spec_atoms(text)
        if a is None:
            return None
        atoms += a
    if not atoms:
        return None
    for n in {x for x, _, _ in atoms}:
        lo, hi, ne = 0, 2**256 - 1, set()
        for x, op, v in atoms:
            if x != n:
                continue
            if op == "<":
                hi = min(hi, v - 1)
            elif op == "<=":
                hi = min(hi, v)
            elif op == ">":
                lo = max(lo, v + 1)
            elif op == ">=":
                lo = max(lo, v)
            elif op == "==":
                lo, hi = max(lo, v), min(hi, v)
            else:
                ne.add(v)
        if lo > hi or (hi - lo < len(ne) and all(k in ne for k in range(lo, hi + 1))):
            clauses = " && ".join(natspec_clauses(f, "requires"))
            return (
                f"vacuous: {f['name']}: @custom:requires {clauses} admits no value of {n}, so "
                "the function's statement would hold of no call"
            )
    return None


def natspec_checks(ast, contract):
    """The decided NatSpec checks of CONTRACT: (the first refutation as a REJECTED reason or None,
    the record's invariant entries). @custom:invariant: the initial state (state variables'
    literal initializers, else 0, then a straight-line literal constructor) must satisfy it, and so
    must the state each straight-line literal function body leaves; an invariant neither refuted
    nor in the fragment is recorded OPEN (the driver proves no invariant). @custom:succeeds-if of a
    nonpayable function: the dispatcher reverts a call with value, so the clause must require
    msg.value == 0 (a top-level conjunct), or the per-function succeeds obligation is false."""
    cdef = next(
        (
            n
            for n in ast.get("nodes", [])
            if n.get("nodeType") == "ContractDefinition" and n.get("name") == contract
        ),
        None,
    )
    if cdef is None:
        return None, []
    # With bases, the checks read the flattened contract: the state variables and the public
    # functions of the whole C3 order (each selector's most derived definition). The initial state
    # is decided only when no base has a constructor or an initializer (a base constructor runs
    # before CDEF's initializers and can reach CDEF's variables through a virtual call).
    lin, _ = linearization(ast, cdef)
    lin = lin or [cdef]
    flat, seen_sel = [], set()
    for c in lin:
        for n in c.get("nodes", []):
            if (
                n.get("nodeType") == "FunctionDefinition"
                and n.get("kind") == "function"
            ):
                sel = n.get("functionSelector")
                if sel is None:
                    continue
                if sel in seen_sel:
                    continue
                seen_sel.add(sel)
            flat.append(n)
    base_init = any(
        n.get("nodeType") == "FunctionDefinition"
        and n.get("kind") == "constructor"
        or n.get("nodeType") == "VariableDeclaration"
        and n.get("stateVariable")
        and n.get("value")
        for c in lin[1:]
        for n in c.get("nodes", [])
    )
    svars = {}
    for n in (m for c in lin for m in c.get("nodes", [])):
        if n.get("nodeType") == "VariableDeclaration" and n.get("stateVariable"):
            v = n.get("value")
            svars[n["name"]] = (
                int(v["value"], 0)
                if v and v.get("nodeType") == "Literal" and v.get("kind") == "number"
                else (0 if v is None else None)
            )
    for f in flat:
        if f.get("nodeType") != "FunctionDefinition" or f.get("kind") != "function":
            continue
        if (
            f.get("visibility") not in ("external", "public")
            or f.get("stateMutability") == "payable"
        ):
            continue
        if why := vacuous_requires(f):
            return why, []
        for text in natspec_clauses(f, "succeeds-if") + natspec_clauses(
            f, "succeeds_if"
        ):
            guards = [p.strip().strip("()").replace(" ", "") for p in text.split("&&")]
            if "||" not in text and not (
                {"msg.value==0", "0==msg.value"} & set(guards)
            ):
                why = (
                    f"spec: {f['name']}_succeeds0 does not check: {f['name']} is nonpayable, so a "
                    f"call with a value other than 0 reverts, but its @custom:succeeds-if {text} "
                    "does not require msg.value == 0"
                )
                return why, []
    invs = []
    for text in natspec_clauses(cdef, "invariant"):
        entry = {"invariant": text, "status": "OPEN: not proved by the driver"}
        atoms = spec_atoms(text)
        if atoms is not None:
            init = {k: v for k, v in svars.items() if v is not None}
            ctor = next(
                (
                    f
                    for f in cdef.get("nodes", [])
                    if f.get("nodeType") == "FunctionDefinition"
                    and f.get("kind") == "constructor"
                ),
                None,
            )
            sets = literal_assignments(ctor.get("body")) if ctor else {}
            if sets is not None and len(init) == len(svars) and not base_init:
                init.update(sets)
                if spec_holds(atoms, init) is False:
                    shown = ", ".join(f"{k} = {init[k]}" for k, _, _ in atoms)
                    return (
                        f"invariant: {text} fails in the initial state (the constructor leaves {shown})",
                        invs,
                    )
            for f in flat:
                if (
                    f.get("nodeType") != "FunctionDefinition"
                    or f.get("kind") != "function"
                ):
                    continue
                if f.get("visibility") not in ("external", "public") or f.get(
                    "stateMutability"
                ) in ("view", "pure"):
                    continue
                sets = literal_assignments(f.get("body"))
                if sets and spec_holds(atoms, sets) is False:
                    shown = ", ".join(f"{k} = {v}" for k, v in sets.items())
                    return (
                        f"invariant: {text} is not preserved by {f['name']} (it sets {shown})",
                        invs,
                    )
        invs.append(entry)
    return None, invs


def recursion_cycles(cdef, variables, lin=None, extra=()):
    """The contract's recursion: each strongly connected component of its internal call graph that
    has a cycle (functions or modifiers that can reach themselves) and that the contract's own
    functions and modifiers reach, as (names, first call site), and the calls through internal
    function values, whose callee the source does not name. The graph's nodes are the contract's
    functions and modifiers (with LIN, CDEF's linearization, most derived first: every function and
    modifier of its contracts) and EXTRA, the sibling library and free functions CDEF runs inline
    (sibling_code; a contract calls them internally, so a cycle through them is recursion too); an
    edge is an internal call naming one of them (an Identifier, or a MemberAccess such as C.f) or a
    modifier invocation, to the callable the call runs in CDEF (Dispatch: final overrides, super).
    VARIABLES holds the ids of the source's variable declarations (struct members included)."""
    disp = Dispatch(lin or [cdef])
    own = dict(disp.callables)
    callables = dict(own)
    for f in extra:
        for n in walk(f):
            if n.get("nodeType") == "FunctionDefinition" and "id" in n:
                callables.setdefault(n["id"], n)
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
                    values.append((cid, n))
            else:
                continue
            k = disp.target(
                ref,
                n.get("modifierName")
                if t == "ModifierInvocation"
                else n.get("expression"),
                cid,
            )
            ref = k if k is not None else ref
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
    # What the contract's own functions and modifiers reach: a free function it never calls is not
    # part of its code.
    reached, todo = set(own), list(own)
    while todo:
        for w in edges[todo.pop()]:
            if w not in reached:
                reached.add(w)
                todo.append(w)
    values = [n for cid, n in values if cid in reached]
    cycles = []
    for comp in sccs:
        members = set(comp)
        inner = [(x, y) for x in comp for y in edges[x] if y in members]
        if not inner or not members & reached:
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


class Fns(list):
    """The external functions of the contract (a list), and in `ctor` its constructor's static word
    parameters ({"params": [{"name", "type", "width"}]}) when it takes arguments, else None: every
    generator of the run receives the functions, so the constructor rides with them."""

    ctor = None


# A constructor parameter bound for every argument word (Verify/Generic/CtorWords.lean,
# bindArgs_words): a uint256-typed word (narrow uintN with its cleanup) or an address.
CTOR_UINT = re.compile(r"uint(8|16|24|32|40|48|56|64|72|80|88|96|104|112|120|128|136|144|152|160|168|176|184|192|200|208|216|224|232|240|248|256)?")


def ctor_word_params(n):
    """The constructor N's parameters as static words ([{"name", "type", "width"}], []) or, for
    parameters outside them, ([], why) with one reason per parameter, each refused precisely:
    dynamic ABI types (bytes, string, arrays, structs), multi-word static types and the word types
    not bound yet (bool, intN, bytesN, enums, contracts, function types)."""
    ps, why = [], []
    for q in n.get("parameters", {}).get("parameters", []):
        name = q.get("name") or "_"
        t = (q.get("typeDescriptions") or {}).get("typeString", "")
        tn = q.get("typeName") or {}
        if tn.get("nodeType") == "ElementaryTypeName" and q.get("storageLocation") in (
            None,
            "default",
        ):
            base = tn.get("name", "")
            m = CTOR_UINT.fullmatch(base)
            if m:
                ps.append({"name": name, "type": t, "width": int(m.group(1) or 256)})
                continue
            if base in ("address", "address payable"):
                ps.append({"name": name, "type": t, "width": 160})
                continue
            if base in ("bytes", "string"):
                why.append(
                    f"parameter {name} of type {t}: a dynamic constructor argument; the deployment "
                    "is stated for static word arguments only"
                )
                continue
            why.append(
                f"parameter {name} of type {t}: only uintN and address words are bound for every "
                "argument (Verify/Generic/CtorWords.lean, bindArgs_words); bool, intN and bytesN "
                "are not yet"
            )
            continue
        kind = tn.get("nodeType", "")
        if kind == "ArrayTypeName" and tn.get("length") is not None:
            why.append(
                f"parameter {name} of type {t}: a static array, not one static word (only uintN and "
                "address words are bound for every argument)"
            )
        elif kind == "ArrayTypeName" or (
            kind == "ElementaryTypeName" and tn.get("name") in ("bytes", "string")
        ):
            why.append(
                f"parameter {name} of type {t}: a dynamic constructor argument; the deployment "
                "is stated for static word arguments only"
            )
        elif t.startswith("struct "):
            why.append(
                f"parameter {name} of type {t}: a struct argument, not one static word (only uintN "
                "and address words are bound for every argument)"
            )
        else:
            why.append(
                f"parameter {name} of type {t}: only uintN and address words are bound for every "
                "argument (Verify/Generic/CtorWords.lean, bindArgs_words); enums, contract and "
                "function types are not yet"
            )
    return (ps, []) if not why else ([], why)


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


# What solar writes where the linker puts a library's address: 20 bytes, as 40 characters.
LIBRARY_PLACEHOLDER = re.compile(r"__\$[0-9a-fA-F]{34}\$__")


def scan_bytecode(hexcode):
    """The opcodes of the runtime bytecode HEXCODE outside the fragment, each with its first pc,
    and "unlinked library" with the offset of the first library placeholder: the code calls a
    library whose address only linking fixes, which is not code the stages can describe."""
    found = {}
    m = LIBRARY_PLACEHOLDER.search(hexcode)
    if m:
        found["unlinked library"] = m.start() // 2
        hexcode = LIBRARY_PLACEHOLDER.sub("0" * 40, hexcode)
    code = bytes.fromhex(hexcode)
    i = 0
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


def kernel_table(doc, name, eq, ty, f, pre, entries):
    """F : String → TY by a table of its values at ENTRIES' signatures ((signature, value literal)
    pairs): each entry is PRE{i}_eq : PRE{i} = value, F on its signature by kernel_rfl alone (F
    costs the kernel too much to evaluate inside a larger check); NAME is the table
    (KernelTable.tableOr) and EQ : F = NAME follows from those equations by KernelTable.eq_tableOr,
    in one line whatever the number of entries."""
    defs = "".join(
        f"def {pre}{i} : {ty} :=\n  {f} {lean_str(s)}\n\nkernel_rfl {pre}{i}_eq {pre}{i} {v}\n\n"
        for i, (s, v) in enumerate(entries)
    )
    rows = ",\n     ".join(f"({lean_str(s)}, {v})" for s, v in entries)
    eqs = [f"{pre}{i}_eq" for i in range(len(entries))]
    agrees = f"⟨{', '.join(eqs)}, trivial⟩" if eqs else "trivial"
    return (
        f"{defs}/-- {doc} -/\ndef {name} : String → {ty} :=\n  KernelTable.tableOr {f}\n    [{rows}]\n\n"
        f"theorem {eq} : {f} = {name} :=\n  KernelTable.eq_tableOr {agrees}\n"
    )


SELECTOR_TABLE = (
    "§ABI.selectorFromSignature§, with the contract's signatures by table.",
    "selectorTable",
    "selector_eq",
    "SolidCore.Solidity.Shared.Word",
    "SolidCore.Solidity.Source.ABI.selectorFromSignature",
    "selSig",
)
DIGEST_TABLE = (
    "§Keccak.digestWord§, with the events' signatures by table.",
    "digestTable",
    "digest_eq",
    "Nat",
    "SolidCore.Solidity.Source.Keccak.digestWord",
    "topicSig",
)


def runtime_lean(deployed, plain):
    """The runtime code a creation installs, as a Lean term: DEPLOYED's bytes, or with immutables
    the term `runtime` (PLAIN) with each word the constructor patches."""
    patches = immutable_patches(deployed, plain)
    if patches is None:
        return lean_bytes(deployed)
    code = "runtime"
    for off, w in patches:
        code = f"patchWord ({code}) {off} {hex(w)}"
    return code


def lowering_lean(name, contract, fns, events, core_lit, storage_lit, sym=False):
    # The run's stages are exported under its own namespace (export_ns.py).
    X = export_ns.of_run(name, contract)
    ns = f"Certified.{name}.Lowering"
    ctor = getattr(fns, "ctor", None)
    if ctor is not None:
        # The probe's storage line for a constructor with word parameters: "ctorArgs WIDTHS WRITES",
        # the parameters' widths and the writes of its body's one normal leaf (ctorShapeOf).
        m = re.fullmatch(
            r"ctorArgs \[([0-9, ]*)\] ;; (.+) ;; (\[.*\]) ;; \[([0-9, ]*)\]",
            str(storage_lit).strip(),
            re.S,
        )
        if m is None:
            raise Failed(
                "constructor parameters: the probe did not print the constructor's writes for every argument"
            )
        widths = [int(x) for x in m.group(1).split(",") if x.strip()]
        want = [p["width"] for p in ctor["params"]]
        if widths != want:
            raise Failed(
                f"constructor parameters: SolidCore's widths {widths} are not the ABI's {want}"
            )
        if sym:
            raise Failed(
                "constructor parameters: the caller route any is not stated for constructor arguments"
            )
        tree, writes = m.group(2), m.group(3)
        witness = [int(x) for x in m.group(4).split(",") if x.strip()]
        # A tree that branches reverts for some arguments: the statement then covers the argument
        # lists on which the constructor returns (CtorReturns), with a witness that one does.
        branches = not tree.lstrip("(").startswith("SolidCore.Refinement.Generic.Br.leaf")
        ctor["branches"] = branches
        ctor["witness"] = witness
        storage_defs = f"""/-- The widths of the constructor's parameters (§uint256§-typed words their width, addresses
160), as SolidCore's check reads them (checked by §shape_eq§). -/
def ctorWidths : List Nat := {widths}

/-- The erasure of the constructor's body over the argument words (§WExpr.arg i§), as the probe
printed it (checked by §shape_eq§): its leaves are normal with §ctorWrites§ and no logs, or revert. -/
def ctorTree : Generic.Br :=
  {tree}

/-- The writes of the constructor's normal leaves (checked by §shape_eq§): slot words and value
words over the argument words. -/
def ctorWrites : Generic.Writes :=
  {writes}

/-- What the constructor leaves in storage on the argument words §ws§: each slot it writes, with its
word. -/
def storageOf (ws : List Nat) : List (Nat × Nat) := Generic.storageOf ctorWrites ws"""
        shape_def = f"""/-- SolidCore's checked core, and whether the constructor's body erases, with §ctorWidths§, to one
normal leaf with the writes §ctorWrites§ and no logs. -/
def shape? : Option (Contract × Bool) := Generic.ctorShapeOf fuel contract? ctorWidths ctorTree ctorWrites"""
        deployed_def = """/-- The deployment on every argument list of the parameters' count, each word below §2 ^ 256§ and
clean for its width, on which the constructor returns: it leaves a state with no logs, and a storage
that writes every slot of §storageOf ws§ writes every slot it leaves nonzero; and the constructor
returns wherever §ctorTree§'s leaf is normal. -/
theorem deployed_of {c : TypeCheck.CheckedContract} (hc : contract? = .ok c) :
    c.core = coreLit ∧
      (∀ ws, ws.length = ctorWidths.length → (∀ a ∈ ws, a < 2 ^ 256) →
        Generic.argsClean ctorWidths ws = true → Generic.CtorReturns fuel contract? ws →
        ∃ d, Generic.Loop.LoadDeployed fuel c ws d ∧
          ∀ σ, Generic.Covers (storageOf ws) σ → Generic.Loop.CoversLoad d σ) ∧
      (∀ ws, ws.length = ctorWidths.length → (∀ a ∈ ws, a < 2 ^ 256) →
        Generic.argsClean ctorWidths ws = true → Generic.normalAt ctorTree ws = true →
        Generic.CtorReturns fuel contract? ws) :=
  Generic.deployed_of_ctorShapeOf shape_eq hc

/-- The constructor returns on the witness words """ + str(witness) + """ (its erasure's leaf there is normal). -/
theorem ctorReturns_witness : Generic.CtorReturns fuel contract? """ + str(witness) + """ := by
  have h := shape_eq
  unfold shape? Generic.ctorShapeOf at h
  split at h
  · next c hc =>
    exact (deployed_of hc).2.2 """ + str(witness) + """ rfl (by decide) (by decide +kernel) (by decide +kernel)
  · simp at h"""
        ctor_import = "import Verify.Generic.CtorWords\n"
        intro_doc = (
            "§coreLit§ is SolidCore's own evaluation of the checked contract, and §ctorWidths§ and\n"
            "§ctorWrites§ the erasure of its constructor's body for every argument list, printed by the\n"
            "driver's probe and read back; §shape_eq§ proves in the kernel that they are what §contract?§\n"
            "gives, so the probe is not trusted."
        )
    else:
        storage_defs = f"""/-- What the constructor leaves in storage, as SolidCore's evaluation printed it (checked by §shape_eq§). -/
def storageLit : List (Nat × Nat) := {storage_lit}"""
        shape_def = """/-- SolidCore's checked core, and whether the constructor leaves exactly §storageLit§ and no logs. -/
def shape? : Option (Contract × Bool) := Generic.shapeOf fuel contract? storageLit"""
        deployed_def = """theorem deployed_of {c : TypeCheck.CheckedContract} (hc : contract? = .ok c) :
    c.core = coreLit ∧ ∃ d, Wide.DeployedAt fuel c d storageLit :=
  Generic.deployed_of_shapeOf shape_eq hc"""
        ctor_import = ""
        intro_doc = (
            "§coreLit§ and §storageLit§ are SolidCore's own evaluation of the checked contract and of its\n"
            "constructor, printed by the driver's probe and read back; §shape_eq§ proves in the kernel that\n"
            "they are what §contract?§ gives, so the probe is not trusted."
        )
    sels = "\n".join(
        f"/-- The selector of §{f['sig']}§. -/\nabbrev sel{i} : Nat := {hex(f['sel'])}\n"
        for i, f in enumerate(fns)
    )
    fndefs = "".join(
        f"/-- §{f['sig']}§ and its parameters. -/\ndef fn{i} : Fn := ⟨sel{i}, [{', '.join(f['params'])}]⟩\n\n"
        for i, f in enumerate(fns)
    )
    fnlist = ", ".join(f"fn{i}" for i in range(len(fns)))
    table = kernel_table(
        *SELECTOR_TABLE, [(f["sig"], f"sel{i}") for i, f in enumerate(fns)]
    )
    using = "selector_eq"
    digest = ""
    if events:
        digest = (
            kernel_table(*DIGEST_TABLE, [(e["sig"], hex(e["topic"])) for e in events])
            + "\n"
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
    Generic.checkSym fuel mirFuel coreLit {X}.stage0 fns = true := by
  decide +kernel

set_option maxRecDepth 100000 in
/-- The kernel's evaluation: stage0 reverts on a call with a value, whoever sends it. -/
theorem checkValueSym_stage0 :
    Generic.checkValue mirFuel coreLit {X}.stage0 fns .allSym = true := by
  decide +kernel
"""
        if sym
        else ""
    )
    return f"""import SolidCore.Refinement.Generic.Value
import SolidCore.Refinement.Generic.Shape
{ctor_import}{sym_import}import SolidCore.Refinement.KernelCopies
import SolidCore.Refinement.KernelRfl
import SolidCore.Refinement.KernelTable
import {name}.Source
import {name}.gen.{contract}.Stage0

-- Generated by solar's tools/certify/generic.py from this run's source and export; do not edit.
-- Untrusted data and composition: every literal below is checked by the kernel.

/-!
# {contract}: the generic source check, generated

{intro_doc} §check_stage0§ and §checkValue_stage0§
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

{storage_defs}

/-! ## Selectors and topics, each by Keccak-256 once, then by table -/

{table}
{digest}{shape_def}

kernel_rfl shape_eq shape? (some (coreLit, true)) using {using}

set_option maxRecDepth 100000 in
/-- The kernel's evaluation: the generic check accepts stage0 for every function. -/
theorem check_stage0 :
    Generic.check fuel mirFuel coreLit {X}.stage0 fns .all = true := by
  decide +kernel

set_option maxRecDepth 100000 in
/-- The kernel's evaluation: no function is payable, and stage0 reverts on a call with a value. -/
theorem checkValue_stage0 :
    Generic.checkValue mirFuel coreLit {X}.stage0 fns .all = true := by
  decide +kernel
{sym_thms}
{deployed_def}

end {ns}
""".replace("§§", "§§").replace("§", "\u0060")


def probe_lean(name, contract, fns):
    X = export_ns.of_run(name, contract)
    """The probe (never part of a proof): SolidCore's evaluation of the checked contract's core and
    of its constructor's storage, printed for the generated module to state as literals, and per
    function whether SolidCore's erased tree and the symbolic run of the first lowered MIR exist
    and agree. Run with lake env lean in a scratch clone; nothing it prints is trusted."""
    checks = "\n".join(
        f'  IO.println ("PROBE fn {f["sig"]}: " ++ (match core.findFunctionBySelector? {f["sel"]} with\n'
        f'    | none => "no source function"\n'
        f"    | some fn => match Generic.eraseFn (Generic.envOf core) fuel fn, "
        f"Generic.symTree {X}.stage0 {f['sel']} {len(f['params'])} mirFuel .all with\n"
        f'      | none, _ => "source tree undefined (erasure)"\n'
        f'      | _, none => "MIR symbolic run undefined"\n'
        f'      | some a, some b => if Generic.Tree.agree a b then "agree" else ("trees differ: source " ++ (reprStr a.canon).replace "\\n" " " ++ " MIR " ++ (reprStr b.canon).replace "\\n" " ")))'
        for f in fns
    )
    # The same check for every caller (Generic.checkFnSym): the source erased with msg.sender a
    # symbolic word below 2^160, the module run with CALLER that word. caller_route reads it.
    checks += "\n" + "\n".join(
        f'  IO.println ("PROBE sym fn {f["sig"]}: " ++ (match core.findFunctionBySelector? {f["sel"]} with\n'
        f'    | none => "no source function"\n'
        f"    | some fn => match Generic.eraseFnSym (Generic.envOf core) fuel fn, "
        f"Generic.symTree {X}.stage0 {f['sel']} {len(f['params'])} mirFuel .allSym with\n"
        f'      | none, _ => "source tree undefined (erasure)"\n'
        f'      | _, none => "MIR symbolic run undefined"\n'
        f'      | some a, some b => if Generic.Tree.agree a b then "agree" else ("trees differ: source " ++ (reprStr a.canon).replace "\\n" " " ++ " MIR " ++ (reprStr b.canon).replace "\\n" " ")))'
        for f in fns
    )
    # The succeeds-witness search (synth_witness): SolidCore's run of every function on the zero
    # call (zero words, no value, the constructor's slots written with 0). Untrusted: the Shape
    # module re-proves the chosen witness by the kernel's evaluation.
    runs = (
        "  let st0 : List (Nat × Nat) := match TypeCheck.CheckedContract.constructWithContext? fuel c\n"
        "      (Wide.ctxOf c) State.empty SolidCore.Refinement.selfAddress 0 [] with\n"
        "    | some (CallResult.returned d _) => d.storage.toList.map fun kv => (kv.1, 0)\n"
        "    | _ => []\n"
        + "\n".join(
            f'  IO.println ("PROBE run fn {f["sig"]}: " ++ (match SolidCore.Refinement.Source.run fuel c\n'
            f"      ⟨SolidCore.Refinement.Abi.calldata {f['sel']} (List.replicate {len(f['params'])} 0), 0, st0⟩ with\n"
            f'    | some o => if o.success then "success" else "revert"\n'
            f'    | none => "none"))'
            for f in fns
        )
    )
    ctor_import = ""
    ctor_helper = ""
    ctor_run = """  match TypeCheck.CheckedContract.constructWithContext? fuel c (Wide.ctxOf c) State.empty
      SolidCore.Refinement.selfAddress 0 [] with
  | some (CallResult.returned d _) =>
    IO.println ("PROBE constructor: returns" ++ (if d.events.isEmpty then "" else ", with logs"))
    IO.println ("PROBE storage: " ++ reprStr d.storage.toList)
  | some _ => IO.println "PROBE constructor: does not return (reverts or fails) with no arguments"
  | none => IO.println "PROBE constructor: undefined with no arguments\""""
    if getattr(fns, "ctor", None):
        # A constructor with static word parameters: its body's erasure with the parameters' width
        # facts (Verify/Generic/CtorWords.lean, ctorShapeOf), for every argument list at once. The
        # storage line carries the widths and the writes (Lowering reads them back as literals).
        ctor_import = "import Verify.Generic.CtorWords\n"
        ctor_run = """  match TypeCheck.CheckedProgram.constructorFunctionFor? c.program c.decl with
  | none => IO.println "PROBE constructor: no constructor function"
  | some fn =>
  match Generic.widthsOf fn.params fn.paramAbiCleanups with
  | none => IO.println "PROBE constructor: the parameters' widths are undefined"
  | some wds =>
  if !Generic.wordParams fn.params then
    IO.println "PROBE constructor: a parameter is not a uint256-typed or address word"
  else if !fn.returns.isEmpty then IO.println "PROBE constructor: returns values"
  else
  match Generic.eraseStmt { Generic.envOf core with facts := Generic.argFacts wds 0 } fuel fn.body
      { Generic.initState fn with caller := .lit SolidCore.Refinement.selfAddress } with
  | none =>
    let bad := (core.storageFields.filter fun f => !Generic.fieldOk f && !Generic.mapOk f).map (·.name)
    IO.println ("PROBE constructor: the constructor's body is outside the erasure (eraseStmt)" ++
      (if bad.isEmpty then "" else "; storage fields outside its unsigned scalars and mappings: " ++ ", ".intercalate bad))
  | some b =>
  match firstNormal b with
  | none => IO.println "PROBE constructor: no normal leaf (it reverts on every argument list)"
  | some S =>
  if !Generic.normalsOnly S.writes b then
    IO.println "PROBE constructor: the erasure's normal leaves differ, log or return; the statement for every argument needs one normal leaf shape"
  else
  let zeros := wds.map fun _ => 0
  let ones := wds.map fun _ => 1
  match (if Generic.normalAt b zeros then some zeros else if Generic.normalAt b ones then some ones else none) with
  | none => IO.println "PROBE constructor: returns on no argument list tried (all zeros, all ones): no witness"
  | some w =>
    IO.println ("PROBE constructor: returns" ++ (match b with | .leaf _ => " for every argument list" | _ => " where its erasure does not revert"))
    IO.println ("PROBE storage: ctorArgs " ++ reprStr wds ++ " ;; " ++ ((repr b).pretty 100000000).replace "\\n" " " ++
      " ;; " ++ ((repr S.writes).pretty 100000000).replace "\\n" " " ++ " ;; " ++ reprStr w)"""
        ctor_helper = """
/-- The first normal leaf of an erasure tree. -/
def firstNormal : Generic.Br → Option Generic.SymState
  | .leaf (.normal S) => some S
  | .leaf _ => none
  | .ite _ t e => (firstNormal t).orElse fun _ => firstNormal e
"""
    return f"""import SolidCore.Refinement.Generic.Value
import SolidCore.Refinement.Generic.StageFrom
{ctor_import}import {name}.Source
import {name}.gen.{contract}.Stage0

namespace Certified.{name}.Probe

open SolidCore.Refinement SolidCore.Refinement.Abi SolidCore.Solidity SolidCore.Solidity.Source
open EthLib.Base SolidCore.Refinement.Evm.Wide

def fuel : Nat := 4096
def mirFuel : Nat := 4096

def contract? : Except TypeCheck.TypeError TypeCheck.CheckedContract :=
  TypeCheck.CheckedInput.contract Certified.{name}.Src.importedSourceUnit {lean_str(contract)}
{ctor_helper}
def main : IO Unit := do
  match contract? with
  | .error _ => IO.println "PROBE typecheck: error"
  | .ok c =>
  let core := c.core
  IO.println "PROBE typecheck: ok"
  IO.println ("PROBE core: " ++ ((repr core).pretty 100000000).replace "\\n" " ")
{ctor_run}
{checks}
{runs}

end Certified.{name}.Probe

#eval Certified.{name}.Probe.main
"""


def lakefile_lib(name, contract):
    """The library stanza the driver appends for Certified/NAME (lakefile_inputs.py, which also
    holds the rule for that append in the inputs commit). Refuses a NAME outside the driver's name
    grammar or a CONTRACT that is not a Solidity name."""
    try:
        return lakefile_inputs.lakefile_lib(name, contract)
    except lakefile_inputs.Refused as e:
        raise Failed(f"lakefile stanza: {e}") from None


def lean_lib_body(name):
    """The declaration of lakefile_lib's stanza, without its comment."""
    try:
        return lakefile_inputs.lean_lib_body(name)
    except lakefile_inputs.Refused as e:
        raise Failed(f"lakefile stanza: {e}") from None


lean_lib_declared = lakefile_inputs.lean_lib_declared


def declare_lean_lib(lf, name, stanza, own=()):
    """Append STANZA to the lakefile LF unless LF already declares lean_lib NAME, so that no path
    declares it twice. A declaration is reused only when LF holds STANZA or one of OWN (another
    stanza this driver writes for NAME); any other declaration of NAME, in either spelling, is
    refused. Returns whether LF declared it already."""
    text = lf.read_text()
    if any(s in text for s in (stanza, *own)):
        return True
    if lean_lib_declared(text, name):
        raise Failed(
            f"the base's lakefile declares lean_lib {name} other than as this driver writes it"
        )
    with open(lf, "a") as f:
        f.write(stanza)
    return False


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
    if declare_lean_lib(clone / "lakefile.lean", name, lakefile_lib(name, contract)):
        held = True
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
            *LAKE,
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
        ln, _ = keep_log(f"{name}.Probe", out + err)
        raise Failed(
            f"the probe's build fails (log {ln}): "
            + " | ".join(l for l in (out + err).splitlines() if l.startswith("error"))[
                :400
            ]
        )
    rc, out, err = run([*LAKE, "env", "lean", str(d / "Probe.lean")], cwd=pl, env=env)
    lines = [l for l in out.splitlines() if l.startswith("PROBE ")]
    res = {}
    for l in lines:
        what, _, verdict = l[len("PROBE ") :].partition(": ")
        res[what] = verdict
    if rc != 0 or "typecheck" not in res:
        ln, _ = keep_log(f"{name}.Probe.run", out + err)
        raise Failed(
            f"the probe fails (log {ln}): "
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


# The spec witness generic.py and the scoreboard name when no function has succeeds-if conditions:
# the succeeds witness is chosen after the probe (synth_witness).
SYNTH_WITNESS = "<succeeds witness>"


def spec_witness(fns):
    """The non-vacuity witness of a spec run: the first function with succeeds-if conditions
    (mutating first), else SYNTH_WITNESS when some function has a spec (requires or ensures), whose
    concrete succeeds witness synth_witness picks after the probe. Fails when no function has a spec."""
    w = next(
        (f["name"] for f in fns if f.get("mutates") and f.get("succeeds")), None
    ) or next((f["name"] for f in fns if f.get("succeeds")), None)
    if w is not None:
        return w
    if any(f.get("specified") for f in fns):
        return SYNTH_WITNESS
    raise Failed(
        "spec: no function has @custom:succeeds-if, so the spec's allowed statuses reject nothing and "
        "the certified statement's non-vacuity (a revert the spec rejects) cannot hold; add succeeds-if "
        "conditions, or run with SPEC none"
    )


def synth_witness(res, fns):
    """The succeeds witness (succeeds_witness.py) of a spec without succeeds-if conditions: the first
    function with a spec (mutating first) whose zero call (zero words, no value, the constructor's
    slots written with 0) SolidCore's probe runs successfully. Untrusted: the Shape module proves
    that it succeeds by the kernel's evaluation and that it meets its precondition (pre_sat), and
    fails otherwise. Fails, naming every verdict, when no zero call succeeds: the proofs support
    only the zero call, so the run is refused rather than the spec weakened."""
    cands = sorted(
        (f for f in fns if f.get("specified")), key=lambda f: not f.get("mutates")
    )
    for f in cands:
        if res.get(f"run fn {f['sig']}") == "success":
            return {
                "function": f["name"],
                "sel": f["sel"],
                "arity": len(f["params"]),
                "args": [0] * len(f["params"]),
                "storage": [],
            }
    seen = ", ".join(f"{f['sig']}: {res.get('run fn ' + f['sig'], 'not run')}" for f in cands)
    raise Failed(
        "spec: no succeeds witness: no function has @custom:succeeds-if, and no spec'd function's zero "
        "call (zero words, no value, the constructor's slots written with 0) succeeds in SolidCore "
        f"({seen}); the spec may be vacuous there (the witness proofs support only the zero call); "
        "add succeeds-if conditions, or run with SPEC none"
    )


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
    if getattr(fns, "ctor", None):
        return (
            "0x2000",
            None,
            [],
            "caller route: the fixed caller 0x2000 (constructor arguments are stated on the fixed caller)",
        )
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
        [*LAKE, "build", f"{name}.Source", "SolidCore.Refinement.Wide.Refines"],
        cwd=pl,
        env=env,
    )
    if rc != 0:
        ln, _ = keep_log(f"{name}.LifeProbe", out + err)
        raise Failed(
            f"lifecycle: the probe's build fails (log {ln}): "
            + " | ".join(l for l in (out + err).splitlines() if l.startswith("error"))[
                :400
            ]
        )
    rc, out, err = run(
        [*LAKE, "env", "lean", str(d / "LifeProbe.lean")], cwd=pl, env=env
    )
    res = {}
    for l in out.splitlines():
        if l.startswith("PROBE "):
            what, _, verdict = l[len("PROBE ") :].partition(": ")
            res[what] = verdict
    if rc != 0 or "typecheck" not in res:
        ln, _ = keep_log(f"{name}.LifeProbe.run", out + err)
        raise Failed(
            f"lifecycle: the probe fails (log {ln}): "
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
    bind=None,
):
    """The lifecycle module of a run (or, with REFUTE, its refutation): solar's init code INIT and
    runtime PLAIN as this run compiled them, the argument words WS the run was given and the storage
    STORAGE of SolidCore's constructor as the probe printed it. Untrusted text: the kernel checks
    every literal. Deploys is Creation.deploys_of_check, one kernel evaluation of the generic
    creation frame; the source half is Route.deployedAtWith by kernel_rfl, as the generic source
    check's DeployedAt is. The runtime code is PLAIN with the immutable words the constructor
    patched in (DEPLOYED's differences), so an unpatched immutable is refuted.

    With BIND (lifecycle_binding: the expected theorem's own constants), the module states the
    lifecycle at that theorem's (c, fuel): fuel and contract? are its constants by definition, and
    init, code and ws are proved equal to its init code, runtime and words (fuel_same,
    contract_same, init_same, code_same, ws_same), so the certificate and the theorem share one
    source unit, constructor and runtime."""
    code = runtime_lean(deployed, plain)
    table = kernel_table(*SELECTOR_TABLE, [(g["sig"], hex(g["sel"])) for g in sigs])
    bind_imports = (
        "".join(f"import {m}\n" for m in bind_modules(bind))
        + "import Verify.Generic.Expected\n"
        if bind
        else ""
    )
    fuel_def = bind["fuel"] if bind else "4096"
    fuel_doc = ": the expected theorem's own" if bind else ""
    contract_def = (
        f"{paren(bind['source'])}.checked"
        if bind
        else f"TypeCheck.CheckedInput.contract {unit} {lean_str(contract)}"
    )
    same = (
        f"""
/-! ## The expected theorem's (c, fuel), constructor and runtime -/

theorem fuel_same : fuel = {bind["fuel"]} := rfl

theorem contract_same : contract? = {paren(bind["source"])}.checked := rfl

theorem init_same : init = {bind["init"]} := by
  apply ByteArray.ext; decide +kernel

theorem code_same : code = SolidCore.Verify.Generic.runtimeCode {bind["runtime"]} := by
  apply ByteArray.ext; decide +kernel

theorem ws_same : ws = [{", ".join(str(w) for w in bind["ws"])}] := rfl
"""
        if bind
        else ""
    )
    head = f"""import Verify.Lifecycle.CreationFrame
import Verify.Lifecycle.Life
import Verify.Lifecycle.KernelIndex
import SolidCore.Refinement.KernelRfl
import SolidCore.Refinement.KernelTable
import {src_import}
{bind_imports}
-- Generated by solar's tools/certify/generic.py from this run's init code, arguments and storage;
-- do not edit. Untrusted data and composition: every literal below is checked by the kernel.

namespace {ns}

open EthLib.Base EthLib.Execution EthLib.Execution.Evm EthLib.Execution.Evm.Logic
open SolidCore.Solidity SolidCore.Refinement Verify.Lifecycle Verify.Lifecycle.Creation

/-- The interpreter fuel of SolidCore's runs{fuel_doc}. -/
def fuel : Nat := {fuel_def}

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
#guard_msgs (whitespace := lax) in
#print axioms refuted

end {ns}
""".replace("§", "\u0060")
        )
    return (
        head
        + f"""
/-- SolidCore's checked §{contract}§. -/
def contract? : Except TypeCheck.TypeError TypeCheck.CheckedContract :=
  {contract_def}

/-! ## Selectors, each by Keccak-256 once, then by table -/

{table}
/-- The generic creation frame's check of this creation. -/
def checkB : Bool := check frameFuel (initArgs init ws) code storageLit

-- The kernel's evaluation, added as a declaration: a false check is a kernel error, never a
-- reduction outside the kernel.
kernel_decide checkB_eq checkB

/-- EthLib's creation of §init§ with §ws§ installs §code§ and leaves §storageLit§. -/
theorem deploys : Deploys (initArgs init ws) code storageLit :=
  deploys_of_check frameFuel checkB_eq

/-- Whether the contract checks and its constructor, called with §ws§, leaves §storageLit§ and no logs. -/
def life? : Bool := lifeOf fuel contract? ws storageLit

kernel_rfl life_eq life? true using selector_eq mkIdx_eq

/-- **The lifecycle statement**: the checked contract's constructor on §ws§ leaves §storageLit§,
and solar's init code with §ws§ deploys §code§ with the same storage. -/
theorem statement : ∃ c, contract? = .ok c ∧ Statement fuel c init code ws storageLit :=
  statement_of_lifeOf deploys life_eq

/-- info: '{ns}.statement' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs (whitespace := lax) in
#print axioms statement
{same}
end {ns}
""".replace("§", "\u0060")
    )


BIND_KEYS = {
    "unit": "the expected theorem's checked source unit (a constant)",
    "unit_module": "its module",
    "fuel": "its fuel constant",
    "init": "its constructor's init code constant (ByteArray)",
    "runtime": "the runtime bytes constant it states Expected for (List Nat)",
    "modules": "the modules holding fuel, init and runtime (the lifecycle module imports these, the "
    "unit's and the source's)",
    "ws": "the pinned constructor words it admits for this run",
    "source": "the expected theorem's Source term (a constant, or one applied to the words in ws, "
    "such as CounterCtx.src 8)",
    "source_module": "the module defining the source term (the lifecycle module imports it: its "
    "contract? is the source's checked)",
    "expected": "{module, binders, prop, term}: the theorem's module, its Expected conjunct, binders and the term "
    "proving PROP ∧ ∀ c, contract? = .ok c → ∀ s, Statement fuel c init (runtimeCode runtime) ws s → "
    "Deploys (initArgs init ws) (runtimeCode runtime) s",
}
LEAN_IDENT = r"[A-Za-z_][A-Za-z0-9_'?!]*(\.[A-Za-z_][A-Za-z0-9_'?!]*)*"
BIND_FORBIDDEN = (
    "sorry",
    "admit",
    "axiom",
    "native_decide",
    "ofReduceBool",
    "trustCompiler",
    "implemented_by",
    "extern",
    "unsafe",
    "#",
    "import",
    "set_option",
)


def lifecycle_binding(path, ws):
    """--lifecycle-bind FILE: the expected theorem the lifecycle certificate is bound to (BIND_KEYS),
    checked as names and text before any is written into a module. WS must be the run's own words."""
    try:
        b = json.loads(pathlib.Path(path).read_text())
    except (OSError, ValueError) as e:
        raise Failed(f"--lifecycle-bind: cannot read {path}: {e}") from None
    if not isinstance(b, dict) or set(b) != set(BIND_KEYS):
        raise Failed(f"--lifecycle-bind: keys must be exactly {sorted(BIND_KEYS)}")
    for k in ("unit", "unit_module", "source_module", "fuel", "init", "runtime"):
        if not (isinstance(b[k], str) and re.fullmatch(LEAN_IDENT, b[k])):
            raise Failed(f"--lifecycle-bind: {k} {b[k]!r} is not a Lean name")
    if not (
        isinstance(b["modules"], list)
        and b["modules"]
        and all(
            isinstance(m, str) and re.fullmatch(LEAN_IDENT, m) for m in b["modules"]
        )
    ):
        raise Failed("--lifecycle-bind: modules must be a list of Lean module names")
    src_term = b.get("source")
    for k, t in (("source", src_term),):
        if not isinstance(t, str):
            raise Failed(f"--lifecycle-bind: {k} must be a Lean term")
        head, *args = t.split(" ")
        if not re.fullmatch(LEAN_IDENT, head) or any(
            not re.fullmatch(r"[0-9]+", x) for x in args
        ):
            raise Failed(
                f"--lifecycle-bind: {k} {t!r} is not a Lean name applied to decimal words"
            )
        if args and [int(x) for x in args] != list(ws):
            raise Failed(
                f"--lifecycle-bind: {k} {t!r} is applied to {args}, not this run's words {list(ws)}"
            )
    if b["ws"] != list(ws):
        raise Failed(
            f"--lifecycle-bind: ws {b['ws']} are not this run's constructor words {list(ws)}"
        )
    e = b["expected"]
    if not (
        isinstance(e, dict)
        and set(e) == {"module", "binders", "prop", "term"}
        and all(isinstance(e[k], str) for k in e)
    ):
        raise Failed(
            "--lifecycle-bind: expected must be {module, binders, prop, term} strings"
        )
    if not re.fullmatch(LEAN_IDENT, e["module"]):
        raise Failed("--lifecycle-bind: expected.module is not a Lean module name")
    if not e["prop"].startswith("SolidCore.Verify.Generic.Expected "):
        raise Failed(
            "--lifecycle-bind: expected.prop must be a SolidCore.Verify.Generic.Expected statement"
        )
    lead = f"SolidCore.Verify.Generic.Expected {paren(src_term)} "
    if not e["prop"].startswith(lead):
        raise Failed(
            f"--lifecycle-bind: expected.prop is not about the bound source {src_term!r} "
            f"(it must start {lead!r})"
        )
    if f" {b['runtime']}" not in e["prop"]:
        raise Failed("--lifecycle-bind: expected.prop is not about the bound runtime")
    for k in ("binders", "prop", "term"):
        t = e[k]
        if "\n" in t or any(w in t for w in BIND_FORBIDDEN):
            raise Failed(
                f"--lifecycle-bind: expected.{k} holds a forbidden token or a newline"
            )
    return b


def paren(t):
    return f"({t})" if " " in t else t


def bind_modules(bind):
    """The modules the bound lifecycle module imports: the unit's, the source term's, and those
    holding fuel, init and runtime, once each, in that order."""
    out = []
    for m in [bind["unit_module"], bind["source_module"], *bind["modules"]]:
        if m not in out:
            out.append(m)
    return out


def source_same_lean(name, bind):
    """The check that this run's source unit, from its own solc AST, is the expected theorem's: one
    definitional equality, so the run's commit-bound source is the unit the theorem is about."""
    return f"""import {name}.Source
import {bind["unit_module"]}

-- Generated by solar's tools/certify/generic.py; do not edit.

namespace Certified.{name}.SourceSame

theorem unit_same : Certified.{name}.Src.importedSourceUnit = {bind["unit"]} := rfl

end Certified.{name}.SourceSame
"""


def composed_lean(name, bind):
    """The composed theorem: the expected theorem's Expected, and at one witness c of its source's
    checked contract (the lifecycle's contract? is that source's checked, by definition):
    Source.CreatedBy c with solar's init code and the creation bytes initArgs init ws, the lifecycle
    Statement and the deployment of those creation bytes. EXPECTED.term proves Expected ∧ ∃ c,
    SOURCE.checked = .ok c ∧ SOURCE.CreatedBy c INIT (initArgs INIT WS) ∧ … (the shape of
    CounterCreated.counter_expected_created); its c is proved to be the certificate's, and the
    admission is the theorem's, never a second one."""
    life = f"Certified.{name}.Lifecycle"
    e = bind["expected"]
    src = paren(bind["source"])
    ws = "[" + ", ".join(str(w) for w in bind["ws"]) + "]"
    init = bind["init"]
    cre = f"(Verify.Lifecycle.initArgs {init} {ws})"
    code = f"(SolidCore.Verify.Generic.runtimeCode {bind['runtime']})"
    return f"""import {name}.Lifecycle
import {name}.SourceSame
import {e["module"]}

-- Generated by solar's tools/certify/generic.py; do not edit.

namespace Certified.{name}.Composed

/-- **The expected theorem with this run's lifecycle certificate**: the theorem's Expected, and at
the one checked contract c of its source: the source is created by solar's init code with exactly
the arguments {ws} (§Source.CreatedBy§, the theorem's typed admission), SolidCore's constructor of c
on {ws} leaves §storageLit§, and those creation bytes deploy the runtime with the same storage. -/
theorem composed {e["binders"]} :
    {e["prop"]} ∧
      ∃ c, {src}.checked = .ok c ∧ {src}.CreatedBy c {init} {cre} ∧
        Verify.Lifecycle.Statement {bind["fuel"]} c {init} {code} {ws} {life}.storageLit ∧
        SolidCore.Refinement.Deploys {cre} {code} {life}.storageLit := by
  obtain ⟨hE, c, hc, hcb, -⟩ := {e["term"]}
  obtain ⟨c', hc', hl⟩ := {life}.statement
  rw [{life}.contract_same, hc] at hc'
  cases hc'
  rw [{life}.fuel_same, {life}.init_same, {life}.code_same, {life}.ws_same] at hl
  exact ⟨hE, c, hc, hcb, hl, hl.deploys⟩

/-- info: 'Certified.{name}.Composed.composed' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs (whitespace := lax) in
#print axioms composed

end Certified.{name}.Composed
""".replace("§", "\u0060")


def storage_list(text):
    """The probe's storage entries in the probe's order (SolidCore's d.storage.toList, which
    DeployedAtWith states exactly)."""
    return [(int(k), int(v)) for k, v in re.findall(r"\((\d+), (\d+)\)", text or "")]


def lifecycle_all_lean(ns, contract, init, plain, deployed, ws, storage):
    """The all-arguments module of a run: every creation of INIT with len(WS) static argument words
    that meets the path conditions of its symbolic run deploys the runtime code and storage that run
    reads off (Verify.Lifecycle.Creation.Sym.deploys_allArgs), and this run's creation, WS with
    DEPLOYED and STORAGE, is its instance at WS (deploys_at). Untrusted text: the specification is
    the kernel's own evaluation of the symbolic run, and every literal is checked by the kernel."""
    code = runtime_lean(deployed, plain)
    words = ", ".join(f"{w}#256" for w in ws)
    return f"""import Verify.Lifecycle.Symbolic
import SolidCore.Refinement.KernelRfl

-- Generated by solar's tools/certify/generic.py from this run's init code, arguments and storage;
-- do not edit. Untrusted data and composition: every literal below is checked by the kernel.

namespace {ns}

open EthLib.Base EthLib.Execution EthLib.Execution.Evm EthLib.Execution.Evm.Logic
open SolidCore.Refinement Verify.Lifecycle Verify.Lifecycle.Creation Verify.Lifecycle.Creation.Sym

/-- The steps of the symbolic creation frame's evaluator. -/
def frameFuel : Nat := 4096

/-- solar's init code for §{contract}§, as this run compiled it. -/
def init : ByteArray := {lean_bytes(init)}

/-- solar's runtime code (§bin-runtime§), immutables as placeholders. -/
def runtime : ByteArray := {lean_bytes(plain)}

/-- The runtime code this run's creation installs: solar's, with each immutable the constructor patches. -/
def code : ByteArray := {code}

/-- The number of the constructor's static argument words. -/
def nArgs : Nat := {len(ws)}

/-- The constructor's argument words, as this run was given them. -/
def ws : List Nat := [{", ".join(str(w) for w in ws)}]

/-- What the constructor leaves in storage, as the probe printed it. -/
def storageLit : List (Nat × Nat) := [{", ".join(f"({k}, {v})" for k, v in storage)}]

/-- The symbolic run's check of its own specification. -/
def allB : Bool := allArgsCheck frameFuel init nArgs

kernel_decide allB_eq allB

/-- **Every creation**: each list of §nArgs§ argument words that meets the symbolic run's path
conditions deploys §specCode§ and §specStore§ read at those words. -/
theorem deploysAll (as : List U256) (hn : as.length = nArgs)
    (hpre : ∀ e ∈ specPre frameFuel init nArgs, e.eval as = 0) :
    Deploys (initArgs init (as.map BitVec.toNat)) (codeOf as (specCode frameFuel init nArgs))
      (storeOf as (specStore frameFuel init nArgs)) :=
  deploys_allArgs allB_eq as hn hpre

/-- Whether this run's words meet the path conditions and give this run's code and storage. -/
def atB : Bool := atArgsCheck frameFuel init nArgs [{words}] ws code storageLit

kernel_decide atB_eq atB

/-- This run's creation, as the instance of §deploysAll§ at its argument words. -/
theorem deploysAt : Deploys (initArgs init ws) code storageLit :=
  deploys_at allB_eq atB_eq

/-- info: '{ns}.deploysAll' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs (whitespace := lax) in
#print axioms deploysAll

/-- info: '{ns}.deploysAt' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs (whitespace := lax) in
#print axioms deploysAt

end {ns}
""".replace("§", "\u0060")


def lifecycle_all(
    lean_dir,
    name,
    source_text,
    contract,
    init,
    plain,
    deployed,
    ws,
    storage,
    rec_dir,
    cache,
):
    """The all-arguments stage, recorded beside a proved lifecycle module and never changing its
    verdict: {"ok", "detail", "statement"?, "lean_sha256"?}. ok when the base has the symbolic
    creation frame and the run's clone builds Certified.NAME.LifecycleAll."""
    if not (lean_dir / "Verify" / "Lifecycle" / "Symbolic.lean").exists():
        return {
            "ok": False,
            "detail": "the base has no symbolic creation frame (Verify/Lifecycle/Symbolic.lean)",
        }
    ns = f"Certified.{name}.LifecycleAll"
    text = lifecycle_all_lean(ns, contract, init, plain, deployed, ws, storage)
    (rec_dir / f"{name}.LifecycleAll.lean").write_text(text)
    ok, out = lifecycle_build(lean_dir, name, source_text, "LifecycleAll", text, cache)
    rec = {"ok": ok, "lean_sha256": hashlib.sha256(text.encode()).hexdigest()}
    if ok:
        rec["statement"] = (
            f"{ns}.deploysAll : ∀ as, as.length = nArgs → (∀ e ∈ specPre frameFuel init nArgs, e.eval as = 0) → "
            "Deploys (initArgs init (as.map BitVec.toNat)) (codeOf as (specCode frameFuel init nArgs)) "
            "(storeOf as (specStore frameFuel init nArgs))"
        )
        rec["detail"] = (
            "the kernel proves every creation with this many argument words that meets the symbolic "
            "run's path conditions, and this run's creation as its instance (deploysAt); creations "
            "that revert are outside it"
        )
    else:
        errs = " | ".join(l for l in out.splitlines() if "error" in l)[:400]
        rec["detail"] = (
            f"the symbolic creation frame does not prove every creation: {errs}"
        )
        rec["log"], rec["log_tail"] = keep_log(f"{name}.LifecycleAll", out)
    return rec


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
    bind=None,
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
        bind=bind,
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
    rok, rout = build("LifecycleRefute", rtext)
    lname = ns.split(".")[1]
    logs = {
        "logs": {
            "Lifecycle": keep_log(f"{lname}.Lifecycle", out)[0],
            "LifecycleRefute": None
            if rok
            else keep_log(f"{lname}.LifecycleRefute", rout)[0],
        },
        "log_tail": "\n".join(out.splitlines()[-60:]),
        "refute_log_tail": None if rok else "\n".join(rout.splitlines()[-60:]),
    }
    if rok:
        return {
            "stage": "kernel",
            "ok": False,
            "verdict": "refuted",
            "lean": rtext,
            "detail": "the kernel proves the creation does not deploy this code and storage (not_deploys_of_refutes)",
            **logs,
        }
    errs = " | ".join(l for l in out.splitlines() if "error" in l)[:400]
    return {
        "stage": "kernel",
        "ok": False,
        "verdict": "outside",
        "detail": f"the kernel proves neither the deployment nor its refutation (a revert path, or a frame outside the shape): {errs}",
        **logs,
    }


def seal_record(rec_dir, record, status, **extra):
    """Write RECORD as REC_DIR/record.json with STATUS: when it has a run manifest (run_files), every
    file the driver left in REC_DIR but record.json is added to it with its sha256."""
    record.update(extra)
    if record.get("run_files") is not None:
        for q in sorted(rec_dir.rglob("*")):
            rel = str(q.relative_to(rec_dir))
            if q.is_file() and rel != "record.json" and rel not in record["run_files"]:
                record["run_files"][rel] = sha256_file(q)
    record["status"] = status
    record["finished"] = datetime.datetime.now(datetime.UTC).isoformat(
        timespec="seconds"
    )
    (rec_dir / "record.json").write_text(json.dumps(record, indent=2) + "\n")


# The lifecycle tier's compared bytes, kept in the record directory (run_files) beside the record.
LIFECYCLE_FILES = {
    "init": "lifecycle/init.hex",
    "creation": "lifecycle/creation.hex",
    "bin_runtime": "lifecycle/bin-runtime.hex",
    "bind": "lifecycle/bind.json",
}


def lifecycle_manifest(
    record,
    lean_src,
    lean_dir,
    base,
    name,
    contract,
    src,
    src_sha,
    rec_dir,
    init,
    creation,
    plain,
):
    """The lifecycle run's manifest, as the certified path writes it: the generated inputs of the
    run's clone (Certified/NAME/ and the lakefile stanza) committed on BASE and checked by
    checkout.sh, solidity_lean.inputs/inputs_sha256/base_tree/executed_tree, the source and inputs
    copied into REC_DIR, the base's pinned packages, and the creation, init and runtime bytes and
    the binding it compared (LIFECYCLE_FILES). seal_record then lists every file in run_files."""
    inputs, changed = commit_inputs(lean_dir, base, name, contract)
    tree = git(lean_dir, "rev-parse", inputs + "^{tree}")
    record["solidity_lean_source"] = str(lean_src)
    record["solidity_lean"] = {
        "base": base,
        "inputs_commit": inputs,
        "inputs_tree": tree,
        "provenance": f"solidity-lean {base} + inputs {inputs} (tree {tree}, Certified/{name} only)",
        "inputs": changed,
        "inputs_sha256": {p: sha256_file(lean_dir / p) for p in changed},
    }
    shutil.copy(src, rec_dir / src.name)
    for q in changed:
        (rec_dir / "inputs" / q).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(lean_dir / q, rec_dir / "inputs" / q)
    record["run_files"] = {
        src.name: src_sha,
        **{f"inputs/{q}": sha256_file(lean_dir / q) for q in changed},
    }
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
    (rec_dir / "lifecycle").mkdir(exist_ok=True)
    (rec_dir / LIFECYCLE_FILES["init"]).write_text(init.hex() + "\n")
    (rec_dir / LIFECYCLE_FILES["creation"]).write_text(creation.hex() + "\n")
    (rec_dir / LIFECYCLE_FILES["bin_runtime"]).write_text(plain.lower() + "\n")
    bind = (record.get("lifecycle") or {}).get("bind")
    if bind is not None:
        (rec_dir / LIFECYCLE_FILES["bind"]).write_text(
            json.dumps(bind, indent=1) + "\n"
        )


def lifecycle_run(
    a, record, solar, src, src_sha, name, contract, ast, work, finish, fns, rec_dir
):
    """--tier lifecycle: the creation stages, ending UNSUPPORTED-lifecycle (exit 4) at the first
    stage with no kernel proof, REJECTED (exit 1) at a stage that disagrees or a creation the
    kernel refutes, CERTIFIED (exit 0) only when the kernel proves the generated lifecycle module."""
    if linearization_module(name, contract, ast, src_sha):
        # This route builds its own modules and never reaches scripts/certify, which is what binds,
        # builds and audits an inherited contract's C3 theorem (Linearization.lean): refused here
        # rather than certified over a flattening nothing checked.
        raise Blocked(
            [
                {
                    "feature": "inheritance: lifecycle tier",
                    "where": "",
                    "detail": f"{contract} has bases, and the lifecycle tier does not build its C3 "
                    "theorem (Certified/NAME/Linearization.lean) yet",
                }
            ]
        )
    types = constructor_params(ast, contract)
    ws = constructor_words(types, a.constructor_args)
    bind_file = getattr(a, "lifecycle_bind", None)
    bind = lifecycle_binding(bind_file, ws) if bind_file else None
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
    # The canonical library stanza (the one the inputs commit may add), before any module build.
    fresh_certified(lean_dir, name, contract)
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
    sealed = finish

    def finish(status, code, line, **extra):
        # Every outcome from here on leaves the run's manifest, so --replay can check it.
        (lean_dir / "Certified" / name).mkdir(parents=True, exist_ok=True)
        (lean_dir / "Certified" / name / "Source.lean").write_text(source_lean(body))
        try:
            lifecycle_manifest(
                record,
                lean_src,
                lean_dir,
                base,
                name,
                contract,
                src,
                src_sha,
                rec_dir,
                init,
                creation,
                plain,
            )
        except (Failed, OSError) as e:
            record.pop("run_files", None)
            record["lifecycle_manifest_error"] = str(e)
        sealed(status, code, line, **extra)

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
    if bind:
        life["bind"] = {k: bind[k] for k in BIND_KEYS}
        ok, out = lifecycle_build(
            lean_dir,
            name,
            source_lean(body),
            "SourceSame",
            source_same_lean(name, bind),
            a.probe_cache or None,
        )
        life["source_same"] = ok
        if not ok:
            errs = " | ".join(l for l in out.splitlines() if "error" in l)[:300]
            log_name, tail = build_failure(rec_dir, name, "SourceSame", out)
            raise Blocked(
                [
                    {
                        "feature": "lifecycle binding",
                        "where": "",
                        "detail": f"this run's source unit is not the expected theorem's ({bind['unit']}): {errs}",
                        "log": log_name,
                        "log_tail": tail,
                    }
                ]
            )
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
        bind,
    )
    stages.append(
        {
            k: kq[k]
            for k in ("stage", "ok", "detail", "logs", "log_tail", "refute_log_tail")
            if k in kq
        }
    )
    log(f"lifecycle kernel: {'YES' if kq['ok'] else 'NO'}: {kq['detail']}")
    if "lean" in kq:
        f = rec_dir / f"{name}.{'Lifecycle' if kq['ok'] else 'LifecycleRefute'}.lean"
        f.write_text(kq["lean"])
        life["lean_sha256"] = hashlib.sha256(kq["lean"].encode()).hexdigest()
    if kq["verdict"] == "proved":
        life["statement"] = (
            f"Certified.{name}.Lifecycle.statement : ∃ c, contract? = .ok c ∧ Verify.Lifecycle.Statement fuel c init code ws storageLit"
        )
        life["all_args"] = lifecycle_all(
            lean_dir,
            name,
            source_lean(body),
            contract,
            init,
            plain_b,
            bytes.fromhex(res["runtime"]),
            ws,
            storage_list(res.get("storage")),
            rec_dir,
            a.probe_cache or None,
        )
        log(
            f"lifecycle all arguments: {'YES' if life['all_args']['ok'] else 'NO'}: {life['all_args']['detail']}"
        )
        if bind:
            text = composed_lean(name, bind)
            (rec_dir / f"{name}.Composed.lean").write_text(text)
            ok, out = lifecycle_build(
                lean_dir,
                name,
                source_lean(body),
                "Composed",
                text,
                a.probe_cache or None,
            )
            life["composed"] = {
                "theorem": f"Certified.{name}.Composed.composed",
                "lean_sha256": hashlib.sha256(text.encode()).hexdigest(),
                "built": ok,
                # One witness c for every conjunct; the admission is the expected theorem's.
                "conjuncts": [
                    "Expected",
                    "checked",
                    "CreatedBy",
                    "Statement",
                    "Deploys",
                ],
                "contract": f"{paren(bind['source'])}.checked",
                "created_by": f"{paren(bind['source'])}.CreatedBy c {bind['init']} "
                f"(Verify.Lifecycle.initArgs {bind['init']} {bind['ws']})",
                "ctor_args": bind["ws"],
            }
            log(
                f"lifecycle composed with the expected theorem: {'YES' if ok else 'NO'}"
            )
            if not ok:
                errs = " | ".join(l for l in out.splitlines() if "error" in l)[:400]
                life["composed"]["log"], life["composed"]["log_tail"] = build_failure(
                    rec_dir, name, "Composed", out
                )
                finish(
                    "BLOCKED",
                    2,
                    f"BLOCKED: lifecycle: the composed theorem does not build: {errs}",
                    tier_reached="lifecycle",
                )
        opened = record.get("lifecycle_open") or []
        loops_note = (
            f"; runtime loops OPEN, outside this tier ({', '.join(o['where'] for o in opened)}): "
            "their fixed-fuel resource obligation is not discharged"
            if opened
            else ""
        )
        if opened and bind:
            lr = lifecycle_loop_resources(
                a, solar, src, name, contract, ast, fns, bind, lean_dir, body, rec_dir
            )
            life["loop_resources"] = lr
            log(f"lifecycle loop resources: {lr['status']}: {lr['detail']}")
            if lr["status"] == "subedge":
                loops_note = (
                    f"; runtime loop ({', '.join(o['where'] for o in opened)}) resources: the final "
                    f"EVM IR subedge is proved on these bytes ({lr['theorem']}); the source join stays "
                    f"OPEN: {LOOP_SOURCE_JOIN}"
                )
        finish(
            "CERTIFIED",
            0,
            f"CERTIFIED: lifecycle: {kq['detail']}" + loops_note,
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


# What the lifecycle tier's loop subedge leaves open. The source side exists in the base:
# eraseLoopStmt erases a counted loop and bodyRuns_of_eraseLoop gives the BodyRuns that
# Verify.LoopSource.Equiv.loop_stage needs; the loop cut hook (loop_cuts.py) generates the erasure,
# its tree and obligations on a run's own source. This tier runs neither that hook nor the join
# (EraseLoopJoin.GeneratedTarget) of those premises with this subedge.
LOOP_SOURCE_JOIN = (
    "the source side of the loop is not composed with this subedge at this tier: the loop cut "
    "hook's erasure-route obligations (eraseLoopStmt, bodyRuns_of_eraseLoop) and their join "
    "(EraseLoopJoin.GeneratedTarget) are not part of the lifecycle statement"
)

# The composed theorem's binder the joint consumer reuses: one iteration bound K under a named bound.
JOINT_BINDERS = re.compile(r"\{K : Nat\} \(hK : K ≤ (" + LEAN_IDENT + r")\)")


def lifecycle_loop_resources(
    a, solar, src, name, contract, ast, fns, bind, lean_dir, body, rec_dir
):
    """The runtime loop's fixed-fuel resources at the lifecycle tier
    (tools/certify/loop_resources.py): the generated module Certified/NAME/LoopResources.lean,
    built in the run's clone after Composed. status is "subedge" when it builds (the final EVM IR's
    Shapes.Resources on this run's bytes, joined with the composed theorem), "unsupported" when the
    loop is outside the generator's shape, and "failed" when the kernel refuses the module."""
    import loop_resources

    m = JOINT_BINDERS.fullmatch(bind["expected"]["binders"])
    if not m:
        return {
            "status": "unsupported",
            "detail": "the composed theorem's binders are not one iteration bound K",
        }
    joint = {
        "composed": f"Certified.{name}.Composed.composed",
        "bound": m.group(1),
        "code_same": f"Certified.{name}.Lifecycle.code_same",
        "runtime": bind["runtime"],
        "import": f"{name}.Composed",
    }
    abi = [
        {
            "signature": f["sig"],
            "selector": f"0x{f['sel']:08x}",
            "params": [
                t for t in f["sig"].split("(", 1)[1].rstrip(")").split(",") if t
            ],
        }
        for f in fns
    ]
    try:
        with tempfile.TemporaryDirectory() as tmp:
            text, facts = loop_resources.for_run(
                solar,
                src,
                contract,
                ast,
                abi,
                tmp,
                ns=f"Certified.{name}.LoopResources",
                code_term=f"Certified.{name}.Lifecycle.code",
                code_import=f"{name}.Lifecycle",
                joint=joint,
            )
    except loop_resources.Unsupported as e:
        return {"status": "unsupported", "detail": str(e)}
    runtime = facts.pop("runtime")
    (rec_dir / f"{name}.LoopResources.lean").write_text(text)
    ok, out = lifecycle_build(
        lean_dir, name, source_lean(body), "LoopResources", text, a.probe_cache or None
    )
    rec = {
        "status": "subedge" if ok else "failed",
        "theorem": f"Certified.{name}.LoopResources.joint",
        "resources": f"Certified.{name}.LoopResources.resources_evmir",
        "lean_sha256": hashlib.sha256(text.encode()).hexdigest(),
        "export_runtime_sha256": hashlib.sha256(bytes.fromhex(runtime)).hexdigest(),
        "obligation": facts,
        "source_join": LOOP_SOURCE_JOIN,
        "detail": (
            "Shapes.Resources of the final EVM IR on Valid K, K ≤ Kmax, joined with the composed theorem"
            if ok
            else "the kernel refuses the generated loop-resource module"
        ),
    }
    if not ok:
        rec["log"], rec["log_tail"] = build_failure(rec_dir, name, "LoopResources", out)
    return rec


def admit_lake_jobs():
    """Refuse the run unless CERTIFY_LAKE_JOBS Lean processes of LEAN_MEMORY MB at once fit the
    memory limit of the driver's cgroup (tools/certify/admit.sh, which scripts/certify runs too):
    above one job, an unreadable or unlimited memory.max refuses."""
    jobs = os.environ.get("CERTIFY_LAKE_JOBS", "1")
    rc, _, err = run(["bash", str(HERE / "admit.sh"), jobs, LEAN_MEMORY])
    if rc != 0:
        sys.exit(err.strip() or "certify: error: CERTIFY_LAKE_JOBS is not admitted")


def lake_jobs_env(lean_dir, env):
    """ENV for a lake build of LEAN_DIR with CERTIFY_LAKE_JOBS lean processes at once.

    lake is a Lean program whose build scheduler runs on Lean's task pool, which LEAN_NUM_THREADS
    sizes, so with LEAN_NUM_THREADS=1 it compiles one module at a time. At CERTIFY_LAKE_JOBS 1
    (the default) ENV is returned unchanged. Above 1, lake gets that many threads and runs lean
    from tools/certify/sysroot.sh's threads mode, which gives each lean ENV's LEAN_NUM_THREADS
    back."""
    jobs = os.environ.get("CERTIFY_LAKE_JOBS", "1")
    if not re.fullmatch(r"[1-9][0-9]*", jobs):
        sys.exit("generic: CERTIFY_LAKE_JOBS must be a positive integer")
    if jobs == "1":
        return env
    rc, out, err = run([*LAKE, "env", "which", "lean"], cwd=lean_dir, env=env)
    if rc != 0:
        sys.exit(f"generic: cannot find lean: {err.strip()}")
    tc = pathlib.Path(out.strip()).resolve().parent.parent
    sysroot = pathlib.Path(lean_dir) / ".lake" / "certify-sysroot-threads"
    rc, out, err = run(
        ["bash", str(HERE / "sysroot.sh"), str(tc), str(sysroot), "threads"]
    )
    if rc != 0:
        sys.exit(f"generic: cannot set up the threads sysroot: {err.strip()}")
    return dict(
        env,
        PATH=f"{out.strip()}:{env.get('PATH', '')}",
        LEAN_NUM_THREADS=jobs,
        CERTIFY_LEAN_THREADS=env.get("LEAN_NUM_THREADS", "1"),
    )


def lifecycle_build(lean_dir, name, source_text, mod, text, cache):
    """Build the run's module Certified/NAME/MOD.lean (with Source.lean) in the run's clone of the
    base, one Lean thread at a time: (ok, output)."""
    d = lean_dir / "Certified" / name
    d.mkdir(parents=True, exist_ok=True)
    (d / "Source.lean").write_text(source_text)
    (d / f"{mod}.lean").write_text(text)
    # A base that already declares NAME as lakefile_lib does keeps that stanza.
    declare_lean_lib(
        lean_dir / "lakefile.lean",
        name,
        f'\nlean_lib «{name}» where\n  srcDir := "Certified"\n  globs := #[.submodules `{name}]\n',
        (lean_lib_body(name),),
    )
    if cache and not (lean_dir / ".lake").exists():
        rc, _, err = run(
            ["cp", "-a", str(pathlib.Path(cache) / ".lake"), str(lean_dir / ".lake")]
        )
        if rc != 0:
            raise Failed(f"cannot copy the probe cache: {err[:200]}")
    env = lake_jobs_env(lean_dir, dict(os.environ, LEAN_NUM_THREADS="1"))
    rc, out, err = run([*LAKE, "build", f"{name}.{mod}"], cwd=lean_dir, env=env)
    if rc != 0:
        keep_log(f"{name}.{mod}", out + err)
    return rc == 0, out + err


# The lean memory cap (MB) of the generated certification library's builds: its Source, the
# probes', and every lifecycle module (SourceSame, Lifecycle, LifecycleRefute, LifecycleAll,
# Composed) with their kernel checks. Counter's Lifecycle peaks at 4.88 GB unbounded and fails the
# kernel at -M 6000; it passes at 10000.
GEN_LEAN_MEMORY = 12000
# The lake options every lake call on the run's clones passes first. The lakefile reads leanMemory
# with get_config? into weakLeanArgs, which lake applies only when it elaborates the lakefile: -R
# makes every call elaborate it with this -K, so a configuration cached by an earlier call without
# it (or with another value) never runs the generated modules with no -M or another one.
LAKE = ["lake", "-R", f"-KleanMemory={GEN_LEAN_MEMORY}"]


# The record directory failed lake builds keep their whole output in (main sets it); None keeps none.
BUILD_LOG_DIR = None


def keep_log(label, out, lines=60):
    """A failed lake build's (or lake-run probe's) whole output, kept as LABEL.build.log in
    BUILD_LOG_DIR, and its last LINES lines: (the log's name or None, the tail)."""
    tail = "\n".join((out or "").splitlines()[-lines:])
    if BUILD_LOG_DIR is None:
        return None, tail
    log_name = f"{label}.build.log"
    (pathlib.Path(BUILD_LOG_DIR) / log_name).write_text(out or "")
    return log_name, tail


def build_failure(rec_dir, name, mod, out, lines=60):
    """A failed lifecycle build's whole lake output, kept as NAME.MOD.build.log in the record
    directory, and its last LINES lines: lake prints a failed module's own messages (such as Lean's
    memory_exception and its exit code) in that module's block, and they need not say "error"."""
    log_name = f"{name}.{mod}.build.log"
    (rec_dir / log_name).write_text(out)
    return log_name, "\n".join(out.splitlines()[-lines:])


def frontend(lean_dir, src, src_sha, name, contract, solc):
    """The solc-AST front end's Lean body of the source. The front end records the absolute path it
    read the source from (importedSourceName), so it reads a copy at a path fixed by the source's
    digest: the body is then the same wherever the source sits, as a replay from another directory
    needs."""
    fe = pathlib.Path("/tmp/solar-certify-src") / src_sha
    fe.mkdir(parents=True, exist_ok=True)
    shutil.copy(src, fe / f"{name}.sol")
    rc, body, err = run(
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
    return rc, body, err


def linearization_module(name, contract, ast, src_sha):
    """The text of Certified/NAME/Linearization.lean for CONTRACT of AST, the solc AST of the run's
    source (SHA-256 SRC_SHA) that the envelope read, or "" for a contract without bases. Its
    theorem linearization_eq, kernel-checked by decide, states that the Lean frontend's C3 order of
    CONTRACT in the bound source unit (ContractDecl.dispatchOrder? over NAME.Source's
    importedSourceUnit: the order CheckedInput.contract flattens it by, so the one the published
    statement's src.checked uses) is AST's linearizedBaseContracts, the order the envelope and its
    Dispatch checked the contract by. NAME.Source stays exactly the frontend's output
    (source_bind.sh). scripts/certify regenerates this module from the run's source with the pinned
    solc (linearization_bind.sh, linearization.py), compares it byte for byte, then builds it and
    audits the theorem. A contract with bases whose order the driver does not certify raises
    Failed, so no contract with bases goes without the theorem."""
    cdef = next(
        (
            n
            for n in ast.get("nodes", [])
            if n.get("nodeType") == "ContractDefinition" and n.get("name") == contract
        ),
        None,
    )
    if cdef is None:
        raise Failed(f"linearization: no contract {contract} in the source")
    if not cdef.get("baseContracts"):
        return ""
    lin, why = linearization(ast, cdef)
    if lin is None:
        raise Failed(f"linearization: {why}")
    try:
        lakefile_inputs.check_name(name)
    except lakefile_inputs.Refused as e:
        raise Failed(f"linearization: {e}") from None
    order = ", ".join(lean_str(c["name"]) for c in lin)
    unit = f"Certified.{name}.Src.importedSourceUnit"
    return (
        "-- Generated by solar's tools/certify/generic.py (tools/certify/linearization.py) from "
        f"{name}.sol (sha256 {src_sha}) and solc's AST of it; do not edit.\n"
        f"\nimport {name}.Source\n\nopen SolidCore.Solidity\n\n"
        f"namespace Certified.{name}.Linearization\n\n"
        f"/-- The Lean frontend flattens {contract} (CheckedInput.contract) over solc's C3 order of it\n"
        "(linearizedBaseContracts in the AST the driver's envelope checked). -/\n"
        "theorem linearization_eq :\n"
        f"    ((Executable.SourceUnit.findContract? {unit} {lean_str(contract)}).bind\n"
        f"      (Executable.ContractDecl.dispatchOrder? (Executable.SourceUnit.contracts {unit}))).map\n"
        f"      (List.map (fun d : ContractDecl => d.name)) = some [{order}] := by decide\n"
        f"\nend Certified.{name}.Linearization\n"
    )


def source_lean(body):
    return (
        "-- Generated by solar's tools/certify/generic.py with scripts/solc_ast_to_lean_source.py; do not edit.\n"
        "\nimport SolidCore.Solidity.Checked\n\nopen SolidCore.Solidity\n\n" + body
    )


def shape_json(
    name,
    contract,
    fns,
    gas,
    spec_desc,
    spec_witness=None,
    events=(),
    any_gas=None,
    succeeds_witness=None,
):
    ns = f"Certified.{name}.Lowering"
    ctor = getattr(fns, "ctor", None)
    storage = f"{ns}.storageLit"
    if ctor is not None:
        # Every argument list: the statement quantifies the words A0, A1, ... each below 2 ^ width,
        # against the source deployed with them (final.py's ctor_args), from any storage covering
        # the slots that deployment writes.
        if spec_witness is not None or any_gas is not None:
            raise Failed(
                "constructor parameters: stated on the compiled tier with the fixed caller only"
            )
        args = [f"A{i}" for i in range(len(ctor["params"]))]
        storage = f"{ns}.storageOf [{', '.join(args)}]"
    valid = f"SolidCore.Refinement.Shapes.genericValid ({ns}.fns) ({storage})"
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
        "export_ns": export_ns.of_run(name, contract),
        "generic": {
            "lowering": f"{name}.Lowering",
            "ns": ns,
            "style": "core",
            "check": "check_stage0",
            "check_value": "checkValue_stage0",
            "fns": f"{ns}.fns",
            "storage": storage,
        },
        "excluded": [
            "deployment: the init code is not covered; the statement is about the runtime bytecode from any storage that holds the slots the constructor writes, as SolidCore's constructor run with no arguments leaves them",
            "source model trusted: SolidCore's checked contract (the solc AST through scripts/solc_ast_to_lean_source.py and the type checker) is the meaning of the source (frontend trust boundary)",
            CALLS_FIXED,
            "gas: the bound is the loop checker's (22100 per item times the final EVM IR's block fuel, plus memory), coarse; below it the bytecode may fail without effects, which equivalence admits",
            f"valid calls: {sigs} ABI-encoded with clean argument words, any value below 2^256 (no function is payable: the value reverts on both sides), any storage covering the constructor's slots; other call data is outside the statement; payable functions not exercised",
        ],
        "source_ast": {
            "file": f"Certified/{name}/Source.lean",
            "namespace": f"Certified.{name}.Src",
            "contract": contract,
        },
    }
    if ctor is not None:
        shape["ctor_args"] = [
            {"var": v, "lt": f"2 ^ {p['width']}"} for v, p in zip(args, ctor["params"])
        ]
        shape["generic"]["ctor"] = {
            "args": args,
            "widths": [p["width"] for p in ctor["params"]],
            "branches": bool(ctor.get("branches")),
        }
        if ctor.get("branches"):
            # The constructor may revert (its erasure branches to reverts): the statement covers
            # the argument lists on which SolidCore's constructor returns, and states one.
            shape["ctor_returns"] = {"witness": ctor["witness"]}
            shape["excluded"].insert(
                1,
                "creations that revert: the statement covers the argument lists on which SolidCore's "
                "constructor returns (CtorReturns, a premise; the witness "
                + str(ctor["witness"])
                + " is one); on the others the source deploys nothing",
            )
        shape["excluded"][0] = (
            "deployment: the init code is not covered; the statement is about the runtime bytecode, "
            "for every constructor argument list ("
            + ", ".join(
                f"{v} for {p['name']} : {p['type']}, below 2^{p['width']}"
                for v, p in zip(args, ctor["params"])
            )
            + "), against SolidCore's contract deployed by its constructor on those words "
            "(solidCoreFromArgs), from any storage that holds the slots that constructor writes"
        )
        shape["excluded"][-1] = shape["excluded"][-1].replace(
            "the constructor's slots", "the slots the constructor writes on the arguments"
        )
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
        if succeeds_witness is not None:
            if succeeds_witness["function"] != spec_witness:
                raise Failed("the succeeds witness must be the spec witness")
            shape["succeeds_witness"] = succeeds_witness
            shape["excluded"].append(
                "spec without succeeds-if conditions: the statement's allowed statuses and postcondition "
                "are the export's, each with the driver's witness clause (tools/certify/succeeds_witness.py): "
                f"{succeeds_witness['function']} on zero words with no value, from the constructor's slots "
                "written with 0, from a pre-state meeting its precondition, succeeds; no other call is "
                "required to succeed"
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
        shape["any_caller"] = {
            "module": f"{name}.AnyCaller",
            "proof": f"Certified.{name}.AnyCaller.published_any",
            "gas": f"fun _ => {any_gas}",
        }
        shape["excluded"] = [
            CALLS_ANY if e == CALLS_FIXED else e for e in shape["excluded"]
        ] + [callers_any(any_gas)]
    return shape


# --- the run ----------------------------------------------------------------------------------


def git(d, *a, check=True):
    rc, out, err = run(["git", "-C", str(d), *a])
    if check and rc != 0:
        raise Failed(f"git {' '.join(a)} in {d}: {err.strip()[:300]}")
    return out.strip()


def changed_paths(d):
    """Every path git status reports in the checkout d, tracked or not, both sides of a rename,
    read from --porcelain -z so that no path is sliced out of stripped or quoted output."""
    rc, out, err = run(
        ["git", "-C", str(d), "status", "--porcelain", "-z", "--untracked-files=all"]
    )
    if rc != 0:
        raise Failed(f"git status in {d}: {err.strip()[:300]}")
    paths, entries = [], iter(out.split("\0"))
    for e in entries:
        if not e:
            continue
        # Each entry is XY, a space and the path; a rename or copy is followed by its source path.
        if len(e) < 4 or e[2] != " ":
            raise Failed(f"git status in {d}: unreadable entry {e[:80]!r}")
        paths.append(e[3:])
        if "R" in e[:2] or "C" in e[:2]:
            paths.append(next(entries, ""))
    return paths


def changed_outside(d, prefix):
    """The paths changed in the checkout d (see changed_paths) that are not under prefix."""
    return [q for q in changed_paths(d) if not q.startswith(prefix)]


SOLAR_BUILD_CMD = [
    "cargo",
    "build",
    "--locked",
    "--bin",
    "solar",
    "--message-format=json-render-diagnostics",
]
# The version of the build provenance a record carries (solar.build); --replay refuses a record
# without it, or with another version, rather than filling it in.
PROVENANCE_VERSION = 2
# The one supported build configuration: the dev profile for the host, no --target, no wrapper, no
# flags beyond the checkout's own. Environment that would change any of that is refused; these are
# the Cargo and rustc variables the build may see and records (none changes the compiled code).
CARGO_ENV_ALLOWED = (
    "CARGO_BUILD_JOBS",
    "CARGO_INCREMENTAL",
    "CARGO_TARGET_DIR",
    "CARGO_HOME",
    "CARGO_NET_OFFLINE",
    "CARGO_TERM_COLOR",
    "CARGO_TERM_PROGRESS_WHEN",
    "CARGO_TERM_VERBOSE",
    "CARGO_TERM_QUIET",
    "CARGO_REGISTRIES_CRATES_IO_PROTOCOL",
    "CARGO_HTTP_TIMEOUT",
    "CARGO_NET_RETRY",
)
CARGO_ENV_REFUSED = (
    "RUSTC",
    "RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
    "RUSTFLAGS",
    "CARGO_ENCODED_RUSTFLAGS",
    "RUSTC_BOOTSTRAP",
    "RUSTC_FORCE_INCREMENTAL",
)
# Config keys that change what Cargo builds or how; refused in any config the build reads, except
# the checkout's own .cargo/config.toml, which its tree binds and which may set [env] and tables for
# targets other than the host.
CARGO_CONFIG_REFUSED_BUILD = (
    "target",
    "rustflags",
    "rustc",
    "rustc-wrapper",
    "rustc-workspace-wrapper",
    "target-dir",
    "rustdocflags",
)
# The artifact the run's own build produced (build_solar): path, sha256, mtime_ns, fresh.
SOLAR_ARTIFACT = None


def cargo_env_problem(env):
    """Why ENV is not the supported build configuration's, or None: a wrapper, an overriding rustc,
    flags, another target, a profile override or an unstable switch."""
    for k in sorted(env):
        if k in CARGO_ENV_REFUSED:
            return f"{k} is set: the build supports no rustc override, wrapper or extra flags"
        if (
            k.startswith(
                (
                    "CARGO_BUILD_",
                    "CARGO_TARGET_",
                    "CARGO_PROFILE_",
                    "CARGO_UNSTABLE_",
                    "CARGO_ENCODED_",
                    "__CARGO_",
                )
            )
            and k not in CARGO_ENV_ALLOWED
        ):
            return f"{k} is set: the build supports only the dev profile for the host, unwrapped"
    return None


def cargo_configs(root, cargo_home):
    """Every Cargo config file a build in ROOT reads: ROOT's and each parent's .cargo/config and
    .cargo/config.toml, then CARGO_HOME's, with the scope each binds as (repo, parent, cargo_home)."""
    root = pathlib.Path(root).resolve()
    out = []
    for i, d in enumerate([root, *root.parents]):
        for n in ("config", "config.toml"):
            f = d / ".cargo" / n
            if f.is_file():
                out.append(("repo" if i == 0 else "parent", f))
    for n in ("config", "config.toml"):
        f = pathlib.Path(cargo_home) / n
        if f.is_file() and all(f.resolve() != g.resolve() for _, g in out):
            out.append(("cargo_home", f))
    return out


def cargo_config_problem(root, cargo_home, host):
    """Why a Cargo config the build reads changes the supported configuration, or None, with the
    configs bound as inputs [{scope, name, sha256}]. Refused anywhere: [build] target, rustflags,
    rustc, wrappers, target-dir; any [profile.*]; [unstable]; a [target] table for the host or a
    cfg(...) one. Outside the checkout's own config also [env] and any [target] table."""
    import tomllib

    bound = []
    for scope, f in cargo_configs(root, cargo_home):
        try:
            c = tomllib.loads(f.read_text())
        except (OSError, ValueError) as e:
            return f"Cargo config {f} cannot be read: {e}", bound
        where = f"Cargo config {f} ({scope})"
        for k in CARGO_CONFIG_REFUSED_BUILD:
            if k in c.get("build", {}):
                return f"{where} sets build.{k}", bound
        for k in ("profile", "unstable"):
            if k in c:
                return f"{where} sets [{k}]", bound
        for t in c.get("target", {}):
            if scope != "repo" or t == host or t.startswith("cfg("):
                return f"{where} sets [target.{t}]", bound
        if scope != "repo":
            # Dependency overrides swap crate code without changing Cargo.lock (--locked does not
            # see them); only the checkout's own config, which its tree binds, may hold them.
            for k, label in (
                ("env", "[env]"),
                ("paths", "paths"),
                ("patch", "[patch]"),
                ("source", "[source]"),
            ):
                if k in c:
                    return f"{where} sets {label}", bound
        bound.append({"scope": scope, "name": f.name, "sha256": sha256_file(f)})
    return None, bound


def build_solar():
    """This checkout's solar, built in the one supported configuration, and the artifact Cargo
    reports producing (SOLAR_ARTIFACT): refused unless it is the expected path, before it runs."""
    global SOLAR_ARTIFACT
    SOLAR_ARTIFACT = None
    if why := cargo_env_problem(os.environ):
        raise Failed(f"solar build: {why}")
    # The configs Cargo would read, checked before it runs (rustc -vV only names the host).
    rc, vv, err = run(["rustc", "-vV"], cwd=ROOT)
    host = re.search(r"^host: (\S+)$", vv or "", re.MULTILINE)
    if rc != 0 or not host:
        raise Failed(f"solar build: rustc -vV fails: {err.strip()[:200]}")
    cargo_home = os.environ.get("CARGO_HOME") or str(pathlib.Path.home() / ".cargo")
    why, _ = cargo_config_problem(ROOT, cargo_home, host.group(1))
    if why:
        raise Failed(f"solar build: {why}")
    env = dict(os.environ, CARGO_BUILD_JOBS=os.environ.get("CARGO_BUILD_JOBS", "2"))
    rc, out, err = run(SOLAR_BUILD_CMD, cwd=ROOT, env=env)
    if rc != 0:
        raise Failed(f"cargo cannot build solar: {err.strip()[-400:]}")
    want = ROOT / os.environ.get("CARGO_TARGET_DIR", "target") / "debug" / "solar"
    exes = []
    for line in (out or "").splitlines():
        try:
            m = json.loads(line)
        except ValueError:
            continue
        t = m.get("target") or {}
        if (
            m.get("reason") == "compiler-artifact"
            and t.get("name") == "solar"
            and "bin" in t.get("kind", [])
        ):
            exes.append(m)
    if len(exes) != 1 or not exes[0].get("executable"):
        raise Failed(
            f"solar build: cargo reported {len(exes)} solar executables, not one"
        )
    exe = pathlib.Path(exes[0]["executable"])
    if exe.resolve() != want.resolve():
        raise Failed(
            f"solar build: cargo built {exe}, not the expected output {want} (another target or profile)"
        )
    st = exe.stat()
    SOLAR_ARTIFACT = {
        "path": str(exe),
        "sha256": sha256_file(exe),
        "mtime_ns": st.st_mtime_ns,
        "fresh": bool(exes[0].get("fresh")),
    }
    return want


def artifact_problem(solar):
    """Why SOLAR is not the artifact this run's build produced, unchanged since, or None."""
    a = SOLAR_ARTIFACT
    if a is None:
        return "the executable is not one this run's build produced"
    p = pathlib.Path(solar)
    if p.resolve() != pathlib.Path(a["path"]).resolve():
        return f"the executable {solar} is not the build's artifact {a['path']}"
    if p.stat().st_mtime_ns != a["mtime_ns"] or sha256_file(p) != a["sha256"]:
        return f"the build's artifact {a['path']} changed after the build"
    return None


def build_solar_verify():
    """This checkout's solar-verify, built as build_solar builds solar."""
    env = dict(os.environ, CARGO_BUILD_JOBS=os.environ.get("CARGO_BUILD_JOBS", "2"))
    rc, _out, err = run(
        ["cargo", "build", "--locked", "-p", "solar-verify", "--bin", "solar-verify"],
        cwd=ROOT,
        env=env,
    )
    if rc != 0:
        raise Failed(f"cargo cannot build solar-verify: {err.strip()[-400:]}")
    return (
        ROOT / os.environ.get("CARGO_TARGET_DIR", "target") / "debug" / "solar-verify"
    )


COMPILER_SOURCES = ("crates", "Cargo.toml", "Cargo.lock")


def solar_commit(version_out):
    """The commit a solar binary embeds, read from its --version output as scripts/certify reads
    it (the Commit SHA line, else the +commit. suffix of the version), or None."""
    if m := re.search(r"(?m)^Commit SHA: ([0-9a-f]{40})$", version_out):
        return m.group(1)
    m = re.search(r"\+commit\.([0-9a-f]{8})", version_out)
    return m.group(1) if m else None


def build_provenance(root, solar, version_out):
    """The solar build's inputs as this run observed them by running git, rustc, Cargo and the
    binary, never from labels a caller passes: the checkout's commit and tree (refused unless its
    compiler sources and .cargo are its commit's), its submodules (refused when one is modified or
    conflicted), the Cargo.lock digest, rustc -vV, the target (the host: no --target is passed and
    every way to choose another is refused), the profile and features the binary reports, the Cargo
    environment and configs the build read, the exact build command, and the artifact Cargo reported
    (which SOLAR must be, unchanged)."""
    root = pathlib.Path(root)
    if why := artifact_problem(solar):
        raise Failed(f"solar: {why}")
    if why := cargo_env_problem(os.environ):
        raise Failed(f"solar build: {why}")
    g = ["git", "-C", str(root)]
    if (
        run([*g, "diff", "--quiet", "HEAD", "--", *COMPILER_SOURCES, ".cargo"])[0] != 0
        or run(
            [
                *g,
                "diff",
                "--quiet",
                "--cached",
                "HEAD",
                "--",
                *COMPILER_SOURCES,
                ".cargo",
            ]
        )[0]
        != 0
    ):
        raise Failed(
            "solar: the compiler sources are not the checkout's commit (dirty checkout)"
        )
    rc, sub, err = run([*g, "submodule", "status", "--recursive"])
    if rc != 0:
        raise Failed(
            f"solar: cannot read the checkout's submodules: {err.strip()[:200]}"
        )
    bad = [l for l in sub.splitlines() if l[:1] in ("+", "U")]
    if bad:
        raise Failed(
            f"solar: a submodule is not its recorded commit: {bad[0].strip()[:120]}"
        )
    rc, vv, err = run(["rustc", "-vV"], cwd=root)
    host = re.search(r"^host: (\S+)$", vv or "", re.MULTILINE)
    if rc != 0 or not host:
        raise Failed(f"solar: rustc -vV fails: {err.strip()[:200]}")
    cargo_home = os.environ.get("CARGO_HOME") or str(pathlib.Path.home() / ".cargo")
    why, configs = cargo_config_problem(root, cargo_home, host.group(1))
    if why:
        raise Failed(f"solar build: {why}")
    prof = re.search(r"^Build Profile: (\S+)", version_out or "", re.MULTILINE)
    feat = re.search(r"^Build Features: (.*)$", version_out or "", re.MULTILINE)
    if not prof or not feat:
        raise Failed("solar: the binary does not report its build profile and features")
    if prof.group(1) != "debug":
        raise Failed(
            f"solar: the binary reports profile {prof.group(1)}, not the supported debug"
        )
    return {
        "provenance_version": PROVENANCE_VERSION,
        "commit": git(root, "rev-parse", "HEAD"),
        "tree": git(root, "rev-parse", "HEAD^{tree}"),
        "submodules": sorted(l.strip() for l in sub.splitlines() if l.strip()),
        "cargo_lock_sha256": sha256_file(root / "Cargo.lock"),
        "rustc_vv": vv.strip(),
        # The host triple, which is the build's target: no --target is passed and CARGO_BUILD_TARGET,
        # build.target and [target.<host>] are refused above.
        "target": host.group(1),
        "target_source": "host (no --target; CARGO_BUILD_TARGET and build.target refused)",
        "profile": prof.group(1),
        "features": feat.group(1).strip(),
        "rustflags": {
            k: os.environ.get(k)
            for k in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_RUSTFLAGS")
        },
        "command": SOLAR_BUILD_CMD,
        "cargo_env": {
            k: os.environ.get(k)
            for k in CARGO_ENV_ALLOWED
            if k not in ("CARGO_TARGET_DIR", "CARGO_HOME", "CARGO_BUILD_JOBS")
        },
        "cargo_config": configs,
        "artifact": {k: SOLAR_ARTIFACT[k] for k in ("sha256", "fresh")},
    }


def solar_problem(solar, root, want_sha=None, allow_foreign=False):
    """Why the solar binary SOLAR is not the compiler of the checkout ROOT, or None: its digest is
    not WANT_SHA (when given), or the commit it embeds is not a commit of ROOT whose compiler
    sources are ROOT's committed and unedited ones, the rule scripts/certify applies to the binary
    it builds (waived, as there, by ALLOW_FOREIGN: CERTIFY_ALLOW_FOREIGN_SOLAR=1)."""
    if want_sha and (got := sha256_file(solar)) != want_sha:
        return f"{solar} has digest {got}, not the expected {want_sha}"
    rc, out, _ = run([str(solar), "--version"])
    commit = solar_commit(out) if rc == 0 else None
    if commit is None:
        return f"cannot read the compiler commit from {solar} --version"
    g = ["git", "-C", str(root)]
    ok = (
        run([*g, "diff", "--quiet", "HEAD", "--", *COMPILER_SOURCES])[0] == 0
        and run([*g, "cat-file", "-e", f"{commit}^{{commit}}"])[0] == 0
        and run([*g, "diff", "--quiet", commit, "HEAD", "--", *COMPILER_SOURCES])[0]
        == 0
    )
    if not (ok or allow_foreign):
        return f"{solar} reports commit {commit}, whose compiler sources are not this checkout's committed ones"
    return None


def solar_runtime(solar, src, contract, extra=(), cwd=None):
    if cwd is not None and os.sep in str(solar):
        solar = os.path.abspath(solar)
    rc, out, err = run(
        [str(solar), str(src), *OPTIONS, "--emit=bin-runtime", *extra], cwd=cwd
    )
    if rc != 0:
        return None, err
    j = json.loads(out)
    for k, v in j["contracts"].items():
        if k.rsplit(":", 1)[1] == contract:
            return v.get("bin-runtime"), err
    return None, f"no contract {contract} in solar's output"


def run_name(src):
    """The run's name for SOURCE: its stem, made a Lean identifier, and its SHA-256's first 8 hex
    digits. The run's modules and its export's namespace (export_ns.py) are under it."""
    src = pathlib.Path(src)
    sha = hashlib.sha256(src.read_text().encode()).hexdigest()
    base = re.sub(r"[^a-z0-9_]", "_", src.stem.lower()).strip("_") or "c"
    if not base[0].isalpha():
        base = "c_" + base
    return f"{base}_{sha[:8]}"


IMPORT_PATH = re.compile(r"""\bimport\s+(?:[^;"']*?\bfrom\s+)?["']([^"']+)["']""")
COMMENT = re.compile(r"//[^\n]*|/\*.*?\*/", re.DOTALL)


def import_closure(src):
    """SOURCE (first) and the files its imports reach, resolved as solar resolves them without
    remappings: a path starting with ./ or ../ against the importing file's directory, any other
    against SOURCE's directory, where solar runs. A file that does not exist is left out, for solar
    to report."""
    src = pathlib.Path(src).resolve()
    seen, todo = [src], [src]
    while todo:
        f = todo.pop()
        for p in IMPORT_PATH.findall(COMMENT.sub(" ", f.read_text())):
            t = (f.parent if p.startswith(("./", "../")) else src.parent) / p
            t = pathlib.Path(os.path.normpath(t))
            if t.is_file() and t not in seen:
                seen.append(t)
                todo.append(t)
    return seen


def export_stages(solar, src, contract, tmp):
    """solar's runtime bytecode of the contract and its stages, exported to TMP/gen under the run's
    own namespace (export_ns.run_prefix of run_name). The manifest records solar's arguments and the
    sources' paths, so solar runs in TMP on a copy of the source and the files its imports reach,
    laid out as they are relative to each other, with every path relative to TMP: the manifest is
    then the same wherever TMP and the source sit."""
    tmp = pathlib.Path(tmp)
    files = import_closure(src)
    root = pathlib.Path(os.path.commonpath([f.parent for f in files]))
    for f in files:
        dest = tmp / f.relative_to(root)
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(f, dest)
    main = files[0].relative_to(root)
    cwd = tmp / main.parent
    gen = os.path.relpath(tmp / "gen", cwd)
    ns = export_ns.run_prefix(run_name(src))
    return solar_runtime(
        solar,
        main.name,
        contract,
        [f"-Zemit-lean-stages={gen}", f"-Zlean-stages-namespace={ns}"],
        cwd=cwd,
    )


def gas_need(solar, src, name, contract, tmp):
    """The loop checker's gas bound for this run's export, as shape_gen.py computes it."""
    import shape_gen

    gen = tmp / "gen"
    shutil.rmtree(tmp, ignore_errors=True)
    tmp.mkdir(parents=True)
    plain, err = export_stages(solar, src, contract, tmp)
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
    try:
        stages = shape_gen.last_stage((tmp / "Proofs" / "Stages.lean").read_text())
    except SystemExit as e:
        raise Failed(f"the probe's stage proofs: {e.code}") from None
    last_module = "Evm" + stages["last"][3:]
    last_text = (gen / contract / f"{last_module}.lean").read_text()
    return loop_checker_gas(last_text, bytes.fromhex(plain), stages["fuel"])


def loop_checker_gas(last_text, code, fuel):
    """The loop checker's gas bound for the final EVM IR on CODE at the last stage's FUEL. asm_cert
    refuses by SystemExit (it is also a command); here a refusal is a typed result, so the run's
    record handlers always see it: a stack with no bound (asm_cert.Unbounded) is BLOCKED, and an
    IR that does not lay over the bytecode is REJECTED."""
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
    try:
        inferred = asm_cert.infer(last_text, code)
    except asm_cert.Unbounded as e:
        raise Blocked(
            [
                {
                    "feature": "stack bound",
                    "where": "",
                    "detail": str(e.code).removeprefix("error: "),
                }
            ]
        ) from None
    except SystemExit as e:
        raise Failed(f"asm_cert.py: {e.code}") from None
    bmax = max(inferred["costs"]) if inferred["costs"] else 0
    words = inferred["top"] // 32
    return 22100 * (fuel * bmax) + 3 * words + words * words // 512


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


def loop_cuts(record, src, ast, a, work, name, src_sha):
    """The loop cut-point hook beside loop_resources (tools/certify/loop_cuts.py): for each
    function solar-verify's manifest gives a loop, the manifest, loop_equiv.py's certificate and
    the base's obligation modules, built in a clone of the base. Records loop_heads, loop_cuts
    (only the functions every step completed for) and loop_cuts_blocked, and returns the extra
    BLOCKED reasons. A function whose loop is certainly outside the source erasure's shape
    (loop_cuts.shape_refusals) stops at step "shape" before any build. The run stays BLOCKED in
    every case: the published statement does not apply the join yet."""
    import fixed_fuel
    import loop_cuts as lc

    try:
        contract = fixed_fuel.contract_of(ast, a.contract)
    except fixed_fuel.Unsupported as e:
        record["loop_cuts_blocked"] = {"*": {"step": "abi", "why": str(e)}}
        return lc.reasons({}, record["loop_cuts_blocked"])
    cdefs = [
        n
        for n in ast.get("nodes", [])
        if n.get("nodeType") == "ContractDefinition" and n.get("name") == contract
    ]
    refused = lc.shape_refusals(cdefs[0]) if len(cdefs) == 1 else {}

    def tool(build):
        def get():
            try:
                return build()
            except Failed as e:
                raise lc.Stop("build", str(e)) from None

        return get

    def checked_solar():
        # The same binary rule as the run's: this checkout's compiler, and the expected digest
        # when the runner names one.
        solar = build_solar()
        if (
            why := solar_problem(
                solar,
                ROOT,
                os.environ.get("CERTIFY_SOLAR_SHA256"),
                os.environ.get("CERTIFY_ALLOW_FOREIGN_SOLAR") == "1",
            )
        ) is not None:
            raise Failed(f"solar: {why}")
        record["loop_cuts_solar_sha256"] = sha256_file(solar)
        return solar

    def lean_dir():
        try:
            _, _, d = clone_base(a, work)
        except Failed as e:
            raise lc.Stop("obligations", f"cannot clone the base: {e}") from None
        rc, body, err = frontend(d, src, src_sha, name, contract, a.solc)
        if rc != 0:
            raise lc.Stop(
                "obligations",
                f"the solc-AST front end refuses the source: {err.strip()[-300:]}",
            )
        # The probe cache seeds the clone's .lake, as lifecycle_build's does; lake rebuilds what
        # differs.
        if a.probe_cache and not (d / ".lake").exists():
            rc, _, err = run(
                [
                    "cp",
                    "-a",
                    str(pathlib.Path(a.probe_cache) / ".lake"),
                    str(d / ".lake"),
                ]
            )
            if rc != 0:
                raise lc.Stop(
                    "obligations", f"cannot copy the probe cache: {err[:200]}"
                )
        (d / "Certified" / name).mkdir(parents=True, exist_ok=True)
        (d / "Certified" / name / "Source.lean").write_text(source_lean(body))
        declare_lean_lib(
            d / "lakefile.lean",
            name,
            f'\nlean_lib «{name}» where\n  srcDir := "Certified"\n  globs := #[.submodules `{name}]\n',
            (lean_lib_body(name),),
        )
        record["loop_cuts_clone"] = str(d)
        return d

    heads, cuts, blocked = lc.hook(
        run=run,
        solar=tool(checked_solar),
        verify=tool(build_solar_verify),
        z3=os.environ.get("SOLAR_VERIFY_Z3"),
        src=src,
        ast=ast,
        contract=contract,
        name=name,
        work=work,
        stages=lambda solar, tmp: export_stages(solar, src, contract, tmp),
        lean_dir=lean_dir,
        lake_env=lambda d: lake_jobs_env(d, dict(os.environ, LEAN_NUM_THREADS="1")),
        log=log,
        summary=loop_summaries(a.loop_summary or []).get,
        shape=refused.get,
        keep=keep_log,
    )
    record["loop_heads"] = heads
    record["loop_cuts"] = cuts
    record["loop_cuts_blocked"] = blocked
    return lc.reasons(cuts, blocked)


def loop_final(record, reasons, src, ast, a, work, name):
    """The loop's published theorem (tools/certify/loop_final.py) in the loop cut hook's clone, once
    the hook built the obligations of every loop function: the stage proofs of this run's export, the
    fixed-fuel module, the join, final.py's statement, lake build NAME.Final and check_final with this
    run's bytes. Records loop_final. The run stays BLOCKED either way: the statement covers only the
    loop function's calls, from any storage (runtime only), with the specification declared vacuous.
    On success the loop function's OPEN reasons become one reason naming the checked theorem; on a
    refusal its reason is added."""
    import fixed_fuel
    import loop_final as lf

    contract = fixed_fuel.contract_of(ast, a.contract)
    solar = build_solar()
    plain, err = solar_runtime(solar, src, contract)
    clone = record.get("loop_cuts_clone")
    if plain is None or clone is None:
        why = (
            "no clone from the loop cut hook"
            if clone is None
            else f"solar: {err.strip()[:200]}"
        )
        record["loop_final"] = {"refused": {"step": "internal", "why": why}}
        return reasons + [lf.reason(lf.Refused("internal", why))]
    out = lf.attach(
        record,
        run=run,
        name=name,
        contract=contract,
        ast=ast,
        solar=solar,
        src=src,
        work=work,
        lean_dir=clone,
        lake_env=lake_jobs_env(clone, dict(os.environ, LEAN_NUM_THREADS="1")),
        plain=plain,
        log=log,
    )
    if out:
        return reasons + out
    fname = record["loop_final"]["function"]
    # The envelope's reason names the function at the start of its detail (its where is a line);
    # the loop cut hook's names it in where.
    kept = [
        r
        for r in reasons
        if not (
            r["feature"] == "loop"
            and (
                r.get("where") == fname
                or r.get("detail", "").startswith(f"{fname}: a counted for loop")
            )
        )
    ]
    # The specification the final states: solar-verify's export when the bridge proved it
    # (loop_final.attach records it), else True.
    fin = (record.get("loop_final") or {}).get("final", {})
    spec_s = (
        "specification: solar-verify's export in the fixed frame TreeLoop.ctx0"
        + (
            ""
            if fin.get("vacuity") is None and fin.get("allowed_vacuity") is None
            else " (not shown non-vacuous: "
            + (fin.get("vacuity") or "the allowed statuses: " + fin["allowed_vacuity"])
            + ")"
        )
        if fin.get("spec_bound_to_export")
        else "specification True (declared vacuous)"
    )
    return kept + [
        {
            "feature": "loop",
            "where": fname,
            "detail": f"loop final checked (Certified.{name}.Final.final, record loop_final): runtime "
            f"only, the loop function's calls only, {spec_s}",
        }
    ]


def loop_summaries(specs):
    """--loop-summary FN=MODULE:CONSTANT, each a GenObl.Summary of FN's body. It is untrusted:
    the generator checks its shape against the run's imported function, and the obligation
    modules prove that check by kernel_rfl. Without one (the default), the generator takes the
    erasure route for FN (eraseLoopStmt of its body)."""
    out = {}
    for spec in specs:
        fn, eq, rest = spec.partition("=")
        mod, colon, const = rest.partition(":")
        if not (fn and eq and colon and mod and const) or ":" in const:
            raise Failed(f"--loop-summary {spec!r}: expected FN=MODULE:CONSTANT")
        out[fn] = (mod, const)
    return out


def clone_base(a, work):
    """The run's clone of the solidity-lean base (--base, else the pin) in WORK/solidity-lean,
    checked out detached with its submodules: (source repository, base commit, clone)."""
    lean_src = pathlib.Path(a.solidity_lean).resolve()
    base = a.base or (HERE / "solidity-lean.rev").read_text().strip()
    base = git(lean_src, "rev-parse", base + "^{commit}")
    lean_dir = work / "solidity-lean"
    if lean_dir.exists():
        raise Failed(f"{lean_dir} exists: the work directory must be fresh")
    work.mkdir(parents=True, exist_ok=True)
    git(work, "clone", "-q", "--no-checkout", "--shared", str(lean_src), str(lean_dir))
    git(lean_dir, "checkout", "-q", "--detach", base)
    git(lean_dir, "submodule", "update", "-q", "--init", "--recursive", check=False)
    return lean_src, base, lean_dir


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
    ("solar", "binary_commit"),
    # The build's inputs as each run observed them (build_provenance). The binary's own digest is
    # recorded and bound within each run (producer_problem) but not compared across runs: a debug
    # build is not byte-reproducible, and identical checked outputs are compared instead.
    ("solar", "build", "provenance_version"),
    ("solar", "build", "commit"),
    ("solar", "build", "tree"),
    ("solar", "build", "submodules"),
    ("solar", "build", "cargo_lock_sha256"),
    ("solar", "build", "rustc_vv"),
    ("solar", "build", "target"),
    ("solar", "build", "profile"),
    ("solar", "build", "features"),
    ("solar", "build", "rustflags"),
    ("solar", "build", "command"),
    ("solar", "build", "target_source"),
    ("solar", "build", "cargo_env"),
    ("solar", "build", "cargo_config"),
    ("solc", "sha256"),
    ("lean_toolchain", "recorded_digest"),
    ("published_statement",),
    ("status",),
    ("strict",),
    ("tier_reached",),
    # The lifecycle tier's provenance-bound inputs: the binding (every key, source_module
    # included), the argument words and the creation bytes they give.
    ("lifecycle", "bind"),
    ("lifecycle", "args"),
    ("lifecycle", "creation_sha256"),
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


def commit_inputs(lean_dir, base, name, contract):
    """Commit what LEAN_DIR's Certified/ and lakefile.lean hold as one commit on BASE, and return
    the commit and the paths it changes (the run's generated inputs). The commit must pass
    checkout.sh's own inputs check, the one scripts/certify applies to the checkout it runs in, so
    the inputs the record lists are exactly the files that run reads: a changed path outside
    Certified/NAME/ or an ignored file under it is refused here too. lakefile.lean may change only
    by lakefile_inputs.py's rule (the canonical stanza for NAME appended to the base's bytes), which
    checkout.sh applies and this re-applies for CONTRACT."""
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
    rc, _, err = run(
        [
            "bash",
            str(HERE / "checkout.sh"),
            "--check-inputs",
            str(lean_dir),
            base,
            inputs,
            f"Certified/{name}",
        ]
    )
    if rc != 0:
        raise Failed(f"the input commit: {err.strip()[-300:]}")
    changed = git(lean_dir, "diff", "--no-renames", "--name-only", base, inputs)
    if "lakefile.lean" in changed.splitlines() and (
        why := lakefile_problem(lean_dir, base, inputs, name, contract)
    ):
        raise Failed(f"the input commit: lakefile.lean: {why}")
    return inputs, changed.splitlines()


def lakefile_problem(repo, base, head, name, contract=None, head_bytes=None):
    """Why lakefile.lean of HEAD in REPO (or HEAD_BYTES) is not lakefile_inputs.py's allowed change
    of BASE's for NAME, or None: the one rule generic.py, checkout.sh, after_run_problem and the
    executed-tree rebuild apply."""
    try:
        old = lakefile_inputs.show(str(repo), base)
        new = (
            head_bytes
            if head_bytes is not None
            else lakefile_inputs.show(str(repo), head)
        )
    except lakefile_inputs.Refused as e:
        return str(e)
    return lakefile_inputs.problem(old, new, name, contract)


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


def after_run_problem(lean_dir, name, inputs, rec, pin):
    """Why the run scripts/certify reports in REC did not execute the input commit INPUTS of
    LEAN_DIR with the pinned packages PIN (pinned_packages' set) unedited, or None: only this
    run's own Certified/NAME/ may differ from the commit after the run, and the commit's and the
    checkout's lakefile.lean must be lakefile_inputs.py's allowed change of the base's."""
    ran = git(lean_dir, "rev-parse", "HEAD")
    outside = changed_outside(lean_dir, f"Certified/{name}/")
    if ran != inputs or rec.get("solidity_lean") != inputs or outside:
        return (
            f"executed tree: scripts/certify ran at {rec.get('solidity_lean')} (checkout {ran}), "
            f"not the input commit {inputs}, or changed files outside Certified/{name}/: {outside[:3]}"
        )
    base = git(lean_dir, "rev-parse", inputs + "^")
    try:
        on_disk = (pathlib.Path(lean_dir) / "lakefile.lean").read_bytes()
    except OSError as e:
        return f"executed tree: cannot read lakefile.lean: {e}"
    for label, head_bytes in (
        ("the input commit's", None),
        ("the checkout's", on_disk),
    ):
        if why := lakefile_problem(
            lean_dir, base, inputs, name, rec.get("contract"), head_bytes
        ):
            return f"executed tree: {label} lakefile.lean: {why}"
    if (
        why := packages_problem(pathlib.Path(lean_dir) / ".lake" / "packages", pin)
    ) is not None:
        return f"packages: after the run: {why}"
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
    name = rec.get("name")
    if not isinstance(name, str):
        return "the record names no run (name), so its generated inputs are unchecked"
    if stray := [
        q
        for q in paths
        if not (
            isinstance(q, str)
            and (q.startswith(f"Certified/{name}/") or q == "lakefile.lean")
        )
    ]:
        return f"the listed generated inputs hold paths outside Certified/{name}/: {stray[:3]}"
    if "lakefile.lean" in paths:
        try:
            lf = (d / "inputs" / "lakefile.lean").read_bytes()
        except OSError:
            return "the listed generated input lakefile.lean is missing from inputs/"
        if why := lakefile_problem(repo, base, None, name, rec.get("contract"), lf):
            return f"the listed generated input lakefile.lean: {why}"
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


def lifecycle_verdict(rec):
    """The lifecycle verdict replay compares between the record and the fresh run: whether a kernel
    stage proved the creation, and the composed theorem's name and whether it built."""
    life = rec.get("lifecycle") or {}
    comp = life.get("composed") or {}
    return {
        "kernel_ok": any(
            st.get("stage") == "kernel" and st.get("ok") is True
            for st in life.get("stages") or []
        ),
        "composed.theorem": comp.get("theorem"),
        "composed.built": comp.get("built"),
    }


def lifecycle_claims_problem(rec):
    """Why a lifecycle-tier record's claims do not hold together, or None. The lifecycle tier runs no
    scripts/certify: it publishes no final statement, statuses or replay of its own, and its
    verdict is the lifecycle stages' (CERTIFIED exactly when the kernel proved the creation, with the
    composed theorem built when the record binds an expected theorem). The fresh run must reproduce
    status, tier_reached and every lifecycle key replay compares."""
    claimed = [k for k in (*CLAIM_KEYS, "strict", "published_statement") if rec.get(k)]
    if claimed:
        return f"the lifecycle tier publishes no {', '.join(claimed)}, but the record claims it"
    if "certify-record.json" in (rec.get("run_files") or {}):
        return "the lifecycle tier runs no scripts/certify, but the record lists a certify-record.json"
    status, reached = rec.get("status"), rec.get("tier_reached")
    life = rec.get("lifecycle") or {}
    if status == "CERTIFIED":
        if reached != "lifecycle":
            return f"a CERTIFIED lifecycle record has tier_reached {reached!r}, not 'lifecycle'"
        if not any(
            st.get("stage") == "kernel" and st.get("ok")
            for st in life.get("stages", [])
        ):
            return "a CERTIFIED lifecycle record has no proved kernel stage"
        if life.get("bind") and not (life.get("composed") or {}).get("built"):
            return "a CERTIFIED lifecycle record that binds an expected theorem has no built composed theorem"
    elif reached == "lifecycle":
        return f"a {status} lifecycle record claims tier_reached 'lifecycle'"
    return None


def claims_problem(rec, d):
    """Why record.json's claims are not certify-record.json's (or its verdict not the one strict
    gives that record), or None. A lifecycle-tier record is checked by lifecycle_claims_problem."""
    if rec.get("requested_tier") == "lifecycle":
        return lifecycle_claims_problem(rec)
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


def producer_problem(rec, d, which):
    # WHICH is "recorded run" or "fresh run".
    """Why the record's solar is not the one its run built and ran, or None: the build provenance is
    present in this version, the binary embeds the commit it was built from, and the executable
    scripts/certify digested (certify-record.json solar_sha256, a hashed run file) is the one the
    record names."""
    s = rec.get("solar") or {}
    b = s.get("build")
    if not isinstance(b, dict) or b.get("provenance_version") != PROVENANCE_VERSION:
        return (
            f"versioned-unsupported: the {which} has no solar build provenance version "
            f"{PROVENANCE_VERSION} (solar.build); nothing is filled in"
        )
    for k in (
        "commit",
        "tree",
        "submodules",
        "cargo_lock_sha256",
        "rustc_vv",
        "target",
        "profile",
        "features",
        "rustflags",
        "command",
        "target_source",
        "cargo_env",
        "cargo_config",
        "artifact",
    ):
        if k not in b:
            return f"the {which}'s solar.build has no {k}"
    if not re.fullmatch(r"[0-9a-f]{64}", str(s.get("binary_sha256"))):
        return f"the {which} has no solar.binary_sha256"
    if b["artifact"].get("sha256") != s["binary_sha256"]:
        return (
            f"substituted executable: the {which}'s build produced {b['artifact'].get('sha256')}, "
            f"not the recorded {s['binary_sha256']}"
        )
    if s.get("dirty") is not False:
        return f"the {which}'s solar checkout is dirty"
    if not (s.get("binary_commit") == b["commit"] == s.get("commit")):
        return (
            f"stale producer: the {which}'s solar embeds {s.get('binary_commit')}, "
            f"not the commit it was built from ({b['commit']})"
        )
    if rec.get("requested_tier") == "lifecycle":
        # The lifecycle tier runs no scripts/certify: the driver itself ran the artifact its build
        # produced, bound above (solar.build.artifact.sha256 == binary_sha256).
        return None
    try:
        cr = json.loads((d / "certify-record.json").read_text())
    except OSError, ValueError:
        return f"the {which} has no readable certify-record.json to bind its executable"
    if cr.get("solar_sha256") != s["binary_sha256"]:
        return (
            f"substituted executable: the {which}'s scripts/certify ran solar "
            f"{cr.get('solar_sha256')}, not the built {s['binary_sha256']}"
        )
    return None


LIFECYCLE_PROVENANCE = ("bind", "args", "creation_sha256")


def lifecycle_files_problem(rec, d):
    """Why a lifecycle record's provenance is not its own listed files, or None: its creation, init
    and runtime bytes and its binding (LIFECYCLE_FILES) must be listed in run_files, and the record's
    creation_sha256, initcode_sha256, bin_runtime_sha256, args and bind must be those files'."""
    if rec.get("requested_tier") != "lifecycle":
        return None
    life, files = rec.get("lifecycle") or {}, rec.get("run_files") or {}
    need = ["init", "creation", "bin_runtime"] + (
        ["bind"] if life.get("bind") is not None else []
    )
    for k in need:
        if LIFECYCLE_FILES[k] not in files:
            return f"lifecycle files: the record lists no {LIFECYCLE_FILES[k]}"
    try:
        init = bytes.fromhex((d / LIFECYCLE_FILES["init"]).read_text().strip())
        creation = bytes.fromhex((d / LIFECYCLE_FILES["creation"]).read_text().strip())
        plain = bytes.fromhex((d / LIFECYCLE_FILES["bin_runtime"]).read_text().strip())
    except (OSError, ValueError) as e:
        return f"lifecycle files: unreadable bytes: {e}"
    if hashlib.sha256(creation).hexdigest() != life.get("creation_sha256"):
        return "lifecycle files: creation_sha256 is not lifecycle/creation.hex's"
    if hashlib.sha256(init).hexdigest() != life.get("initcode_sha256"):
        return "lifecycle files: initcode_sha256 is not lifecycle/init.hex's"
    if hashlib.sha256(plain).hexdigest() != life.get("bin_runtime_sha256"):
        return "lifecycle files: bin_runtime_sha256 is not lifecycle/bin-runtime.hex's"
    try:
        words = b"".join(int(w, 16).to_bytes(32, "big") for w in life.get("args") or [])
    except (TypeError, ValueError, OverflowError):  # fmt: skip
        return "lifecycle files: args are not 32-byte words"
    if creation != init + words:
        return "lifecycle files: args are not the words lifecycle/creation.hex appends to lifecycle/init.hex"
    if life.get("bind") is not None:
        try:
            listed = json.loads((d / LIFECYCLE_FILES["bind"]).read_text())
        except (OSError, ValueError) as e:
            return f"lifecycle files: unreadable lifecycle/bind.json: {e}"
        if listed != life["bind"]:
            return "lifecycle files: lifecycle.bind is not lifecycle/bind.json"
    return None


def lifecycle_provenance_problem(rec):
    """Why a lifecycle-tier record cannot be replayed, or None: it must carry the lifecycle tier's
    provenance-bound inputs (LIFECYCLE_PROVENANCE), which the fresh run is given and compared on; a
    record without them is refused, never filled in. Other tiers are not affected."""
    if rec.get("requested_tier") != "lifecycle":
        return None
    life = rec.get("lifecycle") or {}
    missing = [k for k in LIFECYCLE_PROVENANCE if life.get(k) is None]
    if missing:
        return (
            f"versioned-unsupported: the recorded run has no lifecycle provenance "
            f"({', '.join(missing)}); nothing is filled in"
        )
    return None


def replay_main(a):
    """--replay DIR: refuse a finished run's directory unless its files are the record's and a
    fresh, cold run from the same pinned source/build inputs gives identical checked outputs."""

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
    if (why := lifecycle_provenance_problem(rec)) is not None:
        out(1, f"REJECTED: replay: {why}")
    if (why := lifecycle_files_problem(rec, d)) is not None:
        out(1, f"REJECTED: replay: {why}")
    if (why := claims_problem(rec, d)) is not None:
        out(1, f"REJECTED: replay: {why}")
    if (why := producer_problem(rec, d, "recorded run")) is not None:
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
    life = rec.get("lifecycle") or {}
    if rec.get("requested_tier") == "lifecycle" and life.get("args") is not None:
        cmd += ["--constructor-args", ",".join(str(int(w, 16)) for w in life["args"])]
        if life.get("bind"):
            bind_file = fresh_dir.parent / f"{fresh_dir.name}.bind.json"
            bind_file.write_text(json.dumps(life["bind"]))
            cmd += ["--lifecycle-bind", str(bind_file)]
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
    if (
        p.returncode == 0
        and (why := producer_problem(new, fresh_dir, "fresh run")) is not None
    ):
        out(1, f"REJECTED: replay: {why}")
    for path in REPLAY_KEYS:
        if dig(rec, path) != dig(new, path):
            shown = [str(dig(r, path))[:120].replace("\n", "\\n") for r in (rec, new)]
            out(
                1,
                f"REJECTED: replay: {'.'.join(path)} differs between the record and the fresh run "
                f"(record {shown[0]}, fresh {shown[1]})",
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
    if rec.get("requested_tier") == "lifecycle":
        want, got = lifecycle_verdict(rec), lifecycle_verdict(new)
        for k in want:
            if want[k] != got[k]:
                out(
                    1,
                    f"REJECTED: replay: lifecycle.{k} differs between the record and the fresh run "
                    f"(record {want[k]}, fresh {got[k]})",
                )
    if replayed_modules(rec) != replayed_modules(new):
        out(
            1,
            "REJECTED: replay: replay.replayed (its modules) differs between the record and the fresh run",
        )
    last = new.get("status", "")
    out(
        p.returncode,
        f"{last}: replay of {d}: a fresh cold run from the same pinned source/build inputs gave identical checked outputs ({fresh_dir / 'record.json'})"
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
    ap.add_argument(
        "--lifecycle-bind",
        help="--tier lifecycle: a JSON file naming the expected theorem whose (c, fuel), init code, "
        "runtime and words the certificate states (lifecycle_binding); the run then also checks its "
        "source unit is the theorem's and builds the composed theorem",
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
        "--refute-in-kernel",
        action="store_true",
        help="a decided NatSpec refutation (natspec_checks) does not stop the run: the run goes on to "
        "spec_connect.py, whose kernel-checked refutation names the declaration it refutes; the run is "
        "REJECTED either way (the negative controls, tools/certify/stage_neg.sh)",
    )
    ap.add_argument(
        "--loop-summary",
        action="append",
        metavar="FN=MODULE:CONSTANT",
        help="a GenObl.Summary of loop function FN's body for the loop cut hook (checked by the "
        "generator against the imported function); without one the generator erases FN's body "
        "itself (eraseLoopStmt)",
    )
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
    admit_lake_jobs()

    src = pathlib.Path(a.source).resolve()
    rec_dir = pathlib.Path(a.record_dir).resolve()
    rec_dir.mkdir(parents=True, exist_ok=True)
    global BUILD_LOG_DIR
    BUILD_LOG_DIR = rec_dir
    work = pathlib.Path(a.work).resolve()
    if a.spec not in ("natspec", "none"):
        raise SystemExit("certify: SPEC must be natspec or none")
    tier = a.tier or ("certified" if a.spec == "natspec" else "compiled")
    if a.constructor_args is not None and tier != "lifecycle":
        ap.error("--constructor-args needs --tier lifecycle")
    text = src.read_text()
    src_sha = hashlib.sha256(text.encode()).hexdigest()
    name = run_name(src)
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
        seal_record(rec_dir, record, status, **extra)
        log(line)
        sys.exit(code)

    try:
        log(f"== generic: {src} ({src_sha}) as {name}, requested tier {tier}")
        ast, contracts = solc_ast(a.solc, src)
        lifecycle_open = []
        try:
            contract, fns, events = envelope(
                text,
                ast,
                contracts,
                a.contract,
                a.invariants,
                a.spec == "natspec",
                tier == "lifecycle",
                lifecycle_open,
            )
        except Blocked as b:
            if any(r["feature"] == "loop" for r in b.reasons):
                # The cut points first, so the record holds them whatever loop_resources decides;
                # its REJECTED still decides the verdict, and BLOCKED stays the floor otherwise.
                extra = loop_cuts(record, src, ast, a, work, name, src_sha)
                __import__("loop_any_caller").from_driver(
                    record,
                    src,
                    ast,
                    a.contract,
                    work,
                    build_solar,
                    build_solar_verify,
                    log,
                )
                # The unbounded fixed-fuel obligation's out of gas stays the verdict, but only
                # after the loop's final theorem, which bounds the loop's limit by K and carries
                # its own gas bound, is built and recorded beside it.
                out_of_gas = None
                try:
                    loop_resources(record, src, ast, a.contract)
                except Failed as e:
                    if record.get("rejected_for") != "resource: out of gas":
                        raise
                    out_of_gas = e
                reasons = b.reasons + extra
                if record.get("loop_cuts") and not record.get("loop_cuts_blocked"):
                    reasons = loop_final(record, reasons, src, ast, a, work, name)
                if out_of_gas is not None:
                    raise out_of_gas
                raise Blocked(reasons) from b
            raise
        if lifecycle_open:
            record["lifecycle_open"] = lifecycle_open
            record["resources"] = (
                "OPEN: the fixed-fuel resource obligation of the runtime loop(s) is not required at "
                "the lifecycle tier and is not discharged"
            )
            log(
                f"lifecycle: runtime loops OPEN (not this tier's statement): {[o['where'] for o in lifecycle_open]}"
            )
        if a.spec == "natspec":
            refuted, invs = natspec_checks(ast, contract)
            if invs:
                record["invariants"] = invs
            if refuted and not a.refute_in_kernel:
                raise Failed(refuted)
            if refuted:
                # Recorded, never dropped: a run that reaches no kernel refutation is REJECTED with it.
                record["natspec_refuted"] = refuted
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
            witness = spec_witness(fns)
        log("== generic: build solar")
        solar = build_solar()
        # The binary must be this checkout's compiler, and the expected one when the runner names
        # its digest (CERTIFY_SOLAR_SHA256), before it compiles anything the run records.
        if (
            why := solar_problem(
                solar,
                ROOT,
                os.environ.get("CERTIFY_SOLAR_SHA256"),
                os.environ.get("CERTIFY_ALLOW_FOREIGN_SOLAR") == "1",
            )
        ) is not None:
            raise Failed(f"solar: {why}")
        if why := artifact_problem(solar):
            raise Failed(f"solar: {why}")
        record["solar"]["binary_sha256"] = sha256_file(solar)
        rc, vout, _ = run([str(solar), "--version"])
        record["solar"]["version"] = vout.strip().splitlines()[0] if vout else ""
        # The commit the binary embeds (its build's), recorded apart from the driver's checkout.
        m = re.search(r"Commit SHA: ([0-9a-f]{40})", vout or "")
        record["solar"]["binary_commit"] = m.group(1) if m else None
        record["solar"]["build"] = build_provenance(ROOT, solar, vout)
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
                        "feature": "unlinked library",
                        "where": f"byte {pc}",
                        "detail": "the runtime bytecode has an unlinked library placeholder",
                    }
                    if op == "unlinked library"
                    else {
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
        lean_src, base, lean_dir = clone_base(a, work)
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
        succ = None
        if witness == SYNTH_WITNESS:
            succ = synth_witness(res, fns)
            witness = succ["function"]
            record["succeeds_witness"] = succ
            log(f"succeeds witness (no succeeds-if): {witness} on the zero call")
        if caller == "any":
            # Only on the any route: the fixed route's record is the fixed-frame record.
            record["caller"] = caller
            record["environment"] = ENVIRONMENT_ANY
        if fresh_certified(lean_dir, name, contract):
            # The base already holds this source's record: regenerated here in full, never reused.
            record["base_held_record"] = True
            log(
                f"base holds Certified/{name}: removed in this run's clone and regenerated in full"
            )
        d.mkdir(parents=True)
        shutil.copy(src, d / f"{name}.sol")
        (d / "options").write_text("\n".join(OPTIONS) + "\n")
        (d / "Source.lean").write_text(source_text)
        # The C3 order of a contract with bases, from the AST the envelope read (above).
        lin_text = linearization_module(name, contract, ast, src_sha)
        if lin_text:
            (d / "Linearization.lean").write_text(lin_text)
            log(
                "inheritance: Linearization.lean states that the frontend's C3 order is solc's"
            )
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
                    name,
                    contract,
                    fns,
                    need,
                    spec_desc,
                    witness,
                    events,
                    any_gas,
                    succeeds_witness=succ,
                ),
                indent=2,
            )
            + "\n"
        )
        inputs, changed = commit_inputs(lean_dir, base, name, contract)
        record["solidity_lean_source"] = str(lean_src)
        record["solidity_lean"] = {
            "base": base,
            "inputs_commit": inputs,
            "inputs_tree": git(lean_dir, "rev-parse", f"{inputs}^{{tree}}"),
            "provenance": f"solidity-lean {base} + inputs {inputs} (tree "
            f"{git(lean_dir, 'rev-parse', inputs + '^{tree}')}, Certified/{name} only)",
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
        # The clone is the pin plus one inputs commit under Certified/<name>/: CERTIFY_PIN_BASE has
        # checkout.sh check exactly that, never the CERTIFY_ALLOW_UNPINNED hatch.
        # pin_rev is the pinned commit; pin stays the pinned package set the post-run check needs.
        pin_rev = (HERE / "solidity-lean.rev").read_text().strip()
        if base != pin_rev:
            raise Failed(
                f"the base {base} is not the pin {pin_rev} of tools/certify/solidity-lean.rev"
            )
        env = dict(
            os.environ,
            SOLIDITY_LEAN_DIR=str(lean_dir),
            CERTIFY_PIN_BASE=pin_rev,
            CERTIFY_CLEAN_BUILD="1",
            SOLC=a.solc,
            CERTIFY_SOLAR_SHA256=record["solar"]["binary_sha256"],
            LEAN_MEMORY=LEAN_MEMORY,
        )
        for k in (
            "CERTIFY_ALLOW_UNPINNED",
            "CERTIFY_REPLAY_TRUST_UPSTREAM",
            "SOLAR",
            "CERTIFY_SKIP_BUILD",
            "CERTIFY_ALLOW_FOREIGN_SOLAR",
            "CERTIFY_EXAMPLES",
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
                    "CERTIFY_PIN_BASE",
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
            if spec_rej and not hit and record.get("natspec_refuted"):
                record["natspec_refuted_in_kernel"] = spec_rej[0][:600]
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
        if record.get("natspec_refuted"):
            finish(
                "REJECTED",
                1,
                f"REJECTED: {record['natspec_refuted']} (decided; the kernel refuted no declaration)",
            )
        rec = json.loads(inner.read_text())
        shutil.copy(inner, rec_dir / "certify-record.json")
        if (why := after_run_problem(lean_dir, name, inputs, rec, pin)) is not None:
            finish("REJECTED", 1, f"REJECTED: {why}")
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
    except asm_cert.Unbounded as e:
        # A stack-bound refusal no call site converted (loop_checker_gas and the loop resources
        # do): still BLOCKED with its reason, never an exit without a record.
        b = Blocked(
            [
                {
                    "feature": "stack bound",
                    "where": "",
                    "detail": str(e.code).removeprefix("error: "),
                }
            ]
        )
        if record.get("natspec_refuted"):
            finish(
                "REJECTED",
                1,
                f"REJECTED: {record['natspec_refuted']} (decided; then BLOCKED: {b})",
                blocked=b.reasons,
            )
        finish("BLOCKED", 3, f"BLOCKED: {b}", blocked=b.reasons)
    except Blocked as b:
        if record.get("natspec_refuted"):
            finish(
                "REJECTED",
                1,
                f"REJECTED: {record['natspec_refuted']} (decided; then BLOCKED: {b})",
                blocked=b.reasons,
            )
        finish("BLOCKED", 3, f"BLOCKED: {b}", blocked=b.reasons)
    except Failed as f:
        if record.get("natspec_refuted") and "refuted in the kernel" not in str(f):
            finish(
                "REJECTED",
                1,
                f"REJECTED: {record['natspec_refuted']} (decided; then: {f})",
            )
        finish("REJECTED", 1, f"REJECTED: {f}")
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as e:
        if record.get("natspec_refuted"):
            finish(
                "REJECTED",
                1,
                f"REJECTED: {record['natspec_refuted']} (decided; then the driver failed: "
                f"{type(e).__name__}: {e})",
            )
        finish("REJECTED", 1, f"REJECTED: driver: {type(e).__name__}: {e}")


if __name__ == "__main__":
    main()
