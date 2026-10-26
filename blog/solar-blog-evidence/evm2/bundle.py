"""Turn a runtime-bench result and its artifacts into an `evm2 bundle` JSON file.

Usage: bundle.py RESULTS.json ARTIFACTS_DIR VARIANT OUT.json COMPILER_ID
"""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "benches" / "runtime"))
import benchmark  # noqa: E402

SKIP = {"tempo-zones-portal"}

results, art, variant, out, compiler = sys.argv[1:6]
tests = {t.test_id: t for t in benchmark.select_tests(["runtime"], "all")}
bundle = []
for r in json.load(open(results))["results"]:
    tid = r["test_id"]
    c = r["compilers"].get(compiler)
    if tid in SKIP or not c or c.get("status") != "ok" or c.get("deploy_status") != "ok":
        continue
    d = Path(art) / tid / compiler
    creation = (d / "creation.hex").read_text().strip().removeprefix("0x")
    runtime = (d / "runtime.hex").read_text().strip().removeprefix("0x")
    t = tests[tid]
    args = benchmark.abi_encode_constructor(t.constructor_args, t.constructor_sig)
    assert args is not None, tid
    calls = []
    for g in c.get("gas_results") or []:
        if g.get("gas") is None or g.get("comparison_exclusion_reason"):
            continue
        data, err = benchmark.encode_calldata(g["call"], g["args"])
        assert data is not None, (tid, err)
        calls.append({"label": g["label"], "calldata": data, "gas": g["gas"]})
    if not calls:
        continue
    bundle.append(
        {
            "test_id": tid,
            "variant": variant,
            "contract": r["contract_name"],
            "runtime_size": c.get("runtime_size"),
            "deploy_gas": c.get("deploy_gas"),
            "creation": "0x" + creation + args,
            "runtime": "0x" + runtime,
            "calls": calls,
        }
    )
json.dump(bundle, open(out, "w"))
print(f"{out}: {len(bundle)} cases, {sum(len(b['calls']) for b in bundle)} calls")
