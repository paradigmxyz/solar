"""Prove the compiled pure stack-rule subset for every legal physical depth.

The model compares the entire touched stack and checks input and peak-depth
requirements. It assumes well-formed input code with sufficient stack and gas;
it does not preserve out-of-gas or underflow behavior of malformed code. An
arbitrary deeper prefix is untouched by either sequence. Rust extraction, edit
application and opcode lowering remain explicit trusted contracts.
"""

from itertools import product
import hashlib

from .isle import Rule, forms
from .semantics import Expr, Unsupported, check


def instruction(pattern, bindings):
    name, *args = pattern
    if name in ("dup", "swap", "exchange"):
        depths = tuple(bindings[arg] if not arg.isdecimal() else int(arg) for arg in args)
        if len(depths) != (2 if name == "exchange" else 1) or any(not 1 <= depth <= 235 for depth in depths):
            raise Unsupported("invalid physical depth")
        return name, depths
    if name == "pop" and not args:
        return "pop", ()
    if name == "opcode" and len(args) == 1:
        opcode = args[0].removeprefix("$").lower()
        if opcode in ("not", "iszero", "pop"):
            return opcode, ()
        for kind in ("dup", "swap"):
            if opcode.startswith(kind) and opcode[len(kind):].isdecimal():
                depth = int(opcode[len(kind):])
                if 1 <= depth <= 16:
                    return kind, (depth,)
    raise Unsupported(f"unmodeled physical instruction: {pattern}")


def requirements(sequence):
    minimum = height = peak = 0
    for name, args in sequence:
        required = {"pop": 1, "not": 1, "iszero": 1}.get(name)
        if name == "dup":
            required = args[0]
        elif name in ("swap", "exchange"):
            required = max(args) + 1
        if required is None:
            raise Unsupported(f"unmodeled stack operation: {name}")
        minimum = max(minimum, required - height)
        height += (name == "dup") - (name == "pop")
        peak = max(peak, height)
    return minimum, peak, height


def execute(sequence, inputs):
    stack = list(inputs)
    for name, args in sequence:
        if name == "dup":
            stack.append(stack[-args[0]])
        elif name == "swap":
            index = -args[0] - 1
            stack[-1], stack[index] = stack[index], stack[-1]
        elif name == "exchange":
            a, b = (-index - 1 for index in args)
            stack[a], stack[b] = stack[b], stack[a]
        elif name == "pop":
            stack.pop()
        else:
            stack[-1] = Expr(name, (stack[-1],))
    return stack


def verify_stack_file(path, timeout_ms=5000, artifacts=None):
    source = path.read_text()
    rules = []
    for form, line in forms(source):
        if form[0] != "rule":
            raise Unsupported("stack proof files may only contain rules")
        rule = Rule(form, line, str(path))
        result = dict(line=line, sha256=rule.digest, status="unsupported", variants=[])
        rules.append(result)
        try:
            body = form[2:] if isinstance(form[1], str) and form[1].isdecimal() else form[1:]
            if len(body) != 2:
                raise Unsupported("unmodeled guard or stack rule shape")
            (root, window), (rewrite, skip, edit) = body
            name, *patterns = window
            patterns = [tuple(f"wildcard_{i}_{j}" if arg == "_" else arg
                              for j, arg in enumerate(pattern)) for i, pattern in enumerate(patterns)]
            if root != "peep_nonpush" or name != f"last{len(patterns)}" or rewrite != "rewrite" or int(skip) != len(patterns):
                raise Unsupported("unmodeled window or rewrite extent")
            variables = sorted({arg for pattern in patterns if pattern[0] in ("dup", "swap", "exchange")
                                for arg in pattern[1:] if not arg.isdecimal()})
            if len(variables) > 2:
                raise Unsupported("stack rule has more than two depth bindings")
            # DUPN/SWAPN encode depths 1..235. EXCHANGE admits n < m,
            # n + m <= 30; classic forks lower its subset to three swaps.
            for values in product(range(1, 236), repeat=len(variables)):
                bindings = dict(zip(variables, values))
                before = [instruction(pattern, bindings) for pattern in patterns]
                if any(name == "exchange" and not (1 <= args[0] < args[1] and sum(args) <= 30)
                       for name, args in before):
                    continue
                if edit[0] == "Edit.Keep" and len(edit) == 2 and edit[1].isdecimal():
                    keep = int(edit[1])
                    if keep > len(before):
                        raise Unsupported("edit retains instructions outside the window")
                    after = before[:keep]
                elif edit[0] == "Edit.OverwriteOne" and len(edit) == 2:
                    after = [instruction(("opcode", edit[1]), bindings)]
                else:
                    raise Unsupported(f"unmodeled stack edit: {edit}")
                needed, peak, delta = requirements(before)
                new_needed, new_peak, new_delta = requirements(after)
                if new_needed > needed or new_peak > peak or new_delta != delta:
                    raise Unsupported("replacement increases stack requirements or changes height")
                inputs = [Expr.var(f"s{i}") for i in range(needed)]
                lhs, rhs = execute(before, inputs), execute(after, inputs)
                difference = Expr.const(0)
                for a, b in zip(lhs, rhs):
                    if a != b:
                        difference = Expr("or", (difference, Expr("xor", (a, b))))
                proof, query = check(difference, Expr.const(0), timeout_ms=timeout_ms)
                proof.update(bindings=bindings, minimum_stack=needed, peak_growth=peak)
                if artifacts is not None and query:
                    artifacts.mkdir(parents=True, exist_ok=True)
                    output = artifacts / f"{path.stem}-{line}-{len(result['variants'])}.smt2"
                    output.write_text(query)
                    proof["query"] = str(output)
                result["variants"].append(proof)
            if not result["variants"]:
                raise Unsupported("no applicable physical stack bindings")
            result["status"] = next((p["status"] for p in result["variants"] if p["status"] != "proved"), "proved")
        except (Unsupported, ValueError, TypeError, IndexError) as error:
            result.update(status="unsupported", reason=str(error))
    if not rules:
        raise ValueError("stack proof file contains no rules")
    return dict(source=str(path), sha256=hashlib.sha256(source.encode()).hexdigest(), rules=rules,
                contracts=["canonical physical stack facets and legal depth encodings",
                           "Edit.Keep truncates; Edit.OverwriteOne replaces the matched window",
                           "sufficient input stack and gas; untouched deeper stack prefix",
                           "target lowering preserves physical stack operation semantics"])
