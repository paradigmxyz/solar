#!/usr/bin/env python3
"""Turn solc semantic-test expectation sequences into prefix+symbolic tasks."""
import json, pathlib, re, subprocess, sys
ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "target" / "symaudit"))
import campaign  # noqa

SEM = ROOT / "testdata/solidity/test/libsolidity/semanticTests"
CALL_RE = re.compile(r"^// ([A-Za-z_]\w*)\((.*?)\)(?::\s*(.*?))?\s*->(.*)$")
MAX_PREFIX = 8

def word(tok):
    tok = tok.strip()
    m = re.fullmatch(r'"((?:[^"\\]|\\.)*)"', tok)
    if m:
        b = m.group(1).encode("latin-1", "replace")
        if len(b) > 32: return None
        return int.from_bytes(b.ljust(32, b"\0"), "big")
    m = re.fullmatch(r'hex"([0-9a-fA-F]*)"', tok)
    if m:
        if len(m.group(1)) % 2: return None
        b = bytes.fromhex(m.group(1))
        if len(b) > 32: return None
        return int.from_bytes(b.ljust(32, b"\0"), "big")
    m = re.fullmatch(r"left\((.*)\)", tok)
    if m:
        w = word(m.group(1))
        if w is None: return None
        return (w << (8 * (32 - (w.bit_length() + 7) // 8))) % (1 << 256) if w else 0
    if tok in ("true", "false"):
        return 1 if tok == "true" else 0
    if re.fullmatch(r"-?\d+", tok):
        return int(tok) % (1 << 256)
    if re.fullmatch(r"-?0x[0-9a-fA-F]+", tok):
        return int(tok, 16) % (1 << 256)
    return None

def encode(sel, args):
    out = sel
    if args.strip():
        for tok in args.split(","):
            w = word(tok)
            if w is None:
                return None
            out += f"{w:064x}"
    return "0x" + out

def main():
    tasks = []
    for path in sorted(SEM.rglob("*.sol")):
        text = path.read_text(errors="replace")
        if campaign.MIN_SKIP_RE.search(text) or "// ----" not in text:
            continue
        expect = text.split("// ----", 1)[1]
        r = subprocess.run(["solc", "--abi", "--hashes", "--evm-version", "osaka", "--via-ir", "--optimize", str(path)],
                           capture_output=True, text=True, errors="replace", timeout=30, cwd=path.parent)
        if r.returncode != 0:
            continue
        # contract sections
        sections = re.split(r"^=======\s+.*?:(\w+)\s+=======$", r.stdout, flags=re.M)
        contracts = {}
        for i in range(1, len(sections), 2):
            name, body = sections[i], sections[i+1]
            abi = None; hashes = {}
            for line in body.splitlines():
                if line.startswith("["):
                    try: abi = json.loads(line)
                    except json.JSONDecodeError: pass
                m = re.match(r"^([0-9a-f]{8}): (.*)$", line.strip())
                if m: hashes[m.group(2)] = m.group(1)
            if abi is not None:
                contracts[name] = (abi, hashes)
        if not contracts:
            continue
        # target contract: the last one with functions (solc tests deploy the last contract)
        cname = None
        for name, (abi, hashes) in contracts.items():
            if any(e.get("type") == "function" for e in abi): cname = name
        if cname is None: continue
        abi, hashes = contracts[cname]
        if any(e.get("type") == "constructor" and e.get("inputs") for e in abi): continue
        mut = {campaign.sig(e): e["stateMutability"] for e in abi if e.get("type") == "function"}
        calls = []
        for line in expect.splitlines():
            m = CALL_RE.match(line)
            if not m:
                if line.startswith("// ~") or line.startswith("// gas") or line.startswith("// library:"): continue
                if line.startswith("// ") and "->" in line: break  # unsupported syntax (value, etc.)
                continue
            name, types, args = m.group(1), m.group(2), m.group(3) or ""
            if name == "constructor": break
            if "FAILURE" in (m.group(4) or ""):
                continue  # a reverting call leaves no state; skip it
            sig = f"{name}({types})"
            if sig not in hashes or sig not in mut or mut[sig] == "payable": break
            cd = encode(hashes[sig], args)
            if cd is None: break
            calls.append((sig, cd))
        for i in range(1, min(len(calls), MAX_PREFIX + 1)):
            sig = calls[i][0]
            if mut[sig] == "pure": continue
            prefix = ",".join(cd for _, cd in calls[:i])
            tasks.append(f"{path.relative_to(ROOT)}|{cname}|{sig}|{mut[sig]}|{prefix}")
    pathlib.Path(ROOT / "target/symaudit/prefix-tasks3.txt").write_text("\n".join(tasks) + "\n")
    print(len(tasks), "tasks")

main()
