"""Shared validation for standard-JSON compiler outputs and saved attempts."""

import json


def contract_outputs(output):
    if not isinstance(output, dict):
        raise ValueError("standard JSON output is not an object")
    errors = output.get("errors", [])
    if not isinstance(errors, list) or any(not isinstance(e, dict) for e in errors):
        raise ValueError("malformed standard JSON errors")
    failures = [e for e in errors if e.get("severity") == "error"]
    if failures:
        messages = [
            str(e.get("formattedMessage") or e.get("message") or e)
            for e in failures[:3]
        ]
        detail = "\n".join(messages)
        if len(failures) > 3:
            detail += f"\n... {len(failures) - 3} more errors in stdout.txt"
        raise ValueError("compiler error diagnostics:\n" + detail)
    contracts = output.get("contracts")
    if not isinstance(contracts, dict) or not contracts:
        raise ValueError("missing contract outputs")
    return contracts


def read_attempt(directory):
    result = json.loads((directory / "result.json").read_text(encoding="utf-8"))
    if (
        not isinstance(result, dict)
        or result.get("error")
        or result.get("failure")
        or result.get("returncode") != 0
    ):
        raise ValueError(f"failed compiler attempt: {directory}")
    request = json.loads((directory / "input.json").read_text(encoding="utf-8"))
    output = json.loads((directory / "stdout.txt").read_text(encoding="utf-8"))
    if not isinstance(request, dict):
        raise ValueError("standard JSON input is not an object")
    contract_outputs(output)
    return request, output
