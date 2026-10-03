"""Generate Lean obligations directly from solver-independent EVM expressions.

The ISLE reader and EVM model remain trusted. Lean checks every equality and
applicability witness; unsuccessful tactics and unsupported terms fail closed.
"""

import os
import re
import subprocess
import tempfile
from pathlib import Path

from .semantics import Expr, Unsupported, concrete

LIBRARY = Path(__file__).resolve().parents[1] / "lean"
HEADER = "import Evm\nset_option maxRecDepth 4096\nset_option exponentiation.threshold 512\nset_option maxHeartbeats 0\nset_option linter.unusedVariables false\nset_option linter.unusedSimpArgs false\nopen Evm\n"
UNFOLD = "Evm.shiftSum, Evm.shl, Evm.shr, Evm.sar, Evm.div, Evm.mod, Evm.sdiv, Evm.smod, Evm.addmod, Evm.mulmod, Evm.exp, Evm.byte, Evm.signextend, Evm.clz"


class Lean:
    def __init__(self, variables):
        self.variables = {name: f"v{i}" for i, name in enumerate(sorted(variables))}

    def term(self, value):
        op, children = value.op, value.args
        if op == "var":
            return self.variables[children[0]]
        if op == "const":
            return f"{children[0]}#256"
        if op in (
            "eq",
            "ne",
            "lt",
            "le",
            "gt",
            "ge",
            "slt",
            "sgt",
            "iszero",
            "implies",
        ):
            return f"(if {self.predicate(value)} then (1 : Word) else 0)"
        if op == "select" and len(children) == 3:
            return f"(if {self.predicate(children[0])} then {self.term(children[1])} else {self.term(children[2])})"
        args = [self.term(child) for child in children]
        binary = {
            "add": "+",
            "sub": "-",
            "mul": "*",
            "and": "&&&",
            "or": "|||",
            "xor": "^^^",
        }
        if op in binary and len(args) == 2:
            return f"({args[0]} {binary[op]} {args[1]})"
        if op == "not" and len(args) == 1:
            return f"(~~~{args[0]})"
        arities = {
            "shiftSum": 2,
            "shl": 2,
            "shr": 2,
            "sar": 2,
            "div": 2,
            "mod": 2,
            "sdiv": 2,
            "smod": 2,
            "addmod": 3,
            "mulmod": 3,
            "exp": 2,
            "byte": 2,
            "signextend": 2,
            "clz": 1,
        }
        if op in arities and len(args) == arities[op]:
            return f"(Evm.{op} {' '.join(args)})"
        if op == "address" and not args:
            return "(account.setWidth 256)"
        if op == "balance" and len(args) == 1:
            return f"(balances ({args[0]}.setWidth 160))"
        if op == "selfbalance" and not args:
            return "(balances account)"
        raise Unsupported(f"unsupported Lean operation: {op}/{len(args)}")

    def predicate(self, value):
        op, children = value.op, value.args
        predicates = {
            "eq",
            "ne",
            "lt",
            "le",
            "gt",
            "ge",
            "slt",
            "sgt",
            "iszero",
            "implies",
        }
        if op in ("eq", "ne") and len(children) == 2:
            for a, b in (children, children[::-1]):
                if a.op == "const" and a.args[0] in (0, 1) and b.op in predicates:
                    condition = self.predicate(b)
                    return (
                        condition
                        if (a.args[0] == 1) == (op == "eq")
                        else f"¬ {condition}"
                    )
        if op == "implies" and len(children) == 2:
            return f"({self.predicate(children[0])} → {self.predicate(children[1])})"
        if op in ("var", "const"):
            return f"({self.term(value)} ≠ 0)"
        if op not in predicates:
            return f"({self.term(value)} ≠ 0)"
        args = [self.term(child) for child in children]
        comparisons = {"eq": "=", "ne": "≠", "lt": "<", "le": "≤", "gt": ">", "ge": "≥"}
        if op in comparisons and len(args) == 2:
            return f"({args[0]} {comparisons[op]} {args[1]})"
        if op in ("slt", "sgt") and len(args) == 2:
            a, b = args if op == "slt" else reversed(args)
            return f"(BitVec.slt {a} {b} = true)"
        if op == "iszero" and len(args) == 1:
            return f"({args[0]} = 0)"
        if op in comparisons or op in ("slt", "sgt", "iszero", "implies"):
            raise Unsupported(f"invalid predicate arity: {op}/{len(args)}")
        return f"({self.term(value)} ≠ 0)"

    def assignments(self, output):
        values = dict(re.findall(r"^(v\d+) = (\d+)#256$", output, re.MULTILINE))
        return {key: int(values.get(value, 0)) for key, value in self.variables.items()}

    def binders(self):
        return " ".join(f"({v} : Word)" for v in self.variables.values())


def compile_source(path, timeout=120):
    env = {
        **os.environ,
        "LEAN_PATH": str(LIBRARY.parents[2] / "target/evm-rules/library"),
    }
    try:
        return subprocess.run(
            ["lean", "-j1", "-DwarningAsError=true", str(path)],
            check=False,
            capture_output=True,
            text=True,
            timeout=timeout,
            env=env,
        )
    except subprocess.TimeoutExpired as error:
        return subprocess.CompletedProcess(error.cmd, 1, "", "Lean proof timed out")


def prepare(directory):
    version = (
        (LIBRARY / "lean-toolchain")
        .read_text()
        .strip()
        .removeprefix("leanprover/lean4:v")
    )
    installed = subprocess.run(
        ["lean", "--version"], check=True, capture_output=True, text=True, timeout=10
    ).stdout.strip()
    if not installed.startswith(f"Lean (version {version},"):
        raise Unsupported(f"expected Lean {version}, got {installed}")
    directory.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=directory) as temporary:
        subprocess.run(
            [
                "lean",
                "-j1",
                "-DwarningAsError=true",
                "-o",
                str(Path(temporary) / "Evm.olean"),
                str(LIBRARY / "Evm.lean"),
            ],
            check=True,
            timeout=300,
        )
        (Path(temporary) / "Evm.olean").replace(directory / "Evm.olean")
    return installed


def witness(assumptions, directory, name):
    variables = set().union(*(a.variables() for a in assumptions))
    values = dict.fromkeys(variables, 0)
    if all(concrete(a, values) for a in assumptions):
        return values
    emitter = Lean(variables)
    condition = " ∧ ".join(emitter.predicate(a) for a in assumptions) or "True"
    path = directory / f"{name}_witness.lean"
    path.write_text(
        HEADER
        + f"example {emitter.binders()} : ¬ ({condition}) := by\n  try simp only [{UNFOLD}]\n  all_goals bv_decide (timeout := 30)\n"
    )
    result = compile_source(path, 60)
    if "The prover found a counterexample" not in result.stdout:
        raise Unsupported(f"no applicability witness: {result.stdout}{result.stderr}")
    values = emitter.assignments(result.stdout)
    if not all(concrete(a, values) for a in assumptions):
        raise Unsupported(f"Lean's witness did not replay: {result.stdout}")
    return values


def theorem(lhs, rhs, assumptions, name, values, *, counterexample=False):
    variables = (
        lhs.variables()
        | rhs.variables()
        | set().union(*(a.variables() for a in assumptions))
    )
    emitter = Lean(variables)

    def reads_environment(value):
        return value.op in ("address", "balance", "selfbalance") or any(
            reads_environment(child) for child in value.args if isinstance(child, Expr)
        )

    environment = any(reads_environment(value) for value in (lhs, rhs, *assumptions))
    binders = emitter.binders() + (
        " (account : BitVec 160) (balances : BitVec 160 → Word)" if environment else ""
    )
    guards = " ".join(
        f"(_h{i} : {emitter.predicate(a)})" for i, a in enumerate(assumptions)
    )
    concrete_emitter = Lean(variables)
    concrete_emitter.variables = {k: f"{values.get(k, 0)}#256" for k in variables}
    applicability = (
        " ∧ ".join(concrete_emitter.predicate(a) for a in assumptions) or "True"
    )

    simplify = (
        ""
        if counterexample
        else "  try subst_vars\n  try simp_all\n  try apply congrArg balances\n"
    )
    unfold = (
        f"  try simp only [{UNFOLD}] at *\n"
        if counterexample
        else f"  try simp_all only [{UNFOLD}]\n"
    )
    return (
        HEADER
        + f"example : {applicability} := by decide\n\ntheorem {name} {binders} {guards} :\n    {emitter.term(lhs)} = {emitter.term(rhs)} := by\n{simplify}{unfold}  all_goals first | bv_omega | bv_decide (config := {{timeout := 30, acNf := true}})\n"
    )
