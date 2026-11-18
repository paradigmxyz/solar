#!/usr/bin/env python3
"""The run's binding manifest: one JSON file that ties the pinned inputs, the producer and the
freshly emitted runtime to the published expected-type theorem and its replay, each field checked
against the run's own files. Any mismatch fails closed (exit 1) and writes nothing.

  python3 -I bind.py --record RECORD.json --dir CERTIFIED/NAME --replay RECEIPT.json
      --solar SOLAR --driver-root SOLAR_CHECKOUT --out bind.json

Checked and recorded:
  inputs    the source's sha256 (record, manifest.json sources), the specification's, and the
            options (record options == the manifest's arguments between the source and
            --emit=bin-runtime; the arguments in their canonical relative form, never a temporary
            directory)
  producer  the solar binary's sha256 (record.solar.binary_sha256, and CERTIFY_SOLAR_SHA256 when
            set), its embedded commit (record.solar.binary_commit) equal to the driver checkout's
            HEAD and to record.solar.commit, dirty false
  runtime   bytecode.hex, the manifest's bytecode and bytecode_sha256, record.bytecode_sha256 and
            the final record's manifest binding, all the same bytes
  theorem   Certified.NAME.Final.final : Certified.NAME.Final.Statement, the Generic.Expected shape,
            caller "any", with the manifest binding in its statement
  replay    the receipt's theorem and statement are those; closure == replayed, trusted 0,
            unaccounted [], axioms within propext, Classical.choice, Quot.sound, no sorry; and the
            four modules each have constants in the replayed closure: the export Manifest
            (SolarTV.Export.C.Manifest.*), the Statement (NAME.Final.Statement), the proof
            (the final record's proof constant) and Final (NAME.Final.final)
"""

import argparse
import hashlib
import json
import os
import pathlib
import re
import subprocess
import sys

AXIOMS = {"propext", "Classical.choice", "Quot.sound"}


class Unbound(Exception):
    pass


def sha(p):
    return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()


def need(cond, why):
    if not cond:
        raise Unbound(why)


def head(root):
    p = subprocess.run(
        ["git", "-C", str(root), "rev-parse", "HEAD"],
        capture_output=True,
        text=True,
        check=False,
    )
    need(p.returncode == 0, f"cannot read the driver checkout's HEAD in {root}")
    return p.stdout.strip()


def build(record, d, receipt, solar_sha, driver_commit, env_sha=None):
    """The binding of a run from its record, its Certified/NAME directory D, the replay receipt,
    the solar binary's digest and the driver checkout's commit. Raises Unbound naming the first
    field that does not hold."""
    d = pathlib.Path(d)
    name, contract = record.get("name"), record.get("contract")
    need(
        isinstance(name, str) and isinstance(contract, str),
        "the record names no run or contract",
    )
    ns = f"Certified.{name}.Final"
    m = json.loads((d / "gen" / contract / "manifest.json").read_text())
    src = f"Certified/{name}/{name}.sol"
    final = record.get("final") or {}
    fm = final.get("manifest") or {}

    # inputs
    source = (record.get("source") or {}).get("sha256")
    need(re.fullmatch(r"[0-9a-f]{64}", str(source)), "the record has no source sha256")
    need(sha(d / f"{name}.sol") == source, "the run's source is not the record's")
    need(
        m.get("sources") == [{"path": src, "sha256": source}],
        "the manifest's source is not the record's",
    )
    args, opts = m.get("arguments"), record.get("options")
    need(isinstance(opts, list), "the record has no options")
    canon = [
        src,
        *opts,
        "--emit=bin-runtime",
        f"-Zemit-lean-stages=Certified/{name}/gen",
    ]
    need(
        args == canon, f"the manifest's arguments are not the canonical {canon}: {args}"
    )
    spec = record.get("spec") or {}
    need(
        re.fullmatch(r"[0-9a-f]{64}", str(spec.get("sha256"))),
        "the record has no spec sha256",
    )

    # producer
    sol = record.get("solar") or {}
    need(sol.get("binary_sha256") == solar_sha, "the solar binary is not the record's")
    need(
        env_sha in (None, "", solar_sha), "the solar binary is not CERTIFY_SOLAR_SHA256"
    )
    need(
        sol.get("binary_commit") == driver_commit,
        "the producer's embedded commit is not the driver's",
    )
    need(
        sol.get("commit") == driver_commit,
        "the record's solar commit is not the driver's",
    )
    need(sol.get("dirty") is False, "the producer was built from a dirty checkout")

    # runtime
    hexcode = (d / "bytecode.hex").read_text().strip()
    digest = hashlib.sha256(bytes.fromhex(hexcode)).hexdigest()
    need(m.get("bytecode") == hexcode, "the manifest's bytecode is not bytecode.hex")
    need(
        m.get("bytecode_sha256") == digest,
        "the manifest's bytecode_sha256 is not bytecode.hex's",
    )
    need(
        record.get("bytecode_sha256") == digest,
        "the record's bytecode is not bytecode.hex",
    )
    need(
        fm.get("bytecode_sha256") == digest,
        "the published statement binds another bytecode",
    )
    need(
        fm.get("arguments") == canon and fm.get("sources") == [[src, source]],
        "the published statement binds another manifest",
    )

    # theorem
    need(
        final.get("theorem") == f"{ns}.final",
        "the record's theorem is not NAME.Final.final",
    )
    need(
        final.get("shape") == "SolidCore.Verify.Generic.Expected",
        "the theorem is not Generic.Expected",
    )
    need(record.get("caller") == "any", "the run is not on the any-caller route")
    stmt = final.get("statement") or ""
    need(
        f"SolarTV.Export.{contract}.Manifest.arguments" in stmt,
        "the statement does not bind the manifest",
    )
    proof = final.get("proof")
    need(isinstance(proof, str) and proof, "the final record names no proof constant")

    # replay
    need(
        receipt.get("theorem") == f"{ns}.final"
        and receipt.get("statement") == f"{ns}.Statement",
        "the replay receipt is for another theorem",
    )
    need(
        receipt.get("closure") == receipt.get("replayed_count")
        and receipt.get("closure", 0) > 0,
        "the replay does not cover the closure",
    )
    need(
        receipt.get("trusted_count") == 0 and receipt.get("trusted") in ({}, None),
        "the replay trusts constants",
    )
    need(receipt.get("unaccounted") == [], "the replay leaves constants unaccounted")
    need(
        set(receipt.get("axioms") or []) <= AXIOMS,
        "the closure uses an axiom outside the whitelist",
    )
    need(receipt.get("sorry_present") is False, "the closure holds sorry")
    names = [c[0] for c in receipt.get("constants") or []]
    need(
        len(names) == receipt["closure"], "the receipt's constants are not its closure"
    )
    mods = {
        f"{name}.gen.{contract}.Manifest": [
            n for n in names if n.startswith(f"SolarTV.Export.{contract}.Manifest.")
        ],
        f"{name}.Statement": [n for n in names if n == f"{ns}.Statement"],
        "proof": [n for n in names if n == proof.removeprefix("_root_.")],
        f"{name}.Final": [n for n in names if n == f"{ns}.final"],
    }
    for k, v in mods.items():
        need(v, f"the replayed closure has no constant of {k}")
    return {
        "schema": "solar certify bind 1",
        "name": name,
        "contract": contract,
        "inputs": {
            "source": {"path": src, "sha256": source},
            "spec": spec,
            "options": opts,
            "arguments": args,
        },
        "producer": {
            "binary_sha256": solar_sha,
            "embedded_commit": sol["binary_commit"],
            "driver_commit": driver_commit,
            "dirty": False,
            "version": sol.get("version"),
        },
        "runtime": {"bytecode_sha256": digest, "hex_sha256": sha(d / "bytecode.hex")},
        "theorem": {
            "name": f"{ns}.final",
            "statement": f"{ns}.Statement",
            "shape": final["shape"],
            "caller": "any",
        },
        "replay": {
            "closure": receipt["closure"],
            "replayed": receipt["replayed_count"],
            "trusted": 0,
            "unaccounted": [],
            "closure_sha256": receipt.get("closure_sha256"),
            "axioms": sorted(receipt.get("axioms") or []),
            "modules": {k: len(v) for k, v in mods.items()},
        },
    }


def main(argv=None):
    ap = argparse.ArgumentParser()
    for f in ("--record", "--dir", "--replay", "--solar", "--driver-root", "--out"):
        ap.add_argument(f, required=True)
    a = ap.parse_args(argv)
    try:
        b = build(
            json.loads(pathlib.Path(a.record).read_text()),
            a.dir,
            json.loads(pathlib.Path(a.replay).read_text()),
            sha(a.solar),
            head(a.driver_root),
            os.environ.get("CERTIFY_SOLAR_SHA256"),
        )
    except (Unbound, OSError, ValueError, KeyError, TypeError) as e:
        print(f"UNBOUND: {e}", file=sys.stderr)
        return 1
    pathlib.Path(a.out).write_text(json.dumps(b, indent=2) + "\n")
    print(f"bound {b['theorem']['name']}: {a.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
