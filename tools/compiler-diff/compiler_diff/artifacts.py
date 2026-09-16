"""Shared validation for standard-JSON compiler outputs and saved attempts."""

import json


def contract_outputs(output):
    if not isinstance(output, dict):
        raise ValueError("standard JSON output is not an object")
    errors = output.get("errors", [])
    if not isinstance(errors, list) or any(not isinstance(e, dict) for e in errors):
        raise ValueError("malformed standard JSON errors")
    if any(e.get("severity") == "error" for e in errors):
        raise ValueError("compiler error diagnostics")
    contracts = output.get("contracts")
    if not isinstance(contracts, dict) or not contracts:
        raise ValueError("missing contract outputs")
    return contracts


def read_attempt(directory):
    result = json.loads((directory / "result.json").read_text(encoding="utf-8"))
    if (
        not isinstance(result, dict)
        or result.get("error")
        or result.get("returncode") != 0
    ):
        raise ValueError(f"failed compiler attempt: {directory}")
    request = json.loads((directory / "input.json").read_text(encoding="utf-8"))
    output = json.loads((directory / "stdout.txt").read_text(encoding="utf-8"))
    if not isinstance(request, dict):
        raise ValueError("standard JSON input is not an object")
    contract_outputs(output)
    return request, output
