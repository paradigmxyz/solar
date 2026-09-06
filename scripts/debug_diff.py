#!/usr/bin/env python3
"""Compile and execute debug-info differentials through the local soldb runner."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
ADDRESS = "0x5fbdb2315678afecb367f032d93f642f64180aa3"


def run(command, *, input_text=None, allowed=(0,)):
    result = subprocess.run(
        [str(arg) for arg in command], input=input_text, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=180, cwd=ROOT,
    )
    if result.returncode not in allowed:
        raise RuntimeError(
            f"command failed ({result.returncode}): {' '.join(map(str, command))}\n"
            f"{result.stdout}\n{result.stderr}"
        )
    return result


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n")


def compile_source(binary, source, mode, ethdebug, directory):
    selections = ["abi", "evm.bytecode.object", "evm.deployedBytecode.object",
                  "evm.bytecode.sourceMap", "evm.deployedBytecode.sourceMap"]
    if ethdebug:
        selections += ["evm.bytecode.ethdebug", "evm.deployedBytecode.ethdebug", "ethdebug.resources"]
    request = {
        "language": "Solidity",
        "sources": {source.name: {"content": source.read_bytes().decode("utf-8")}},
        "settings": {
            "evmVersion": "cancun",
            "optimizer": {"enabled": mode != "none", "runs": 1 if mode == "size" else 200},
            "outputSelection": {"*": {"*": selections}},
        },
    }
    write_json(directory / "input.json", request)
    response = run([binary, "--standard-json"], input_text=json.dumps(request))
    # solc can prefix JSON with an informational SMT message.
    output = json.loads(response.stdout[response.stdout.index("{"):])
    write_json(directory / "output.json", output)
    errors = [error for error in output.get("errors", []) if error.get("severity") == "error"]
    if errors:
        raise RuntimeError("\n".join(error.get("formattedMessage", error["message"]) for error in errors))
    return output


def write_artifacts(output, source, contract, directory, ethdebug):
    directory.mkdir(parents=True, exist_ok=True)
    artifact = output["contracts"][source.name][contract]
    evm = artifact["evm"]
    (directory / source.name).write_bytes(source.read_bytes())
    (directory / f"{contract}.bin").write_text(evm["bytecode"]["object"] + "\n")
    write_json(directory / f"{contract}.abi", artifact["abi"])
    if ethdebug:
        write_json(directory / "ethdebug_resources.json", output["ethdebug"]["resources"])
        write_json(directory / f"{contract}_ethdebug.json", evm["bytecode"]["ethdebug"])
        write_json(directory / f"{contract}_ethdebug-runtime.json", evm["deployedBytecode"]["ethdebug"])
    else:
        source_list = sorted(output["sources"], key=lambda name: output["sources"][name]["id"])
        if any(output["sources"][name]["id"] != index for index, name in enumerate(source_list)):
            raise RuntimeError("source IDs must be dense for the legacy combined JSON adapter")
        write_json(directory / "combined.json", {
            "sourceList": source_list,
            "contracts": {f"{source.name}:{contract}": {
                "bin": evm["bytecode"]["object"],
                "bin-runtime": evm["deployedBytecode"]["object"],
                "srcmap": evm["bytecode"]["sourceMap"],
                "srcmap-runtime": evm["deployedBytecode"]["sourceMap"],
            }},
        })


def execute(soldb, artifacts, contract, case, path):
    command = [soldb, "run", artifacts / f"{contract}.bin", "--save-trace", path, "--json"]
    if case.get("deploy"):
        command += ["--deploy"]
    else:
        command += [case["signature"], *case.get("args", [])]
    run(command)
    trace = json.loads(path.read_text())
    if trace["success"] != case.get("success", True):
        raise RuntimeError(f"unexpected execution status for {case['name']}")
    if "output" in case and trace["output"].removeprefix("0x") != case["output"].removeprefix("0x"):
        raise RuntimeError(f"unexpected return data for {case['name']}: {trace['output']}")


def compare(soldb, left, right, contract, reference, candidate, case, mode, checkpoints=None):
    command = [
        soldb, "debug-diff", "--reference-trace-file", reference,
        "--candidate-trace-file", candidate,
        "--reference-ethdebug-dir", f"{ADDRESS}:{contract}:{left}",
        "--candidate-ethdebug-dir", f"{ADDRESS}:{contract}:{right}",
        "--mode", mode, "--json",
    ]
    if case.get("deploy"):
        command += ["--reference-constructor-args", "0x", "--candidate-constructor-args", "0x"]
    if checkpoints is not None:
        command += ["--checkpoints-file", checkpoints]
    result = run(command, allowed=(0, 2))
    report = json.loads(result.stdout)
    if "equivalent" not in report or (result.returncode == 0) != report["equivalent"]:
        raise RuntimeError(f"invalid debug-diff report: {result.stdout}")
    return report


def checkpoint_lines(source):
    checkpoints = {}
    for line, text in enumerate(source.read_text().splitlines(), 1):
        if "// debug-check:" in text:
            name = text.split("// debug-check:", 1)[1].strip()
            if not name or name in checkpoints:
                raise ValueError(f"invalid or duplicate checkpoint at {source}:{line}")
            checkpoints[name] = {"source": source.name, "line": line}
    return checkpoints


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--solar", default=str(ROOT / "target/debug/solar"))
    parser.add_argument("--solc", default=os.environ.get("SOLC", "solc"))
    parser.add_argument("--soldb", default=os.environ.get("SOLDB", "soldb"))
    parser.add_argument("--output", type=Path, default=ROOT / "target/debug-diff")
    parser.add_argument("--suite", type=Path, default=ROOT / "tests/debug-diff/cases.json")
    parser.add_argument("--optimizations", nargs="+", choices=["none", "gas", "size"], default=["none", "gas", "size"])
    parser.add_argument("--comparison", choices=["formats", "solc", "all"], default="all")
    parser.add_argument("--mode", choices=["steps", "spans", "coverage"], default="coverage")
    parser.add_argument("--all-stops", action="store_true", help="compare all solc source stops, including declarations")
    args = parser.parse_args()
    args.output = args.output.resolve()
    suite = json.loads(args.suite.read_text())
    report = {"versions": {}, "results": [], "configuration": {
        "comparison": args.comparison, "mode": args.mode, "allStops": args.all_stops,
        "optimizations": args.optimizations,
    }}
    try:
        binaries = [("solar", args.solar), ("soldb", args.soldb)]
        if args.comparison != "formats":
            binaries.append(("solc", args.solc))
        for name, binary in binaries:
            report["versions"][name] = run([binary, "--version"]).stdout.strip()
        for fixture in suite:
            source = args.suite.parent / fixture["source"]
            contract = fixture["contract"]
            checkpoints = checkpoint_lines(source)
            for optimization in args.optimizations:
                directory = args.output / source.stem / optimization
                solar = compile_source(args.solar, source, optimization, True, directory / "solar")
                write_artifacts(solar, source, contract, directory / "ethdebug", True)
                write_artifacts(solar, source, contract, directory / "source-maps", False)
                if args.comparison != "formats":
                    solc = compile_source(args.solc, source, optimization, False, directory / "solc")
                    write_artifacts(solc, source, contract, directory / "solc", False)
                for case in fixture["cases"]:
                    candidate = directory / f"{case['name']}.solar.trace.json"
                    execute(args.soldb, directory / "ethdebug", contract, case, candidate)
                    pairs = []
                    if args.comparison != "solc":
                        pairs.append(("formats", directory / "source-maps", candidate))
                    if args.comparison != "formats":
                        reference = directory / f"{case['name']}.solc.trace.json"
                        execute(args.soldb, directory / "solc", contract, case, reference)
                        pairs.append(("solc", directory / "solc", reference))
                    for name, left, reference in pairs:
                        checkpoint_file = None
                        if name == "solc" and not args.all_stops:
                            checkpoint_file = directory / f"{case['name']}.checkpoints.json"
                            expected = case.get("checkpoints", [case["name"]])
                            if not expected:
                                raise ValueError(f"no checkpoints for {case['name']}")
                            write_json(checkpoint_file, [checkpoints[key] for key in expected])
                        diff = compare(args.soldb, left, directory / "ethdebug", contract,
                                       reference, candidate, case, args.mode, checkpoint_file)
                        identifier = f"{source.stem}/{optimization}/{case['name']}/{name}"
                        write_json(directory / f"{case['name']}.{name}.diff.json", diff)
                        report["results"].append({"id": identifier, **diff})
                        print(f"{'PASS' if diff['equivalent'] else 'FAIL'} {identifier}", flush=True)
    except (RuntimeError, ValueError, KeyError, OSError, subprocess.TimeoutExpired) as error:
        report["error"] = str(error)
        print(str(error), file=sys.stderr)
    write_json(args.output / "report.json", report)
    print(f"Reports: {args.output / 'report.json'}")
    return int("error" in report or not report["results"] or any(not result["equivalent"] for result in report["results"]))


if __name__ == "__main__":
    sys.exit(main())
