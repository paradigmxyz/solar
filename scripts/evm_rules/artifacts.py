"""Collect and fingerprint every query in a word/stack verification report."""

import hashlib
from pathlib import Path


def query_paths(report, *, require_proved=False):
    paths = []
    rule_count = 0
    for file in report.get("files", []):
        rules = file.get("rules", [])
        if require_proved and not rules:
            raise ValueError("proof source has no rules")
        for rule in rules:
            rule_count += 1
            if require_proved and rule.get("status") != "proved":
                raise ValueError(f"rule at {file.get('source')}:{rule.get('line')} is not proved")
            if "variants" in rule:
                variants = rule["variants"]
                if require_proved and not variants:
                    raise ValueError("stack rule has no variants")
                for variant in variants:
                    if require_proved and variant.get("status") != "proved":
                        raise ValueError("stack variant is not proved")
                    path = variant.get("query")
                    if path:
                        paths.append(path)
                    elif require_proved:
                        raise ValueError("stack variant has no saved query")
            else:
                queries = rule.get("smt2", [])
                if require_proved and not queries:
                    raise ValueError("word rule has no saved queries")
                if require_proved and rule.get("proof_method") == "exhaustive-shift-partition":
                    if len(queries) != rule.get("cases", 0) + 1:
                        raise ValueError("shift proof is missing a partition or its coverage query")
                paths.extend(queries)
    if require_proved and not rule_count:
        raise ValueError("proof report has no rules")
    if any(not isinstance(path, str) or not path for path in paths):
        raise ValueError("query paths must be nonempty strings")
    if len(set(paths)) != len(paths):
        raise ValueError("proof report reuses a query path")
    return paths


def query_manifest(report):
    return {path: hashlib.sha256(Path(path).read_bytes()).hexdigest()
            for path in query_paths(report)}
