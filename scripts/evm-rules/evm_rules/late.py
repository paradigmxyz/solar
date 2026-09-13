"""Check late word windows under closed-count and protected-input contracts.

The count is an arbitrary word independent of the changed shift base. Its
computation stays in order. A closed body reads no incoming stack items; the
protected variant may move its base, but cannot copy, inspect, or consume it.
The modeled surrounding instructions cannot increase peak requirements.
Rust extraction and edit application remain trusted.
"""

import hashlib

from .isle import Rule, forms
from .semantics import Expr, Model, Unsupported, check, partition_shift


def execute(instructions):
    stack = []
    peak = 0
    for name, argument in instructions:
        if name == "push":
            stack.append(Expr.const(argument))
        elif name == "count":
            stack.append(Expr.var("n"))
        elif name == "dup":
            if not 1 <= argument <= len(stack):
                raise Unsupported("window reads its incoming stack")
            stack.append(stack[-argument])
        elif name == "swap":
            if not 1 <= argument < len(stack):
                raise Unsupported("window reads its incoming stack")
            stack[-1], stack[-argument - 1] = stack[-argument - 1], stack[-1]
        elif name in ("shl", "shr", "sub", "add"):
            if len(stack) < 2:
                raise Unsupported("window reads its incoming stack")
            lhs, rhs = stack.pop(), stack.pop()
            stack.append(Expr(name, (lhs, rhs)))
        elif name == "not" and stack:
            stack[-1] = Expr("not", (stack[-1],))
        else:
            raise Unsupported(f"unmodeled late opcode: {name}")
        peak = max(peak, len(stack))
    return stack, peak


def verify_late_file(path, timeout_ms=5000, artifacts=None):
    source = path.read_text()
    results = []
    for form, line in forms(source):
        if form[0] != "rule":
            raise Unsupported("late word proof files may only contain rules")
        rule = Rule(form, line, str(path))
        result = dict(line=line, rule_sha256=rule.digest)
        results.append(result)
        try:
            body = form[2:] if isinstance(form[1], str) and form[1].isdecimal() else form[1:]
            root, *guards, edit = body
            if root[0] != "late_peep" or len(root) != 2:
                raise Unsupported("unmodeled late rule root")
            bind, window_name, window = root[1]
            if bind != "and" or window[0] not in ("late_window", "protected_window"):
                raise Unsupported("unmodeled late window")
            first = window[1]
            if first[0] != "push" or len(first) != 2 or not first[1].isidentifier():
                raise Unsupported("late window must bind a literal push")
            def opcode(pattern):
                if len(pattern) != 2 or pattern[0] != "opcode":
                    raise Unsupported("unmodeled late opcode facet")
                return pattern[1].removeprefix("$").lower(), None
            expected_guards = [("if-let", "true", ("u256_is_one", first[1]))]
            if window[0] == "late_window":
                first, second, penultimate, last = window[1:]
                expected_guards += [("if-let", "true", ("closed_count", window_name)),
                                    ("if-let", "true", ("low_mask_profitable",))]
                expected_edit = "Edit.LowMask"
                if second[0] != "dup" or len(second) != 2 or not second[1].isdecimal():
                    raise Unsupported("unmodeled late stack instruction")
                before = [("push", 1), ("dup", int(second[1])), ("count", None),
                          opcode(penultimate), opcode(last)]
                # Edit.LowMask changes only the prefix pair and final opcode.
                after = [("push", 0), ("not", None), *before[2:-1], ("not", None)]
                result.update(minimum_stack=0, count_initial_height_before=2, count_initial_height_after=1)
            else:
                first, shift, decrement, swap, last = window[1:]
                if decrement[0] != "push" or len(decrement) != 2 or not decrement[1].isidentifier():
                    raise Unsupported("unmodeled decrement push")
                if swap[0] != "swap" or len(swap) != 2 or not swap[1].isdecimal():
                    raise Unsupported("unmodeled late swap")
                expected_guards += [("if-let", "true", ("u256_is_one", decrement[1])),
                                    ("if-let", "true", ("protected_count", window_name)),
                                    ("if-let", "true", ("protected_mask_profitable",))]
                expected_edit = "Edit.LowMaskWithSwap"
                before = [("push", 1), ("count", None), opcode(shift),
                          ("push", 1), ("swap", int(swap[1])), opcode(last)]
                # The body is unchanged; only its protected input changes value.
                after = [("push", 0), ("not", None), *before[1:-3], ("not", None)]
                result.update(incoming_stack_requirements="unchanged under protected_count contract",
                              count_initial_height_before=1, count_initial_height_after=1)
            if guards != expected_guards:
                raise Unsupported("unmodeled or missing late window guard")
            if edit != ("rewrite", ("late_length", window_name), (expected_edit,)):
                raise Unsupported("unmodeled late window edit or extent")
            lhs, old_peak = execute(before)
            rhs, new_peak = execute(after)
            if len(lhs) != 1 or len(rhs) != 1 or new_peak > old_peak:
                raise Unsupported("late edit changes final stack height or raises peak")
            model = Model()
            proof, query = check(lhs[0], rhs[0], timeout_ms=timeout_ms, model=model)
            partitions = []
            if proof["status"] == "unknown" and query:
                partitioned, partitions = partition_shift(lhs[0], rhs[0], [], timeout_ms, model)
                if partitions:
                    proof = partitioned
            result.update(proof, summary_before_peak=old_peak, summary_after_peak=new_peak)
            if artifacts is not None and query:
                artifacts.mkdir(parents=True, exist_ok=True)
                paths = []
                for suffix, text in partitions or [("word", query)]:
                    output = artifacts / f"{path.stem}-{line}-{rule.digest[:12]}-{suffix}.smt2"
                    output.write_text(text)
                    paths.append(str(output))
                result["smt2"] = paths
        except (Unsupported, ValueError, TypeError, IndexError) as error:
            result.update(status="unsupported", reason=str(error))
    if not results:
        raise ValueError("late word proof file contains no rules")
    return dict(source=str(path), source_sha256=hashlib.sha256(source.encode()).hexdigest(),
                rules=results, contracts=[
                    "closed_count has no incoming-stack dependencies and leaves one arbitrary word",
                    "closed_count has unchanged instructions, order, and external reads; no gas/PC observation",
                    "late_window exposes two prefix and two suffix instructions around closed_count",
                    "protected_count moves but never duplicates, consumes, or inspects the unique protected input",
                    "protected_count leaves that input below the count; all other stack results are independent of it",
                    "Edit.LowMask replaces the prefix with PUSH0 NOT and the final opcode with NOT",
                    "Edit.LowMaskWithSwap replaces the first push with PUSH0 NOT and the last three instructions with NOT",
                    "sufficient gas and stack; untouched deeper prefix; target profitability and legality"])
