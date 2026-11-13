#!/usr/bin/env python3
"""Checks that the certified bytes are the compiler's bytes, and that they rebuild.

verify-repro.sh runs this from the repository root with one compiler binary, or two built in
separate clean target directories. Each check prints PASS, FAIL or SKIP with what it compared,
and the summary at the end counts them; any FAIL exits 1.

neutrality  Every Solidity source of tests/ui/codegen, the solar-tv milestones and examples and
            crates/verify/examples, at -Ogas and -Osize: -Zemit-lean-stages leaves the
            compiler's bin and bin-runtime output byte for byte as it is, a source the exporter
            rejects fails with no output and no manifest, and an accepted export writes the same
            files with one thread as with several.
pin         Each milestone's pinned bytecode.hex is what solar emits with its options.
manifest    The committed manifest binds the source's SHA-256, the options, the stage files'
            SHA-256 and the bytecode with its SHA-256, in manifest.json and Manifest.lean alike.
literal     The bytecode literal the assembly theorem reads (Bytecode.lean's bytecode, then its
            program data) is the pinned bytecode.
deploy      Deploying solar's creation code in evm2 (solar-tv-replay --deploy) installs the
            pinned bytecode as the account's code.
commit      The compiler commit the committed manifest names exists and has the checkout's
            compiler sources.
regen       In a clean worktree at HEAD, the export and certify.py regenerate every committed
            file of the milestone byte for byte, but for the compiler's version and commit in the
            manifest; with two compilers, both regenerate the same files.
lean        (--lean) Lean builds the milestone's proofs, the assembly theorem reads the
            exported bytecode constant, #eval of that constant is the pinned bytecode, and the
            theorems depend on no axioms beyond propext, Classical.choice and Quot.sound.
solidity-lean
            (--solidity-lean DIR) Each milestone a solidity-lean checkout imports was pinned,
            at the solar commit its lake manifest names, to what solar emits now.
"""

import argparse
import concurrent.futures
import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
MILESTONES = ROOT / "solar-tv" / "milestone"
CORPUS = [
    "tests/ui/codegen",
    "solar-tv/milestone",
    "solar-tv/examples/solidity",
    "crates/verify/examples",
]
LEVELS = ["-Ogas", "-Osize"]
COMPILER_SOURCES = ["crates", "Cargo.toml", "Cargo.lock"]
AXIOMS = {"propext", "Classical.choice", "Quot.sound"}

results: list[tuple[str, str, str, str]] = []


def report(check: str, subject: str, status: str, detail: str = "") -> None:
    results.append((check, subject, status, detail))
    line = f"{status:4} {check:10} {subject}"
    print(f"{line}: {detail}" if detail else line, flush=True)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def run(
    args: list[str], cwd: pathlib.Path = ROOT, **kwargs
) -> subprocess.CompletedProcess:
    return subprocess.run(args, cwd=cwd, capture_output=True, check=False, **kwargs)


def git(*args: str, cwd: pathlib.Path = ROOT) -> subprocess.CompletedProcess:
    return run(["git", *args], cwd=cwd, text=True)


def sources() -> list[str]:
    found = []
    for top in CORPUS:
        for dirpath, dirnames, filenames in os.walk(ROOT / top):
            # Auxiliary sources are imported by the tests beside them, not compiled alone.
            dirnames[:] = sorted(d for d in dirnames if d != "auxiliary")
            for name in filenames:
                if name.endswith(".sol"):
                    found.append(str(pathlib.Path(dirpath, name).relative_to(ROOT)))
    return sorted(found)


def tree(directory: pathlib.Path) -> dict[str, bytes]:
    out = {}
    for dirpath, _, filenames in os.walk(directory):
        for name in filenames:
            path = pathlib.Path(dirpath, name)
            out[str(path.relative_to(directory))] = path.read_bytes()
    return out


def without_command_line(files: dict[str, bytes], directory: str) -> dict[str, bytes]:
    """The files with the manifests' command line dropped and the export directory renamed."""
    out = {}
    for path, data in files.items():
        if path.endswith("manifest.json"):
            manifest = json.loads(data)
            manifest.pop("arguments")
            manifest["options"] = manifest["options"].replace(directory, "DIR")
            data = json.dumps(manifest, sort_keys=True).encode()
        elif path.endswith("Manifest.lean"):
            data = re.sub(
                rb"def arguments : List String :=\n.*?\n\n", b"", data, flags=re.DOTALL
            )
        out[path] = data
    return out


def neutrality_one(solar: str, source: str) -> list[tuple[str, str, str]]:
    out = []
    for level in LEVELS:
        args = [
            solar,
            source,
            level,
            "--evm-version",
            "osaka",
            "--emit=bin,bin-runtime",
        ]
        env = dict(os.environ, SOLAR_TEST_VERSION="1")
        plain = run(args, env=env, timeout=600)
        if plain.returncode != 0:
            out.append((level, "not compiled", ""))
            continue
        with tempfile.TemporaryDirectory() as a, tempfile.TemporaryDirectory() as b:
            exported = run([*args, f"-Zemit-lean-stages={a}"], env=env, timeout=600)
            written = tree(pathlib.Path(a))
            manifests = [
                p for p in written if p.endswith(("manifest.json", "Manifest.lean"))
            ]
            if exported.returncode != 0:
                if exported.stdout:
                    out.append((level, "FAIL", "the rejected export printed output"))
                elif manifests:
                    out.append(
                        (level, "FAIL", f"the rejected export wrote {manifests}")
                    )
                else:
                    out.append((level, "rejected", ""))
                continue
            if exported.stdout != plain.stdout:
                out.append((level, "FAIL", "-Zemit-lean-stages changed the output"))
                continue
            if not manifests:
                out.append((level, "no stages", ""))
                continue
            single = run(
                [*args, f"-Zemit-lean-stages={b}", "-j1"], env=env, timeout=600
            )
            other = tree(pathlib.Path(b))
            if single.returncode != 0 or single.stdout != plain.stdout:
                out.append(
                    (level, "FAIL", "the export with one thread differs in its output")
                )
            elif without_command_line(written, a) != without_command_line(other, b):
                differ = sorted(
                    p
                    for p in set(written) | set(other)
                    if written.get(p) != other.get(p)
                )
                out.append((level, "FAIL", f"one thread writes other files: {differ}"))
            else:
                out.append((level, "exported", ""))
    return out


def neutrality(solar: str, jobs: int) -> None:
    files = sources()
    counts: dict[str, int] = {}
    failures = 0
    with concurrent.futures.ThreadPoolExecutor(jobs) as pool:
        for source, outcome in zip(
            files, pool.map(lambda s: neutrality_one(solar, s), files)
        ):
            for level, status, detail in outcome:
                counts[status] = counts.get(status, 0) + 1
                if status == "FAIL":
                    failures += 1
                    report("neutrality", f"{source} {level}", "FAIL", detail)
    summary = ", ".join(f"{n} {k}" for k, n in sorted(counts.items()))
    subject = f"{len(files)} sources x {len(LEVELS)} levels"
    report("neutrality", subject, "FAIL" if failures else "PASS", summary)


class Milestone:
    """A committed milestone: its source, options, contract and committed files."""

    def __init__(self, directory: pathlib.Path):
        self.name = directory.name
        self.dir = directory
        self.rel = directory.relative_to(ROOT)
        if self.name == "w1":
            self.source = self.rel / "w1.sol"
            self.options = ["-Ogas", "--evm-version", "osaka"]
            self.contract = "Main"
            self.gen = self.rel / "gen"
            text = (directory / "gen.sh").read_text()
            pin = re.search(r"^expected=([0-9a-f]+)$", text, re.MULTILINE)
            assert pin, "w1/gen.sh pins no bytecode"
            self.pin = pin.group(1)
        else:
            self.source = self.rel / f"{self.name}.sol"
            self.options = (directory / "options").read_text().split("\n")[:-1]
            (self.contract,) = os.listdir(directory / "gen")
            self.gen = self.rel / "gen"
            self.pin = (directory / "bytecode.hex").read_text().strip()
        self.contract_gen = self.gen / self.contract
        self.manifest = json.loads(
            (ROOT / self.contract_gen / "manifest.json").read_text()
        )

    def compile(self, solar: str, cwd: pathlib.Path, *extra: str) -> dict:
        args = [
            solar,
            str(self.source),
            *self.options,
            "--emit=bin,bin-runtime",
            *extra,
        ]
        out = run(args, cwd=cwd)
        if out.returncode != 0:
            raise RuntimeError(out.stderr.decode()[-2000:])
        return json.loads(out.stdout)["contracts"][f"{self.source}:{self.contract}"]


def lean_string(text: str, name: str) -> str | None:
    found = re.search(rf'^def {name} : String :=\s*"([^"]*)"', text, re.MULTILINE)
    return found.group(1) if found else None


def lean_bytes(text: str, name: str) -> list[int] | None:
    found = re.search(rf"^def {name} : List Nat :=\s*\[([^\]]*)\]", text, re.MULTILINE)
    return (
        [int(b, 16) for b in found.group(1).replace(",", " ").split()]
        if found
        else None
    )


def check_pin(m: Milestone, solar: str) -> dict | None:
    try:
        out = m.compile(solar, ROOT)
    except RuntimeError as error:
        report("pin", m.name, "FAIL", f"solar rejects it: {error}")
        return None
    if out["bin-runtime"] == m.pin:
        report(
            "pin",
            m.name,
            "PASS",
            f"{len(m.pin) // 2} bytes, sha256 {sha256(bytes.fromhex(m.pin))}",
        )
    else:
        emitted = out["bin-runtime"]
        report(
            "pin",
            m.name,
            "FAIL",
            f"solar emits {len(emitted) // 2} bytes ({emitted[:48]}...), the pin is {len(m.pin) // 2}",
        )
    return out


def check_manifest(m: Milestone) -> None:
    gen = ROOT / m.contract_gen
    man = m.manifest
    problems = []
    source = ROOT / m.source
    if [s["path"] for s in man["sources"]] != [str(m.source)]:
        problems.append(f"sources {man['sources']}")
    elif man["sources"][0]["sha256"] != sha256(source.read_bytes()):
        problems.append("the source's SHA-256")
    if man["bytecode"] != m.pin:
        problems.append("bytecode is not the pin")
    if man["bytecode_sha256"] != sha256(bytes.fromhex(man["bytecode"])):
        problems.append("bytecode_sha256")
    expected_args = [
        str(m.source),
        *m.options,
        "--emit=bin-runtime",
        f"-Zemit-lean-stages={m.gen}",
    ]
    if man["arguments"] != expected_args:
        problems.append(f"arguments {man['arguments']} are not {expected_args}")
    if f"emit_lean_stages: Some({json.dumps(str(m.gen))})" not in man["options"]:
        problems.append("options do not name the export directory")
    for stage in man["stages"]:
        if stage["sha256"] != sha256((gen / stage["file"]).read_bytes()):
            problems.append(f"{stage['file']}'s SHA-256")
    lean = (gen / "Manifest.lean").read_text()
    if lean_string(lean, "bytecodeHex") != man["bytecode"]:
        problems.append("Manifest.lean bytecodeHex")
    if lean_string(lean, "bytecodeSha256") != man["bytecode_sha256"]:
        problems.append("Manifest.lean bytecodeSha256")
    if lean_string(lean, "compiler") != man["compiler"]:
        problems.append("Manifest.lean compiler")
    for source_entry in man["sources"]:
        if f'("{source_entry["path"]}", "{source_entry["sha256"]}")' not in lean:
            problems.append("Manifest.lean sources")
    for stage in man["stages"]:
        if f'"{stage["sha256"]}"' not in lean:
            problems.append(f"Manifest.lean {stage['file']}")
    if problems:
        report("manifest", m.name, "FAIL", "; ".join(problems))
    else:
        report(
            "manifest",
            m.name,
            "PASS",
            f"{len(man['stages'])} stages bound to source and bytecode",
        )


def check_literal(m: Milestone) -> None:
    text = (ROOT / m.contract_gen / "Bytecode.lean").read_text()
    code = lean_bytes(text, "bytecode")
    data = lean_bytes(text, "data") or []
    start = re.search(r"^def dataStart : Nat := (\d+)$", text, re.MULTILINE)
    if code is None:
        report("literal", m.name, "FAIL", "Bytecode.lean defines no bytecode")
        return
    if start and int(start.group(1)) != len(code):
        report("literal", m.name, "FAIL", "dataStart is not the end of the code")
        return
    literal = bytes(code + data).hex()
    if literal != m.pin or lean_string(text, "bytecodeHex") != m.pin:
        report("literal", m.name, "FAIL", f"Bytecode.lean holds {literal}")
    else:
        report(
            "literal",
            m.name,
            "PASS",
            f"bytecode ({len(code)} bytes) and bytecodeHex are the pin",
        )


def check_deploy(m: Milestone, out: dict | None, replay: str) -> None:
    if out is None:
        report("deploy", m.name, "SKIP", "solar rejects the milestone")
        return
    request = json.dumps(
        {"version": 1, "evm_version": evm_version(m), "initcode": out["bin"]}
    )
    answer = run([replay, "--deploy"], input=request.encode())
    try:
        result = json.loads(answer.stdout)
    except json.JSONDecodeError:
        report("deploy", m.name, "FAIL", answer.stderr.decode()[-500:])
        return
    if result.get("status") != "ok":
        report("deploy", m.name, "FAIL", str(result.get("reason")))
        return
    deployed = result["deployed"]["runtime"]
    pin = bytearray.fromhex(m.pin)
    code = bytearray.fromhex(deployed)
    # NOTE: the deployment writes each immutable over its placeholder, so those bytes may differ.
    for immutable in m.manifest.get("immutables", []):
        window = slice(immutable["start"], immutable["start"] + immutable["bytes"])
        code[window] = pin[window]
    if code != pin:
        report(
            "deploy",
            m.name,
            "FAIL",
            f"evm2 installs {len(code)} bytes, {deployed[:64]}..., not the pin",
        )
    else:
        digest = result["deployed"]["code_hash"]
        report("deploy", m.name, "PASS", f"evm2 installs the pin, code hash {digest}")


def evm_version(m: Milestone) -> str:
    return m.manifest["evm_version"]


def check_commit(m: Milestone, head: str) -> None:
    named = m.manifest.get("compiler_commit")
    if not named:
        short = re.search(r"\+commit\.([0-9a-f]{8})", m.manifest["compiler"])
        named = short.group(1) if short else ""
    resolved = git("rev-parse", "--verify", "--quiet", f"{named}^{{commit}}")
    if not named or resolved.returncode != 0:
        report(
            "commit",
            m.name,
            "FAIL",
            f"the manifest's compiler commit {named or '?'} is not in the repository",
        )
        return
    commit = resolved.stdout.strip()
    if git("diff", "--quiet", commit, head, "--", *COMPILER_SOURCES).returncode != 0:
        report(
            "commit",
            m.name,
            "FAIL",
            f"compiler commit {commit[:12]} has other compiler sources than HEAD",
        )
    else:
        report("commit", m.name, "PASS", f"{commit[:12]} has HEAD's compiler sources")


def regenerate(m: Milestone, solar: str, work: pathlib.Path) -> dict[str, bytes]:
    """Exports and certifies the milestone in the worktree; returns its files."""
    directory = work / m.rel
    gen = work / m.gen
    shutil.rmtree(gen)
    args = [
        solar,
        str(m.source),
        *m.options,
        "--emit=bin-runtime",
        f"-Zemit-lean-stages={m.gen}",
    ]
    out = run(args, cwd=work)
    if out.returncode != 0:
        raise RuntimeError(out.stderr.decode()[-2000:])
    certify = [sys.executable, "solar-tv/milestone/certify.py", str(m.contract_gen)]
    if m.name == "w1":
        check = (directory / "check.sh").read_text()
        axioms = re.search(r"--axioms \"\$\(cat <<'EOF'\n(.*?)\nEOF", check, re.DOTALL)
        assert axioms, "w1/check.sh passes no axioms"
        # check.sh lets certify.py overwrite its modules beside w1's own library files.
        proofs = directory / "proofs" / "W1Proofs"
        certify += [
            "--gen-module",
            "gen.Main",
            "--proof-dir",
            str(proofs.relative_to(work)),
        ]
        certify += ["--proof-module", "W1Proofs", "--namespace", "SolarTV.Milestone.W1"]
        certify += ["--label", "w1", "--axioms", axioms.group(1)]
        # gen.sh writes the stage list lake builds.
        manifest = json.loads((gen / "Main" / "manifest.json").read_text())
        imports = "".join(
            f"import gen.Main.{s['file'][:-5]}\n" for s in manifest["stages"]
        )
        (gen / "Stages.lean").write_text(
            "-- Generated by gen.sh from Main/manifest.json; do not edit.\n"
            + imports
            + "import gen.Main.Manifest\n"
        )
    else:
        cap = "".join(p[:1].upper() + p[1:] for p in m.name.split("_"))
        loops = (
            (directory / "loops").read_text().strip()
            if (directory / "loops").exists()
            else "0"
        )
        shutil.rmtree(directory / "Proofs")
        (directory / "Proofs.lean").unlink()
        certify += ["--gen-module", f"{m.name}.gen.{m.contract}"]
        certify += [
            "--proof-dir",
            f"{m.rel}/Proofs",
            "--proof-module",
            f"{m.name}.Proofs",
        ]
        certify += [
            "--namespace",
            f"SolarTV.Milestone.{cap}",
            "--label",
            m.name,
            "--loops",
            loops,
        ]
    out = run(certify, cwd=work)
    if out.returncode != 0:
        raise RuntimeError(out.stderr.decode()[-2000:])
    return tree(directory)


def without_compiler(files: dict[str, bytes]) -> dict[str, bytes]:
    """The files with the compiler's version and commit dropped from the manifests."""
    out = {}
    for path, data in files.items():
        if path.endswith("manifest.json"):
            manifest = json.loads(data)
            manifest.pop("compiler")
            manifest.pop("compiler_commit", None)
            data = json.dumps(manifest, sort_keys=True).encode()
        elif path.endswith("Manifest.lean"):
            data = re.sub(
                rb'^def compiler(Commit)? : String := "[^"]*"\n\n',
                b"",
                data,
                flags=re.MULTILINE,
            )
        out[path] = data
    return out


def check_regen(
    milestones: list[Milestone], solars: list[str], head: str, keep: bool
) -> None:
    trees: dict[str, list[dict[str, bytes]]] = {m.name: [] for m in milestones}
    for index, solar in enumerate(solars):
        with tempfile.TemporaryDirectory(prefix="verify-repro-") as scratch:
            work = pathlib.Path(scratch) / "tree"
            added = git("worktree", "add", "--detach", str(work), head)
            if added.returncode != 0:
                report("regen", f"compiler {index + 1}", "FAIL", added.stderr.strip())
                continue
            try:
                for m in milestones:
                    try:
                        trees[m.name].append(regenerate(m, solar, work))
                    except (RuntimeError, OSError) as error:
                        report(
                            "regen",
                            f"{m.name} (compiler {index + 1})",
                            "FAIL",
                            str(error),
                        )
                        trees[m.name].append({})
                if keep:
                    kept = ROOT / "target" / "verify-repro" / f"regen-{index + 1}"
                    shutil.rmtree(kept, ignore_errors=True)
                    shutil.copytree(work / "solar-tv" / "milestone", kept)
                    print(f"kept the regenerated milestones in {kept}")
            finally:
                git("worktree", "remove", "--force", str(work))
    for m in milestones:
        committed = tree(ROOT / m.rel)
        regenerated = trees[m.name]
        if not all(regenerated):
            continue
        differ = set()
        for files in regenerated:
            a, b = without_compiler(committed), without_compiler(files)
            differ |= {p for p in set(a) | set(b) if a.get(p) != b.get(p)}
        same = all(files == regenerated[0] for files in regenerated)
        if differ:
            shown = ", ".join(sorted(differ)[:8]) + (" ..." if len(differ) > 8 else "")
            report(
                "regen",
                m.name,
                "FAIL",
                f"{len(differ)} files differ from the committed ones: {shown}",
            )
        elif not same:
            report(
                "regen",
                m.name,
                "FAIL",
                "the two compilers write different compiler fields",
            )
        else:
            what = "both compilers regenerate" if len(solars) > 1 else "regenerates"
            report("regen", m.name, "PASS", f"{what} all {len(committed)} files")


def audit_text(module: str, namespace: str, export: str, final: str) -> str:
    """The Lean commands that read the assembly theorem's bytecode constant and the axioms."""
    return f"""import {module}
import Lean
open Lean in
#eval show Elab.Command.CommandElabM Unit from do
  let env ← getEnv
  let some info := env.find? `{namespace}.decodes | throwError "no decodes theorem"
  unless info.type.getUsedConstants.contains `{export}.bytecode do
    throwError "decodes does not read {export}.bytecode"
  logInfo "decodes reads {export}.bytecode"
#eval IO.println ("bytecode=" ++ String.join ({export}.bytecode.map fun b =>
  let d := Nat.toDigits 16 b; if d.length < 2 then String.ofList ('0' :: d) else String.ofList d))
#print axioms {final}
#print axioms {namespace}.walk_eq
#print axioms {namespace}.decodes
"""


def check_lean(milestones: list[Milestone], memory: str) -> None:
    tv = ROOT / "solar-tv"
    env = dict(os.environ, LEAN_NUM_THREADS=os.environ.get("LEAN_NUM_THREADS", "1"))
    for m in milestones:
        if m.name == "w1":
            library, module, namespace = "W1Proofs", "W1Proofs", "SolarTV.Milestone.W1"
            final = f"{namespace}.w1_mir_to_evmir"
        else:
            cap = "".join(p[:1].upper() + p[1:] for p in m.name.split("_"))
            library, module, namespace = (
                m.name,
                f"{m.name}.Proofs",
                f"SolarTV.Milestone.{cap}",
            )
            final = f"{namespace}.{m.name}_mir_to_evmir"
        built = run(["lake", "build", library], cwd=tv, env=env, text=True)
        if built.returncode != 0:
            log = built.stdout + built.stderr
            failed = re.findall(r"^- (\S+)$", log, re.MULTILINE)
            first = re.search(r"^error: (.*)$", log, re.MULTILINE)
            reason = first.group(1)[:300] if first else log[-300:]
            report(
                "lean",
                m.name,
                "FAIL",
                f"{len(failed)} modules fail: {', '.join(failed)}; first: {reason}",
            )
            continue
        export = f"SolarTV.Export.{m.contract}"
        audit = audit_text(module, namespace, export, final)
        with tempfile.NamedTemporaryFile(
            "w", suffix=".lean", dir=tv / ".lake", delete=False
        ) as f:
            f.write(audit)
        try:
            checked = run(
                ["lake", "env", "lean", "-M", memory, f.name],
                cwd=tv,
                env=env,
                text=True,
            )
        finally:
            os.unlink(f.name)
        text = checked.stdout + checked.stderr
        problems = []
        if checked.returncode != 0:
            problems.append(text[-1500:])
        found = re.search(r"^bytecode=([0-9a-f]*)$", text, re.MULTILINE)
        start = re.search(
            r"^def dataStart : Nat := (\d+)$",
            (ROOT / m.contract_gen / "Bytecode.lean").read_text(),
            re.MULTILINE,
        )
        code = m.pin[: 2 * int(start.group(1))] if start else m.pin
        if not found or found.group(1) != code:
            problems.append(
                f"#eval {export}.bytecode is {found.group(1) if found else '?'}"
            )
        axioms = re.findall(r"depends on axioms: \[([^\]]*)\]", text)
        axioms += [""] * len(re.findall(r"does not depend on any axioms", text))
        if len(axioms) != 3:
            problems.append("an audited theorem is missing")
        extra = {a.strip() for line in axioms for a in line.split(",")} - AXIOMS - {""}
        if extra:
            problems.append(f"axioms beyond the standard three: {sorted(extra)}")
        if "sorryAx" in text or "ofReduceBool" in text:
            problems.append("sorryAx or ofReduceBool")
        if problems:
            report("lean", m.name, "FAIL", "; ".join(problems))
        else:
            report(
                "lean",
                m.name,
                "PASS",
                f"{final.rsplit('.', 1)[1]} and decodes, kernel-checked, read the pinned bytecode",
            )


def check_solidity_lean(
    milestones: list[Milestone], checkout: pathlib.Path, solar: str
) -> None:
    """solidity-lean's theorems about solar's bytecode read the milestones of the solar commit
    its lake manifest pins: that commit's pins must be what solar emits now."""
    manifest = json.loads((checkout / "lake-manifest.json").read_text())
    (rev,) = [p["rev"] for p in manifest["packages"] if p["name"] == "solar_tv"]
    imported = set()
    for path in checkout.rglob("*.lean"):
        if ".lake" in path.parts:
            continue
        text = path.read_text(errors="replace")
        imported |= set(re.findall(r"^import (\w+)\.gen\.", text, re.MULTILINE))
        if re.search(r"^import (W1Proofs|gen\.Main)", text, re.MULTILINE):
            imported.add("w1")
    if git("cat-file", "-e", f"{rev}^{{commit}}").returncode != 0:
        report(
            "solidity-lean",
            rev[:12],
            "FAIL",
            "its pinned solar commit is not in this repository",
        )
        return
    for m in milestones:
        if m.name not in imported:
            continue
        if m.name == "w1":
            shown = git("show", f"{rev}:{m.rel}/gen.sh")
            found = re.search(r"^expected=([0-9a-f]+)$", shown.stdout, re.MULTILINE)
            pinned = found.group(1) if found else ""
        else:
            pinned = git("show", f"{rev}:{m.rel}/bytecode.hex").stdout.strip()
        try:
            emitted = m.compile(solar, ROOT)["bin-runtime"]
        except RuntimeError as error:
            report("solidity-lean", m.name, "FAIL", f"solar rejects it: {error}")
            continue
        if pinned == emitted:
            report(
                "solidity-lean",
                m.name,
                "PASS",
                f"solar at {rev[:12]} pins what solar emits",
            )
        else:
            report(
                "solidity-lean",
                m.name,
                "FAIL",
                f"its theorems read the {len(pinned) // 2} bytes pinned at {rev[:12]}; "
                f"solar emits {len(emitted) // 2} bytes",
            )


def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument(
        "--solar",
        action="append",
        required=True,
        help="a compiler binary; give two to compare clean builds",
    )
    ap.add_argument("--replay", required=True, help="the solar-tv-replay binary")
    ap.add_argument("--jobs", type=int, default=os.cpu_count() or 1)
    ap.add_argument(
        "--lean", action="store_true", help="also build and audit the proofs"
    )
    ap.add_argument("--lean-memory", default="3000")
    ap.add_argument(
        "--skip-corpus", action="store_true", help="skip the neutrality corpus"
    )
    ap.add_argument(
        "--keep",
        action="store_true",
        help="keep the regenerated milestones in target/verify-repro",
    )
    ap.add_argument(
        "--solidity-lean",
        type=pathlib.Path,
        help="also check the milestones a solidity-lean checkout's theorems read",
    )
    ap.add_argument("milestone", nargs="*", help="milestones to check (default: all)")
    args = ap.parse_args()
    args.solar = [str(pathlib.Path(s).resolve()) for s in args.solar]
    args.replay = str(pathlib.Path(args.replay).resolve())

    head = git("rev-parse", "HEAD").stdout.strip()
    if git("diff", "--quiet", "HEAD", "--", *COMPILER_SOURCES).returncode != 0:
        print("error: the compiler sources have uncommitted changes", file=sys.stderr)
        return 2
    names = args.milestone or sorted(
        d.name
        for d in MILESTONES.iterdir()
        if (d / "gen").is_dir() and ((d / "bytecode.hex").exists() or d.name == "w1")
    )
    milestones = [Milestone(MILESTONES / name) for name in names]
    if not args.skip_corpus:
        for solar in args.solar:
            neutrality(solar, args.jobs)
    for m in milestones:
        out = check_pin(m, args.solar[0])
        check_manifest(m)
        check_literal(m)
        check_deploy(m, out, args.replay)
        check_commit(m, head)
    if len(args.solar) > 1:
        check_binaries(args.solar)
    check_regen(milestones, args.solar, head, args.keep)
    if args.lean:
        check_lean(milestones, args.lean_memory)
    if args.solidity_lean:
        check_solidity_lean(milestones, args.solidity_lean, args.solar[0])

    print("\nsummary")
    checks = sorted({r[0] for r in results}, key=[r[0] for r in results].index)
    for check in checks:
        statuses = [r[2] for r in results if r[0] == check]
        counts = ", ".join(
            f"{statuses.count(s)} {s}"
            for s in ("PASS", "FAIL", "SKIP")
            if s in statuses
        )
        failing = [r[1] for r in results if r[0] == check and r[2] == "FAIL"]
        print(
            f"  {check:10} {counts}" + (f"  ({', '.join(failing)})" if failing else "")
        )
    failed = sum(r[2] == "FAIL" for r in results)
    print(
        f"{'FAILED' if failed else 'OK'}: {len(results) - failed} of {len(results)} checks pass at {head[:12]}"
    )
    return 1 if failed else 0


def check_binaries(solars: list[str]) -> None:
    """Two clean builds: report whether the binaries are identical, then compare their outputs."""
    digests = [sha256(pathlib.Path(s).read_bytes()) for s in solars]
    same = len(set(digests)) == 1
    detail = (
        "identical binaries" if same else "the binaries differ (outputs compared below)"
    )
    report(
        "rebuild", "binaries", "PASS", f"{detail}: {', '.join(d[:16] for d in digests)}"
    )
    mismatches = 0
    files = sources()
    for source in files:
        outputs = []
        for solar in solars:
            for level in LEVELS:
                args = [
                    solar,
                    source,
                    level,
                    "--evm-version",
                    "osaka",
                    "--emit=bin,bin-runtime",
                ]
                out = run(args, env=dict(os.environ, SOLAR_TEST_VERSION="1"))
                outputs.append((level, out.returncode, out.stdout))
        half = len(outputs) // 2
        if outputs[:half] != outputs[half:]:
            mismatches += 1
            report("rebuild", source, "FAIL", "the two builds compile it differently")
    if not mismatches:
        report(
            "rebuild",
            f"{len(files)} sources x {len(LEVELS)} levels",
            "PASS",
            "both builds emit the same bytes",
        )


if __name__ == "__main__":
    sys.exit(main())
