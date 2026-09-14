"""Shared extraction helpers for benchmark workflow contract tests."""

import re


def extract_job(text: str, name: str, label: str = "job") -> str:
    jobs = text.split("\njobs:\n", 1)[1]
    match = re.search(
        rf"^  {re.escape(name)}:\n(?P<body>.*?)(?=^  [A-Za-z0-9_-]+:\n|\Z)",
        jobs,
        re.MULTILINE | re.DOTALL,
    )
    if match is None:
        raise AssertionError(f"{label} {name!r} is missing")
    return match.group(0)


def step_block(job: str, name: str) -> str:
    marker = f"      - name: {name}\n"
    if marker not in job:
        raise AssertionError(f"step {name!r} is missing")
    remainder = job.split(marker, 1)[1]
    next_step = remainder.find("\n      - ")
    if next_step >= 0:
        remainder = remainder[:next_step]
    return marker + remainder


def github_script(step: str) -> str:
    script = step.split("          script: |\n", 1)[1]
    if not all(
        not line or line.startswith("            ") for line in script.splitlines()
    ):
        raise AssertionError("github-script block has unexpected indentation")
    return "\n".join(line[12:] for line in script.splitlines())
