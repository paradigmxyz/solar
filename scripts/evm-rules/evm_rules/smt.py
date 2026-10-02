"""Verify rule files with Z3, cvc5 fallbacks and exportable SMT-LIB queries.

The readers in `isle.py`, `late.py` and `stack.py` build solver-independent
obligations; this module gives them their Z3 meaning and records the queries.
"""

import hashlib
from typing import Any

from .expr import Expr, Unsupported
from .isle import Context, Rule, forms
from .late import CONTRACTS as LATE_CONTRACTS
from .late import late_obligation, late_rules
from .semantics import Model, check, partition_bits, partition_shift
from .stack import CONTRACTS as STACK_CONTRACTS
from .stack import stack_rules, stack_variants


def verify_file(
    path,
    timeout_ms,
    artifacts=None,
    partition_shifts=False,
    fallback=None,
    bit_partition_timeout_ms=0,
    index_partition_timeout_ms=0,
    bit_partition_jobs=1,
    shard_index=0,
    shard_count=1,
):
    source = path.read_text()
    rules = [
        Rule(form, line, str(path)) for form, line in forms(source) if form[0] == "rule"
    ]
    if not rules:
        raise ValueError(f"no rules in {path}")
    if not 0 <= shard_index < shard_count <= len(rules):
        raise ValueError(
            "shards must be nonempty and satisfy 0 <= index < count <= rules"
        )
    results = []
    for rule in rules[shard_index::shard_count]:
        context = Context()
        model = Model()
        result: dict[str, Any]
        query = ""
        partitions = []
        try:
            lhs, rhs = context.obligation(rule)
            assumptions = [model.condition(c) for c in context.assumptions]
            result, query = check(lhs, rhs, assumptions, timeout_ms, model)
            if constants := result.get("constant_specializations"):
                model = Model(
                    {name: int(value, 16) for name, value in constants.items()}
                )
            if query and (
                result["status"] == "unknown"
                or partition_shifts
                and result["status"] == "proved"
            ):
                partitioned, partitions = partition_shift(
                    lhs,
                    rhs,
                    assumptions,
                    index_partition_timeout_ms or timeout_ms,
                    model,
                )
                if partitions:
                    if constants:
                        partitioned["constant_specializations"] = constants
                    result = partitioned
            if query and result["status"] == "unknown" and fallback is not None:
                # Prove the complete original obligation. A successful fallback
                # replaces partial partitions, never promotes their proved prefix.
                attempt = fallback.solve(query)
                if attempt["status"] == "unsat":
                    result = {
                        "status": "proved",
                        "proof_method": "solver-fallback",
                        "fallback": attempt,
                    }
                    if constants:
                        result["constant_specializations"] = constants
                    partitions = []
                else:
                    result["fallback"] = attempt
                    if attempt["status"] == "sat":
                        result["reason"] = (
                            "cvc5 reported SAT; no independently replayed counterexample"
                        )
                    else:
                        reason = result.get("reason", "Z3 verification incomplete")
                        result["reason"] = (
                            f"{reason}; cvc5 fallback returned {attempt['status']}"
                        )
                    if partitions:
                        partitions.append(("word", query))
            if (
                query
                and result["status"] == "unknown"
                and bit_partition_timeout_ms > 0
                and result.get("fallback", {}).get("status", "unknown")
                in ("unknown", "timeout")
            ):
                # All output bits must agree under the complete original guards.
                # Do not hide a fallback solver's SAT result or process failure.
                previous_fallback = result.get("fallback")
                result, partitions = partition_bits(
                    lhs,
                    rhs,
                    assumptions,
                    bit_partition_timeout_ms,
                    model,
                    bit_partition_jobs,
                )
                if constants:
                    result["constant_specializations"] = constants
                if previous_fallback is not None:
                    result["fallback"] = previous_fallback
                if result["status"] != "proved":
                    partitions.append(("word", query))
        except Unsupported as error:
            result = {"status": "unsupported", "reason": str(error)}
        result.update(
            line=rule.line, rule_sha256=rule.digest, contracts=sorted(context.contracts)
        )
        if query and artifacts is not None:
            artifacts.mkdir(parents=True, exist_ok=True)
            paths = []
            for suffix, text in partitions or [("word", query)]:
                query_path = (
                    artifacts
                    / f"{path.stem}-{rule.line}-{rule.digest[:12]}-{suffix}.smt2"
                )
                query_path.write_text(text)
                paths.append(str(query_path))
            result["smt2"] = paths
        results.append(result)
    return {
        "source": str(path),
        "source_sha256": hashlib.sha256(source.encode()).hexdigest(),
        "shard": {
            "index": shard_index,
            "count": shard_count,
            "total_rules": len(rules),
        },
        "rules": results,
    }


def verify_late_file(path, timeout_ms=5000, artifacts=None):
    source, rules = late_rules(path)
    results = []
    for rule in rules:
        result = {"line": rule.line, "rule_sha256": rule.digest}
        results.append(result)
        try:
            details, lhs, rhs = late_obligation(rule)
            model = Model()
            proof, query = check(lhs, rhs, timeout_ms=timeout_ms, model=model)
            partitions = []
            if proof["status"] == "unknown" and query:
                partitioned, partitions = partition_shift(
                    lhs, rhs, [], timeout_ms, model
                )
                if partitions:
                    proof = partitioned
            result.update(details)
            result.update(proof)
            if artifacts is not None and query:
                artifacts.mkdir(parents=True, exist_ok=True)
                paths = []
                for suffix, text in partitions or [("word", query)]:
                    output = (
                        artifacts
                        / f"{path.stem}-{rule.line}-{rule.digest[:12]}-{suffix}.smt2"
                    )
                    output.write_text(text)
                    paths.append(str(output))
                result["smt2"] = paths
        except (Unsupported, ValueError, TypeError, IndexError) as error:
            result.update(status="unsupported", reason=str(error))
    return {
        "source": str(path),
        "source_sha256": hashlib.sha256(source.encode()).hexdigest(),
        "rules": results,
        "contracts": LATE_CONTRACTS,
    }


def verify_stack_file(path, timeout_ms=5000, artifacts=None):
    source, rules = stack_rules(path)
    results = []
    for rule in rules:
        result: dict[str, Any] = {
            "line": rule.line,
            "sha256": rule.digest,
            "status": "unsupported",
            "variants": [],
        }
        results.append(result)
        try:
            for variant in stack_variants(rule):
                proof, query = check(
                    variant["difference"], Expr.const(0), timeout_ms=timeout_ms
                )
                proof.update(
                    bindings=variant["bindings"],
                    minimum_stack=variant["minimum_stack"],
                    peak_growth=variant["peak_growth"],
                )
                if artifacts is not None and query:
                    artifacts.mkdir(parents=True, exist_ok=True)
                    output = (
                        artifacts
                        / f"{path.stem}-{rule.line}-{len(result['variants'])}.smt2"
                    )
                    output.write_text(query)
                    proof["query"] = str(output)
                result["variants"].append(proof)
            result["status"] = next(
                (p["status"] for p in result["variants"] if p["status"] != "proved"),
                "proved",
            )
        except (Unsupported, ValueError, TypeError, IndexError) as error:
            result.update(status="unsupported", reason=str(error))
    return {
        "source": str(path),
        "sha256": hashlib.sha256(source.encode()).hexdigest(),
        "rules": results,
        "contracts": STACK_CONTRACTS,
    }
