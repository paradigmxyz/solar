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
  3  BLOCKED: <reason>: the source is outside the supported envelope; nothing was proved or claimed
  1  ERROR: <message>: the driver or a step failed

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

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(HERE))
import keccak  # noqa: E402

SOLC_VERSION = "0.8.35"
ENVIRONMENT = "one frame, caller 0x2000, contract at 0x1000, the block and transaction of Target.message (fixed, not quantified)"
OPTIONS = ["-Ogas", "--evm-version", "osaka"]
# Opcodes outside the envelope in the runtime bytecode: calls, creation, self-destruct, gas and
# return data reads. The loop checker's fragment rejects them too; this check only names them early.
BAD_OPS = {0xF0: "CREATE", 0xF1: "CALL", 0xF2: "CALLCODE", 0xF4: "DELEGATECALL", 0xF5: "CREATE2",
           0xFA: "STATICCALL", 0xFF: "SELFDESTRUCT", 0x5A: "GAS", 0x3D: "RETURNDATASIZE",
           0x3E: "RETURNDATACOPY", 0x31: "BALANCE", 0x47: "SELFBALANCE", 0x3B: "EXTCODESIZE",
           0x3C: "EXTCODECOPY", 0x3F: "EXTCODEHASH"}


class Blocked(Exception):
    def __init__(self, reasons):
        order = ["external call", "contract creation", "inline assembly", "value transfer", "loop", "bytecode"]
        groups = {}
        for r in reasons:
            groups.setdefault(r["feature"], []).append(r)
        feats = sorted(groups, key=lambda f: order.index(f) if f in order else len(order))
        super().__init__("; ".join(
            f"{f} ({len(groups[f])}x, first {(groups[f][0]['where'] + ': ') if groups[f][0]['where'] else ''}{groups[f][0]['detail']})"
            for f in feats))
        self.reasons = reasons


class Failed(Exception):
    pass


def sha256_file(p):
    return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()


def run(cmd, **kw):
    r = subprocess.run(cmd, capture_output=True, text=True, **kw)
    return r.returncode, r.stdout, r.stderr


def log(msg):
    print(msg, flush=True)


# --- the envelope, from the solc AST ---------------------------------------------------------

def solc_ast(solc, src):
    # The import closure is read from the source's directory for the analysis only (so that every
    # reason a source is outside the envelope is named); a source with imports is never certified.
    inp = {"language": "Solidity", "sources": {src.name: {"urls": [src.name]}},
           "settings": {"outputSelection": {"*": {"": ["ast"], "*": ["abi"]}}}}
    rc, out, err = run([solc, "--standard-json", "--base-path", str(src.parent), "--allow-paths", str(src.parent)],
                       input=json.dumps(inp))
    if rc != 0:
        raise Failed(f"solc --standard-json exits {rc}: {err.strip()[:400]}")
    res = json.loads(out)
    errs = [e for e in res.get("errors", []) if e.get("severity") == "error"]
    if errs:
        msg = errs[0].get("formattedMessage", errs[0].get("message", ""))
        if "not found" in msg and "import" in msg.lower():
            raise Blocked([{"feature": "imports", "where": "", "detail": "the source imports other files; the driver takes one self-contained file and never flattens (a frozen import closure is not supported yet)"}])
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
        n = enums.get(it[len("enum "):])
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


def envelope(text, ast, contracts, contract, invariants, natspec=False):
    reasons = []

    def block(feature, node, detail):
        reasons.append({"feature": feature, "where": line_of(text, node) if node else "", "detail": detail})

    for n in ast.get("nodes", []):
        if n.get("nodeType") == "ImportDirective":
            block("imports", n, "the source imports other files; the driver takes one self-contained file and never flattens (a frozen import closure is not supported yet)")
    defs = [n for n in ast.get("nodes", []) if n.get("nodeType") == "ContractDefinition"]
    for d in defs:
        if d.get("contractKind") != "contract" or d.get("abstract"):
            block("several contracts", d, f"{d.get('contractKind')} {d.get('name')}: only one concrete contract per file")
        if d.get("baseContracts"):
            block("inheritance", d, f"{d.get('name')} inherits; only one contract with no bases")
    concrete = [d for d in defs if d.get("contractKind") == "contract" and not d.get("abstract")]
    if contract is None:
        if len(concrete) != 1:
            block("several contracts", None, f"{len(concrete)} concrete contracts; name one with --contract")
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
            block("inline assembly", n, "assembly blocks have no SolidCore source semantics here")
        elif t == "TryStatement":
            block("external call", n, "try/catch makes an external call")
        elif t == "NewExpression":
            block("contract creation", n, "new creates a contract (CREATE)")
        elif t in ("ForStatement", "WhileStatement", "DoWhileStatement"):
            block("loop", n, "loops need a kernel-checked invariant certificate; the generic source stage at this base is loop-free"
                  + (", and the supplied invariants file is not consumed yet" if invariants else " (no --invariants given)"))
        elif t == "FunctionDefinition":
            if n.get("stateMutability") == "payable":
                block("value transfer", n, f"{n.get('kind')} {n.get('name') or ''} is payable")
            if n.get("kind") == "constructor" and n.get("parameters", {}).get("parameters"):
                block("constructor parameters", n, "the generic stage runs the constructor with no arguments (Wide.DeployedAt), so its storage is unknown")
            if n.get("kind") in ("receive", "fallback"):
                block("fallback", n, f"{n.get('kind')} function: calls outside the ABI's functions")
        elif t == "MemberAccess" and n.get("memberName") in ("call", "delegatecall", "staticcall", "transfer", "send", "code", "codehash", "balance"):
            block("external call" if n["memberName"] not in ("code", "codehash", "balance") else "account read", n,
                  f".{n['memberName']} on an address")
        elif t == "FunctionCall":
            ty = n.get("expression", {}).get("typeDescriptions", {}).get("typeString", "")
            ref = n.get("expression", {}).get("referencedDeclaration")
            if " external" in ty and (ref is None or ref not in own or n.get("expression", {}).get("nodeType") == "MemberAccess"):
                block("external call", n, f"a call of an external function ({ty[:80]})")
        elif t == "Identifier" and n.get("name") in ("selfdestruct", "gasleft", "blockhash"):
            block("environment", n, f"{n['name']} is outside the source semantics' environment")
    if natspec:
        # The published statement fixes the environment (one frame, Target.message's block and
        # transaction): a spec clause that reads the environment could be false there and make the
        # function's spec hold vacuously, so it is outside the envelope.
        env_re = re.compile(r"\b(block\.\w+|tx\.\w+|gasleft|blockhash|chainid|basefee|coinbase|prevrandao)\b")
        for n in walk(cdef):
            if n.get("nodeType") in ("ContractDefinition", "FunctionDefinition", "VariableDeclaration"):
                doc = (n.get("documentation") or {})
                doc = doc.get("text", "") if isinstance(doc, dict) else str(doc)
                for line in doc.splitlines():
                    if "@custom:" in line and env_re.search(line):
                        block("environment", n, f"a spec clause reads the environment ({env_re.search(line).group(1)}), which the statement fixes: {line.strip()[:120]}")
    abi = contracts[contract]["abi"]
    fns = []
    for e in abi:
        if e["type"] != "function":
            continue
        sig = f"{e['name']}({','.join(p['type'] for p in e['inputs'])})"
        params = [param_of(p, enums) for p in e["inputs"]]
        outs = [param_of(p, enums) for p in e.get("outputs", [])]
        if None in params or None in outs:
            block("ABI types", None, f"{sig}: only uintN, intN, bool, address and enum parameters and results")
        fdef = next((n for n in walk(cdef) if n.get("nodeType") == "FunctionDefinition" and n.get("name") == e["name"]
                     and n.get("kind") == "function"), None)
        doc = ((fdef or {}).get("documentation") or {}).get("text", "") if fdef else ""
        fns.append({"name": e["name"], "sig": sig, "params": params,
                    "mutates": e.get("stateMutability") not in ("view", "pure"),
                    "specified": "@custom:ensures" in doc or "@custom:requires" in doc,
                    "succeeds": "@custom:succeeds-if" in doc or "@custom:succeeds_if" in doc,
                    "sel": int.from_bytes(keccak.keccak256(sig.encode())[:4], "big")})
    events = []
    for e in abi:
        if e["type"] == "event":
            sig = f"{e['name']}({','.join(p['type'] for p in e['inputs'])})"
            events.append({"sig": sig, "topic": int.from_bytes(keccak.keccak256(sig.encode()), "big")})
    if not fns:
        block("no functions", None, "the contract has no external functions")
    if reasons:
        raise Blocked(reasons)
    return contract, fns, events


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


def lowering_lean(name, contract, fns, events, core_lit, storage_lit):
    ns = f"Certified.{name}.Lowering"
    sels = "\n".join(f"/-- The selector of §{f['sig']}§. -/\nabbrev sel{i} : Nat := {hex(f['sel'])}\n" for i, f in enumerate(fns))
    fnlist = ", ".join(f"⟨sel{i}, [{', '.join(f['params'])}]⟩" for i, f in enumerate(fns))
    kr = []
    for i, f in enumerate(fns):
        kr.append(f"def selSig{i} : SolidCore.Solidity.Shared.Word :=\n  SolidCore.Solidity.Source.ABI.selectorFromSignature {lean_str(f['sig'])}\n\n"
                  f"kernel_rfl selSig{i}_eq selSig{i} sel{i}\n")
    for i, e in enumerate(events):
        kr.append(f"def topicSig{i} : Nat := SolidCore.Solidity.Source.Keccak.digestWord {lean_str(e['sig'])}\n\n"
                  f"kernel_rfl topicSig{i}_eq topicSig{i} {hex(e['topic'])}\n")
    table = "".join(f"  if s = {lean_str(f['sig'])} then sel{i}\n  else" for i, f in enumerate(fns))
    splits = nested_splits([f"selSig{i}_eq" for i in range(len(fns))])
    using = "selector_eq"
    digest = ""
    if events:
        dt = "".join(f"  if s = {lean_str(e['sig'])} then {hex(e['topic'])}\n  else" for e in events)
        dsplits = nested_splits([f"topicSig{i}_eq" for i in range(len(events))])
        digest = (f"/-- §Keccak.digestWord§, with the events' signatures by table. -/\n"
                  f"def digestTable (s : String) : Nat :=\n{dt} SolidCore.Solidity.Source.Keccak.digestWord s\n\n"
                  f"theorem digest_eq : SolidCore.Solidity.Source.Keccak.digestWord = digestTable := by\n"
                  f"  funext s\n  unfold digestTable\n{dsplits}\n")
        using += " digest_eq"
    return f"""import SolidCore.Refinement.Generic.Value
import SolidCore.Refinement.KernelCopies
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
/-- The external functions of §{contract}§, from its ABI. -/
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
        f'    | some fn => match Generic.eraseFn (Generic.envOf core) fuel fn, '
        f'Generic.symTree SolarTV.Export.{contract}.stage0 {f["sel"]} {len(f["params"])} mirFuel .all with\n'
        f'      | none, _ => "source tree undefined (erasure)"\n'
        f'      | _, none => "MIR symbolic run undefined"\n'
        f'      | some a, some b => if a.canon.beq b.canon then "agree" else ("trees differ: source " ++ (reprStr a.canon).replace "\\n" " " ++ " MIR " ++ (reprStr b.canon).replace "\\n" " ")))'
        for f in fns)
    return f"""import SolidCore.Refinement.Generic.Value
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
        rc, _, err = run(["cp", "-a", str(pathlib.Path(cache) / ".lake"), str(pl / ".lake")])
        if rc != 0:
            raise Failed(f"cannot copy the probe cache: {err[:200]}")
    d = pl / "Certified" / name
    d.mkdir(parents=True)
    shutil.copytree(work / "probe" / "gen", d / "gen")
    (d / "Source.lean").write_text(source_text)
    (d / "Probe.lean").write_text(probe_lean(name, contract, fns))
    with open(pl / "lakefile.lean", "a") as f:
        f.write(lakefile_lib(name, contract))
    env = dict(os.environ, LEAN_NUM_THREADS=os.environ.get("LEAN_NUM_THREADS", "2"))
    rc, out, err = run(["lake", "build", f"{name}.Source", f"{name}.gen.{contract}.Stage0",
                        "SolidCore.Refinement.Generic.Value", "SolidCore.Refinement.KernelCopies", "SolidCore.Refinement.KernelRfl"],
                       cwd=pl, env=env)
    if rc != 0:
        raise Failed("the probe's build fails: " + " | ".join(l for l in (out + err).splitlines() if l.startswith("error"))[:400])
    rc, out, err = run(["lake", "env", "lean", str(d / "Probe.lean")], cwd=pl, env=env)
    lines = [l for l in out.splitlines() if l.startswith("PROBE ")]
    res = {}
    for l in lines:
        what, _, verdict = l[len("PROBE "):].partition(": ")
        res[what] = verdict
    if rc != 0 or "typecheck" not in res:
        raise Failed("the probe fails: " + " | ".join(l for l in (out + err).splitlines() if "error" in l)[:400])
    reasons = []
    if res["typecheck"] != "ok":
        reasons.append({"feature": "source type check", "where": "", "detail": "SolidCore's type checker rejects the source"})
    for what, verdict in res.items():
        if what.startswith("fn ") and verdict != "agree":
            reasons.append({"feature": "generic source check", "where": what[3:],
                            "detail": f"{what[3:]}: {verdict[:600]}"})
        if what == "constructor" and not verdict.startswith("returns"):
            reasons.append({"feature": "constructor", "where": "", "detail": verdict})
        if what == "constructor" and "with logs" in verdict:
            reasons.append({"feature": "constructor", "where": "", "detail": "the constructor emits logs; the generic stage needs none"})
    missing = [f["sig"] for f in fns if f"fn {f['sig']}" not in res]
    if not reasons and (missing or "core" not in res or "storage" not in res):
        raise Failed(f"the probe printed no verdict for {missing or 'the core or storage'}")
    shutil.rmtree(pl, ignore_errors=True)
    summary = [f"PROBE {k}: {v if k != 'core' else '<' + str(len(v)) + ' chars>'}" for k, v in res.items()]
    return res, summary, reasons


def source_lean(body):
    return ("-- Generated by solar's tools/certify/generic.py with scripts/solc_ast_to_lean_source.py; do not edit.\n"
            "\nimport SolidCore.Solidity.Checked\n\nopen SolidCore.Solidity\n\n" + body)


def shape_json(name, contract, fns, gas, spec_desc, spec_witness=None, events=()):
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
        "generic": {"lowering": f"{name}.Lowering", "ns": ns, "style": "core", "check": "check_stage0",
                    "check_value": "checkValue_stage0", "fns": f"{ns}.fns", "storage": f"{ns}.storageLit"},
        "excluded": [
            "deployment: the init code is not covered; the statement is about the runtime bytecode from any storage that holds the slots the constructor writes, as SolidCore's constructor run with no arguments leaves them",
            "source model trusted: SolidCore's checked contract (the solc AST through scripts/solc_ast_to_lean_source.py and the type checker) is the meaning of the source (frontend trust boundary)",
            "calls: one frame, caller 0x2000, contract at 0x1000, the block and transaction of Target.message; no external calls, GAS reads or return data in the code (the loop checker's fragment)",
            "gas: the bound is the loop checker's (22100 per item times the final EVM IR's block fuel, plus memory), coarse; below it the bytecode may fail without effects, which equivalence admits",
            f"valid calls: {sigs} ABI-encoded with clean argument words, any value below 2^256 (no function is payable: the value reverts on both sides), any storage covering the constructor's slots; other call data is outside the statement",
        ],
        "source_ast": {"file": f"Certified/{name}/Source.lean", "namespace": f"Certified.{name}.Src", "contract": contract},
    }
    if spec_witness is not None:
        # The specification is solar-verify's export of this run's source, proved of SolidCore's
        # contract by tools/certify/spec_connect.py and composed with the bytecode by shape_gen.py:
        # the statement's postcondition and allowed statuses are read off that export, and its
        # non-vacuity witness is a call of SPEC_WITNESS the spec requires to succeed.
        shape.update({
            "kind": "certified",
            "allowed_export": True,
            "spec_export": {"ctx": "SolidCore.Refinement.Shapes.ctxOfCall", "step": "SolidCore.Refinement.Shapes.idStep"},
            "nonvacuous": f"Certified.{name}.Shape.nonvacuous",
        })
        shape["spec_modules"] += ["Verify.Spec.TreeMap", "Verify.ShapeSpecExport"]
        shape["generic"]["spec_connect"] = {"witness": spec_witness, "ghost_fixed": True,
                                            "using": [f"{ns}.digest_eq"] if events else []}
        shape["excluded"].append("spec: solar-verify's export of the source's NatSpec, with its succeeds-if conditions; events are covered by equivalence")
    return shape


# --- the run ----------------------------------------------------------------------------------

def git(d, *a, check=True):
    rc, out, err = run(["git", "-C", str(d), *a])
    if check and rc != 0:
        raise Failed(f"git {' '.join(a)} in {d}: {err.strip()[:300]}")
    return out.strip()


def build_solar():
    env = dict(os.environ, CARGO_BUILD_JOBS=os.environ.get("CARGO_BUILD_JOBS", "2"))
    rc, out, err = run(["cargo", "build", "--locked", "--bin", "solar"], cwd=ROOT, env=env)
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
        raise Blocked([{"feature": "exporter", "where": "", "detail": "the stage exporter rejects the contract (outside the fragment the stages have Lean semantics for): " + err.strip()[:300]}])
    rc, out, err = run([sys.executable, str(HERE / "certify.py"), str(gen / contract), "--gen-module", f"{name}.gen.{contract}",
                        "--proof-dir", str(tmp / "Proofs"), "--proof-module", f"{name}.Proofs",
                        "--namespace", "SolarTV.Milestone.X", "--label", name, "--loops", "0", "--heartbeats", "2000000"])
    if rc != 0:
        raise Failed(f"certify.py cannot write the stage proofs: {err.strip()[-400:]}")
    stages = shape_gen.last_stage((tmp / "Proofs" / "Stages.lean").read_text())
    last_module = "Evm" + stages["last"][3:]
    last_text = (gen / contract / f"{last_module}.lean").read_text()
    outside = asm_cert.outside(last_text)
    if outside:
        raise Blocked([{"feature": "assembly fragment", "where": "", "detail": f"the final EVM IR is outside the loop checker's fragment: {outside}"}])
    inferred = asm_cert.infer(last_text, bytes.fromhex(plain))
    bmax = max(inferred["costs"]) if inferred["costs"] else 0
    words = inferred["top"] // 32
    return 22100 * (stages["fuel"] * bmax) + 3 * words + words * words // 512


def source_bound(record):
    """A positive binding verdict only: source_bind.sh's line for this run's own generated source
    AST, its contract and source file, under the pinned solc. An empty, unknown or other value is
    not a binding."""
    name, contract = record.get("name"), record.get("contract")
    v = (record.get("statuses") or {}).get("source")
    if not (isinstance(v, str) and isinstance(name, str) and isinstance(contract, str)):
        return False
    want = (f"Certified/{name}/Source.lean is the front end's AST of {contract} in "
            f"Certified/{name}/{name}.sol (solc {SOLC_VERSION})")
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
        "replay_full": (isinstance(rp.get("closure"), int) and rp.get("closure", 0) > 0
                        and rp.get("replayed_count") == rp.get("closure") and rp.get("trusted_count") == 0
                        and rp.get("unaccounted") == []),
        "strongest": st.get("strongest") is True,
    }
    need = ["published_statement", "equivalence", "termination", "resources", "source_bound",
            "provenance_clean", "replay_full"]
    if tier in ("functional", "certified"):
        need.append("spec")
    if tier == "certified":
        need.append("strongest")
    return all(c[k] for k in need), {k: c[k] for k in need}, c


REPLAY_KEYS = [("source", "sha256"), ("spec",), ("options",), ("functions",), ("environment",), ("bytecode_sha256",),
               ("gas_bound",), ("requested_tier",), ("solidity_lean", "base"), ("solidity_lean", "inputs_sha256"),
               ("solar", "commit"), ("solar", "binary_sha256"), ("solc", "sha256"), ("lean_toolchain", "recorded_digest"),
               ("published_statement",), ("status",), ("strict",), ("tier_reached",)]


def dig(d, path):
    for k in path:
        d = d.get(k) if isinstance(d, dict) else None
    return d


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
            out(1, f"REJECTED: replay: {rel} differs from the record (stale or edited run file)")
    # Every file and link of the directory, anywhere under it, is the record or in its manifest.
    on_disk = {str(q.relative_to(d)) for q in d.rglob("*") if q.is_file() or q.is_symlink()}
    extra = sorted(on_disk - set(files) - {"record.json"})
    if extra:
        out(1, f"REJECTED: replay: files the record's manifest does not list: {extra[:3]}")
    srcs = [r for r in files if r.endswith(".sol") and "/" not in r]
    if len(srcs) != 1 or files[srcs[0]] != dig(rec, ("source", "sha256")):
        out(1, "REJECTED: replay: the record's source digest is not its source file's")
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    fresh_dir = d.parent / f"{d.name}.replay-{stamp}"
    work = pathlib.Path(a.work).resolve() if a.work else d.parent / f"{d.name}-replay-work-{stamp}"
    cmd = [sys.executable, str(HERE / "generic.py"), str(d / srcs[0]), dig(rec, ("spec", "kind")) or "none",
           "--tier", rec.get("requested_tier", "certified"),
           "--solidity-lean", a.solidity_lean or rec.get("solidity_lean_source", ""),
           "--base", dig(rec, ("solidity_lean", "base")) or "", "--work", str(work), "--record-dir", str(fresh_dir)]
    if rec.get("contract"):
        cmd += ["--contract", rec["contract"]]
    for flag, val in (("--packages-from", a.packages_from), ("--probe-cache", a.probe_cache)):
        if val:
            cmd += [flag, val]
    print(f"== replay: {' '.join(cmd)}", flush=True)
    p = subprocess.run(cmd)
    try:
        new = json.loads((fresh_dir / "record.json").read_text())
    except (OSError, ValueError):
        out(1, f"REJECTED: replay: the fresh run wrote no record (exit {p.returncode})")
    for path in REPLAY_KEYS:
        if dig(rec, path) != dig(new, path):
            out(1, f"REJECTED: replay: {'.'.join(path)} differs between the record and the fresh run "
                   f"(record {str(dig(rec, path))[:120]}, fresh {str(dig(new, path))[:120]})")
    last = new.get("status", "")
    out(p.returncode, f"{last}: replay of {d}: every pin reproduced by a fresh cold run ({fresh_dir / 'record.json'})"
        if p.returncode == 0 else f"REJECTED: replay: the fresh run's verdict is {last} (exit {p.returncode})")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("source", nargs="?")
    ap.add_argument("spec", nargs="?", help="natspec or none")
    ap.add_argument("--replay", metavar="DIR", help="check a finished run's directory against a fresh cold run")
    ap.add_argument("--invariants")
    ap.add_argument("--contract")
    ap.add_argument("--tier", choices=["compiled", "functional", "certified"])
    ap.add_argument("--solidity-lean", help="a solidity-lean git checkout to clone the base from")
    ap.add_argument("--base", help="the solidity-lean base commit (default tools/certify/solidity-lean.rev)")
    ap.add_argument("--work", help="a fresh work directory")
    ap.add_argument("--record-dir")
    ap.add_argument("--packages-from", help="a checkout whose .lake/packages to copy (still replayed in full)")
    ap.add_argument("--solc", default=os.environ.get("SOLC", str(pathlib.Path.home() / ".cache/solc" / f"solc-{SOLC_VERSION}")))
    ap.add_argument("--no-negatives", action="store_true")
    ap.add_argument("--probe-cache", help="a checkout whose .lake the probe clone starts from (lake rebuilds what differs)")
    ap.add_argument("--prepare-only", action="store_true", help="stop after the input commit (development)")
    a = ap.parse_args()
    if a.replay:
        replay_main(a)
    if not (a.source and a.spec and a.solidity_lean and a.work and a.record_dir):
        ap.error("SOURCE, SPEC, --solidity-lean, --work and --record-dir are required (or --replay DIR)")

    src = pathlib.Path(a.source).resolve()
    rec_dir = pathlib.Path(a.record_dir).resolve()
    rec_dir.mkdir(parents=True, exist_ok=True)
    work = pathlib.Path(a.work).resolve()
    if a.spec not in ("natspec", "none"):
        raise SystemExit("certify: SPEC must be natspec or none")
    tier = a.tier or ("certified" if a.spec == "natspec" else "compiled")
    text = src.read_text()
    src_sha = hashlib.sha256(text.encode()).hexdigest()
    base_name = re.sub(r"[^a-z0-9_]", "_", src.stem.lower()).strip("_") or "c"
    if not base_name[0].isalpha():
        base_name = "c_" + base_name
    name = f"{base_name}_{src_sha[:8]}"
    record = {
        "schema": "solar-certify-generic/1",
        "name": name,
        "started": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
        "command": [str(HERE / "certify"), *sys.argv[1:]],
        "requested_tier": tier,
        "source": {"file": str(src), "sha256": src_sha},
        "spec": {"kind": a.spec, "sha256": src_sha if a.spec == "natspec" else None},
        "invariants": {"file": a.invariants, "sha256": sha256_file(a.invariants) if a.invariants else None},
        "options": OPTIONS,
        "solar": {"commit": git(ROOT, "rev-parse", "HEAD"), "dirty": bool(git(ROOT, "status", "--porcelain", "--", "crates", "scripts", "tools/certify", "Cargo.toml", "Cargo.lock"))},
        "solc": {"version": SOLC_VERSION, "path": a.solc, "sha256": sha256_file(a.solc) if os.path.exists(a.solc) else None},
        "lean_toolchain": {"recorded_digest": (HERE / "toolchain.sha256").read_text().strip()},
    }

    def finish(status, code, line, **extra):
        record.update(extra)
        # The manifest: every file the driver leaves in the record directory but record.json.
        if record.get("run_files") is not None:
            for q in sorted(rec_dir.rglob("*")):
                rel = str(q.relative_to(rec_dir))
                if q.is_file() and rel != "record.json" and rel not in record["run_files"]:
                    record["run_files"][rel] = sha256_file(q)
        record["status"] = status
        record["finished"] = datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds")
        (rec_dir / "record.json").write_text(json.dumps(record, indent=2) + "\n")
        log(line)
        sys.exit(code)

    try:
        log(f"== generic: {src} ({src_sha}) as {name}, requested tier {tier}")
        ast, contracts = solc_ast(a.solc, src)
        contract, fns, events = envelope(text, ast, contracts, a.contract, a.invariants, a.spec == "natspec")
        record["contract"] = contract
        record["functions"] = [{"signature": f["sig"], "selector": f"0x{f['sel']:08x}", "params": f["params"]} for f in fns]
        record["environment"] = ENVIRONMENT
        log(f"envelope (source): inside; functions {', '.join(f['sig'] for f in fns)}")
        log("== generic: build solar")
        solar = build_solar()
        record["solar"]["binary_sha256"] = sha256_file(solar)
        rc, vout, _ = run([str(solar), "--version"])
        record["solar"]["version"] = vout.strip().splitlines()[0] if vout else ""
        plain, err = solar_runtime(solar, src, contract)
        if plain is None:
            raise Failed(f"solar does not compile the source: {err.strip()[:400]}")
        bad = scan_bytecode(plain)
        if bad:
            raise Blocked([{"feature": "bytecode", "where": f"pc {pc}", "detail": f"the runtime bytecode has {op}"} for op, pc in bad.items()])
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
        git(work, "clone", "-q", "--no-checkout", "--shared", str(lean_src), str(lean_dir)) if work.exists() else None
        if not lean_dir.exists():
            work.mkdir(parents=True)
            git(work, "clone", "-q", "--no-checkout", "--shared", str(lean_src), str(lean_dir))
        git(lean_dir, "checkout", "-q", "--detach", base)
        git(lean_dir, "submodule", "update", "-q", "--init", "--recursive", check=False)
        rc, body, err = run([sys.executable, str(lean_dir / "scripts/solc_ast_to_lean_source.py"), str(src), "--contract", contract,
                             "--solc", a.solc, "--namespace", f"Certified.{name}.Src", "--body-only"])
        if rc != 0:
            raise Blocked([{"feature": "source frontend", "where": "", "detail": "the solc-AST front end refuses the source: " + err.strip()[-300:]}])
        d = lean_dir / "Certified" / name
        source_text = source_lean(body)
        log("== generic: probe SolidCore's evaluation and the generic check (separate clone)")
        res, summary, reasons = run_probe(lean_dir, work, name, contract, fns, source_text, a.probe_cache or None)
        record["probe"] = summary
        for l in summary:
            log(l)
        if reasons:
            raise Blocked(reasons)
        d.mkdir(parents=True)
        shutil.copy(src, d / f"{name}.sol")
        (d / "options").write_text("\n".join(OPTIONS) + "\n")
        (d / "Source.lean").write_text(source_text)
        (d / "Lowering.lean").write_text(lowering_lean(name, contract, fns, events, res["core"], res["storage"]))
        # A placeholder of the proof module, which tools/certify/shape_gen.py regenerates in the run
        # (scripts/certify lists the statement's modules before step 6b writes it): its imports
        # are the generated module's, and nothing in it is read.
        (d / "Shape.lean").write_text("".join(f"import {m}\n" for m in (
            "Verify.ShapeGeneric", f"{name}.Lowering", f"{name}.Proofs.Stages", f"{name}.gen.{contract}.Bytecode"))
            + "\n-- Placeholder: tools/certify/shape_gen.py writes this module in the run.\n")
        spec_desc = ("none: no specification requested" if a.spec == "none" else
                     "solar-verify's NatSpec in the source: exported and checked by scripts/certify step 6; not connected to the published statement by this driver yet")
        witness = None
        if a.spec == "natspec":
            spec_desc = "solar-verify's export of the source's NatSpec in this run, read by the published statement (bundlePost, bundleAllowed)"
            # The statement's non-vacuity needs a call the spec requires to succeed: a function with
            # @custom:succeeds-if. Without one the allowed statuses reject nothing, and the certified
            # tier cannot be stated non-vacuously: the run is refused before any proof.
            witness = next((f["name"] for f in fns if f.get("mutates") and f.get("succeeds")), None) or \
                next((f["name"] for f in fns if f.get("succeeds")), None)
            if witness is None:
                raise Failed("spec: no function has @custom:succeeds-if, so the spec's allowed statuses reject nothing and "
                             "the certified statement's non-vacuity (a revert the spec rejects) cannot hold; add succeeds-if "
                             "conditions, or run with SPEC none")
        (d / "shape.json").write_text(json.dumps(shape_json(name, contract, fns, need, spec_desc, witness, events), indent=2) + "\n")
        with open(lean_dir / "lakefile.lean", "a") as f:
            f.write(lakefile_lib(name, contract))
        git(lean_dir, "add", "-A", "Certified", "lakefile.lean")
        git(lean_dir, "-c", "user.name=solar certify", "-c", "user.email=certify@localhost", "commit", "-q",
            "-m", f"certify: generated inputs for {name}")
        inputs = git(lean_dir, "rev-parse", "HEAD")
        changed = git(lean_dir, "diff", "--name-only", base, inputs).splitlines()
        stray = [p for p in changed if not (p.startswith(f"Certified/{name}/") or p == "lakefile.lean")]
        if stray:
            raise Failed(f"the input commit touches {stray}")
        record["solidity_lean_source"] = str(lean_src)
        record["solidity_lean"] = {"base": base, "inputs_commit": inputs, "inputs": changed,
                                   "inputs_sha256": {p: sha256_file(lean_dir / p) for p in changed}}
        # The run's directory keeps the source and every generated input, for --replay to check.
        shutil.copy(src, rec_dir / src.name)
        for q in changed:
            (rec_dir / "inputs" / q).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(lean_dir / q, rec_dir / "inputs" / q)
        record["run_files"] = {src.name: src_sha, **{f"inputs/{q}": sha256_file(lean_dir / q) for q in changed}}
        if a.prepare_only:
            finish("PREPARED", 1, f"PREPARED: {lean_dir} at {inputs} (no proof run)")
        if a.packages_from:
            pk = pathlib.Path(a.packages_from) / ".lake" / "packages"
            (lean_dir / ".lake").mkdir(exist_ok=True)
            rc, _, err = run(["cp", "-a", str(pk), str(lean_dir / ".lake" / "packages")])
            if rc != 0:
                raise Failed(f"cannot copy the packages: {err[:200]}")
            record["packages_from"] = str(pk)
        log(f"== generic: solidity-lean {base} + inputs {inputs}; scripts/certify")
        env = dict(os.environ, SOLIDITY_LEAN_DIR=str(lean_dir), CERTIFY_ALLOW_UNPINNED="1", CERTIFY_CLEAN_BUILD="1",
                   SOLC=a.solc, SOLAR=str(solar))
        env.pop("CERTIFY_REPLAY_TRUST_UPSTREAM", None)
        cmd = ["bash", str(ROOT / "scripts" / "certify"), str(d / f"{name}.sol"), "--name", name, "--contract", contract, "--require-final"]
        if a.no_negatives:
            cmd.append("--no-negatives")
        record["scripts_certify"] = {"command": cmd, "env": {k: env[k] for k in ("SOLIDITY_LEAN_DIR", "CERTIFY_ALLOW_UNPINNED", "CERTIFY_CLEAN_BUILD", "SOLC", "SOLAR")}}
        logf = rec_dir / "certify.log"
        with open(logf, "w") as lf:
            p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, env=env)
            for line in p.stdout:
                lf.write(line)
                lf.flush()
                sys.stdout.write(line)
            p.wait()
        record["scripts_certify"]["exit"] = p.returncode
        record["lean_toolchain"]["lean-toolchain"] = (lean_dir / "lean-toolchain").read_text().strip()
        inner = d / "record.json"
        if p.returncode != 0 or not inner.exists():
            tail = logf.read_text().strip().splitlines()[-3:]
            finish("REJECTED", 1, f"REJECTED: scripts/certify: exits {p.returncode}: {' | '.join(tail)}")
        rec = json.loads(inner.read_text())
        shutil.copy(inner, rec_dir / "certify-record.json")
        ok, need_c, comps = strict(dict(rec, name=name, contract=contract), tier)
        reached = "certified" if comps.get("strongest") else ("functional" if comps["spec"] and comps["equivalence"] else
                                                              ("compiled" if comps["equivalence"] and comps["resources"] else "none"))
        res = dict(statuses=rec.get("statuses"), strict={"tier": tier, "holds": ok, "components": need_c},
                   tier_reached=reached, replay=rec.get("replay"), caveats=rec.get("caveats"),
                   published_statement=rec.get("end_to_end_statement"), final=rec.get("final"))
        if ok:
            finish(tier.upper(), 0, f"{tier.upper()}: {name}: the requested tier holds (record {rec_dir / 'record.json'})", **res)
        missing = [k for k, v in need_c.items() if not v]
        finish("REJECTED", 2, f"REJECTED: strict tier: {name} reached {reached}, below the requested tier {tier} (false: {', '.join(missing)})", **res)
    except Blocked as b:
        finish("BLOCKED", 3, f"BLOCKED: {b}", blocked=b.reasons)
    except Failed as f:
        finish("REJECTED", 1, f"REJECTED: {f}")
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as e:
        finish("REJECTED", 1, f"REJECTED: driver: {type(e).__name__}: {e}")


if __name__ == "__main__":
    main()
