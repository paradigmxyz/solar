#!/usr/bin/env python3
"""producer_check.py PRODUCER_JSON CASE_DIR: checks that the case's record names the producer.

Exit 0 only when rec/record.json exists and its solar.binary_sha256 and solar.binary_commit equal
the producer's, and the driver was not given a stage cache (--probe-cache). Prints a JSON report
either way. A case whose run stopped before the driver wrote a record reports no_record."""
import json, pathlib, sys

prod = json.load(open(sys.argv[1]))
case = pathlib.Path(sys.argv[2])
rep = {"producer_sha256": prod["binary_sha256"], "producer_commit": prod["solar_commit"],
       "producer_path": prod["binary_path"], "build_command": prod["build_command"]}
cmd = (case / "command").read_text() if (case / "command").exists() else ""
rep["stage_cache"] = "--probe-cache" in cmd
recp = case / "rec" / "record.json"
if not recp.exists():
    rep["result"] = "no_record"
    print(json.dumps(rep, indent=2)); sys.exit(1)
rec = json.load(open(recp))
solar = rec.get("solar") or {}
rep["record_binary_sha256"] = solar.get("binary_sha256")
rep["record_binary_commit"] = solar.get("binary_commit")
rep["record_status"] = rec.get("status")
ok = (rep["record_binary_sha256"] == prod["binary_sha256"]
      and rep["record_binary_commit"] == prod["solar_commit"] and not rep["stage_cache"])
rep["result"] = "ok" if ok else "mismatch"
print(json.dumps(rep, indent=2))
sys.exit(0 if ok else 1)
