"""Check late word windows under closed-count and protected-input contracts.

The count is an arbitrary word independent of the changed shift base. Its
computation stays in order. A closed body reads no incoming stack items; the
protected variant may move its base, but cannot copy, inspect, or consume it.
The modeled surrounding instructions cannot increase peak requirements.
Rust extraction and edit application remain trusted.
"""

from .expr import Expr, Unsupported
from .isle import Rule, forms


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


# Trusted contracts of every late window rule; reports list them with the proofs.
CONTRACTS = [
    "closed_count has no incoming-stack dependencies and leaves one arbitrary word",
    "closed_count has unchanged instructions, order, and external reads; no gas/PC observation",
    "late_window exposes two prefix and two suffix instructions around closed_count",
    "protected_count moves but never duplicates, consumes, or inspects the unique protected input",
    "protected_count leaves that input below the count; all other stack results are independent of it",
    "Edit.LowMask replaces the prefix with PUSH0 NOT and the final opcode with NOT",
    "Edit.LowMaskWithSwap replaces the first push with PUSH0 NOT and the last three instructions with NOT",
    "sufficient gas and stack; untouched deeper prefix; target profitability and legality",
]


def late_rules(path):
    """Read the rules of a late word proof file, failing on any other form."""
    source = path.read_text()
    rules = []
    for form, line in forms(source):
        if form[0] != "rule":
            raise Unsupported("late word proof files may only contain rules")
        rules.append(Rule(form, line, str(path)))
    if not rules:
        raise ValueError("late word proof file contains no rules")
    return source, rules


def late_obligation(rule):
    """Return a rule's stack details and its single-word before/after results."""
    form = rule.form
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

    expected_guards: list[tuple[str, str, tuple[str, ...]]] = [
        ("if-let", "true", ("u256_is_one", first[1]))
    ]
    if window[0] == "late_window":
        first, second, penultimate, last = window[1:]
        expected_guards += [
            ("if-let", "true", ("closed_count", window_name)),
            ("if-let", "true", ("low_mask_profitable",)),
        ]
        expected_edit = "Edit.LowMask"
        if second[0] != "dup" or len(second) != 2 or not second[1].isdecimal():
            raise Unsupported("unmodeled late stack instruction")
        before = [
            ("push", 1),
            ("dup", int(second[1])),
            ("count", None),
            opcode(penultimate),
            opcode(last),
        ]
        # Edit.LowMask changes only the prefix pair and final opcode.
        after = [("push", 0), ("not", None), *before[2:-1], ("not", None)]
        details = {
            "minimum_stack": 0,
            "count_initial_height_before": 2,
            "count_initial_height_after": 1,
        }
    else:
        first, shift, decrement, swap, last = window[1:]
        if (
            decrement[0] != "push"
            or len(decrement) != 2
            or not decrement[1].isidentifier()
        ):
            raise Unsupported("unmodeled decrement push")
        if swap[0] != "swap" or len(swap) != 2 or not swap[1].isdecimal():
            raise Unsupported("unmodeled late swap")
        expected_guards += [
            ("if-let", "true", ("u256_is_one", decrement[1])),
            ("if-let", "true", ("protected_count", window_name)),
            ("if-let", "true", ("protected_mask_profitable",)),
        ]
        expected_edit = "Edit.LowMaskWithSwap"
        before = [
            ("push", 1),
            ("count", None),
            opcode(shift),
            ("push", 1),
            ("swap", int(swap[1])),
            opcode(last),
        ]
        # The body is unchanged; only its protected input changes value.
        after = [("push", 0), ("not", None), *before[1:-3], ("not", None)]
        details = {
            "incoming_stack_requirements": "unchanged under protected_count contract",
            "count_initial_height_before": 1,
            "count_initial_height_after": 1,
        }
    if guards != expected_guards:
        raise Unsupported("unmodeled or missing late window guard")
    if edit != ("rewrite", ("late_length", window_name), (expected_edit,)):
        raise Unsupported("unmodeled late window edit or extent")
    lhs, old_peak = execute(before)
    rhs, new_peak = execute(after)
    if len(lhs) != 1 or len(rhs) != 1 or new_peak > old_peak:
        raise Unsupported("late edit changes final stack height or raises peak")
    details.update(summary_before_peak=old_peak, summary_after_peak=new_peak)
    return details, lhs[0], rhs[0]
