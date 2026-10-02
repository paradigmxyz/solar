"""Translate portable SMT-LIB word queries into Lean 4 theorems.

A query is UNSAT exactly when the conjunction of its assertions is false for
every assignment of its free constants. The theorem states that, with each
free constant as a universally quantified argument, so a Lean proof of it is a
proof of the rule's obligation checked by Lean's kernel instead of an SMT
answer. Operations map to the `BitVec` functions with SMT-LIB semantics: SMT
division by zero yields all ones (`BitVec.smtUDiv`), while Lean's `/` yields
zero. Shared subterms stay `let`-bound, as in the query. A balance array of a
`QF_ABV` query becomes a universally quantified function from addresses to
words, read by application.
"""

import re
from itertools import pairwise

THEOREM_PRELUDE = """import Std.Tactic.BVDecide

set_option maxHeartbeats 0
set_option maxRecDepth 1000000
set_option linter.unusedVariables false
"""


class UnsupportedQuery(ValueError):
    """A query outside the quantifier-free bitvector fragment this translator maps."""


def tokens(text):
    pattern = re.compile(r";[^\n]*|\(|\)|\|[^|]*\||[^\s()]+")
    for token in pattern.findall(text):
        if not token.startswith(";"):
            yield token


def parse(text):
    """Return the query's top-level s-expressions as nested lists of atoms."""
    stack, top = [], []
    for token in tokens(text):
        if token == "(":
            stack.append([])
        elif token == ")":
            if not stack:
                raise UnsupportedQuery("unbalanced parentheses")
            node = stack.pop()
            (stack[-1] if stack else top).append(node)
        else:
            (stack[-1] if stack else top).append(token)
    if stack:
        raise UnsupportedQuery("unbalanced parentheses")
    return top


def lean_name(symbol):
    name = re.sub(r"[^A-Za-z0-9_]", "_", symbol.strip("|"))
    return name if name and not name[0].isdigit() else f"v_{name}"


class Translator:
    def __init__(self):
        self.sorts = {}
        self.fresh = 0

    def sort(self, node):
        if node == "Bool":
            return "Bool"
        if isinstance(node, list) and node[:2] == ["_", "BitVec"]:
            return int(node[2])
        if isinstance(node, list) and len(node) == 3 and node[0] == "Array":
            domain, codomain = self.sort(node[1]), self.sort(node[2])
            if isinstance(domain, int) and isinstance(codomain, int):
                return ("Array", domain, codomain)
        raise UnsupportedQuery(f"unsupported sort {node}")

    def declare(self, form):
        if form[0] == "declare-fun":
            _, name, arguments, sort = form
            if arguments:
                raise UnsupportedQuery("uninterpreted functions are not supported")
        elif form[0] == "declare-const":
            _, name, sort = form
        else:
            raise UnsupportedQuery(f"unexpected declaration {form[0]}")
        self.sorts[name] = self.sort(sort)
        return lean_name(name), self.sorts[name]

    def term(self, node, scope):
        """Return the Lean text and sort (`Bool` or a width) of an SMT term."""
        if isinstance(node, str):
            return self.atom(node, scope)
        head, *args = node
        if head == "_":
            # An indexed literal such as `(_ bv7 256)`.
            return self.indexed(node, [], scope)
        if head == "let":
            return self.let(args, scope)
        if isinstance(head, list):
            return self.indexed(head, args, scope)
        operands = [self.term(arg, scope) for arg in args]
        texts = [text for text, _ in operands]
        sorts = [sort for _, sort in operands]
        match head, len(args):
            case "not", 1:
                return f"(!{texts[0]})", "Bool"
            case "and", _:
                return "(" + " && ".join(texts) + ")", "Bool"
            case "or", _:
                return "(" + " || ".join(texts) + ")", "Bool"
            case "xor", 2:
                return f"({texts[0]} != {texts[1]})", "Bool"
            case "=>", 2:
                return f"(!{texts[0]} || {texts[1]})", "Bool"
            case "=", _ if len(args) >= 2:
                pairs = [f"({a} == {b})" for a, b in pairwise(texts)]
                return "(" + " && ".join(pairs) + ")", "Bool"
            case "distinct", 2:
                return f"({texts[0]} != {texts[1]})", "Bool"
            case "ite", 3:
                return f"(bif {texts[0]} then {texts[1]} else {texts[2]})", sorts[1]
            case (("bvult" | "bvule" | "bvslt" | "bvsle"), 2):
                return f"(BitVec.{head[2:]} {texts[0]} {texts[1]})", "Bool"
            case (("bvugt" | "bvuge" | "bvsgt" | "bvsge"), 2):
                flipped = {
                    "bvugt": "ult",
                    "bvuge": "ule",
                    "bvsgt": "slt",
                    "bvsge": "sle",
                }
                return f"(BitVec.{flipped[head]} {texts[1]} {texts[0]})", "Bool"
            case "bvneg", 1:
                return f"(-{texts[0]})", sorts[0]
            case "bvnot", 1:
                return f"(~~~{texts[0]})", sorts[0]
            case (("bvadd" | "bvmul" | "bvand" | "bvor" | "bvxor"), _) if (
                len(args) >= 2
            ):
                operator = {
                    "bvadd": "+",
                    "bvmul": "*",
                    "bvand": "&&&",
                    "bvor": "|||",
                    "bvxor": "^^^",
                }[head]
                return "(" + f" {operator} ".join(texts) + ")", sorts[0]
            case "select", 2 if isinstance(sorts[0], tuple):
                return f"({texts[0]} {texts[1]})", sorts[0][2]
            case "bvsub", 2:
                return f"({texts[0]} - {texts[1]})", sorts[0]
            case "bvshl", 2:
                return f"({texts[0]} <<< {texts[1]})", sorts[0]
            case "bvlshr", 2:
                return f"({texts[0]} >>> {texts[1]})", sorts[0]
            case "bvashr", 2:
                return f"(BitVec.sshiftRight' {texts[0]} {texts[1]})", sorts[0]
            case "bvurem", 2:
                # `%` is `BitVec.umod`, which keeps the dividend for a zero divisor, as SMT does.
                return f"({texts[0]} % {texts[1]})", sorts[0]
            case (("bvudiv" | "bvsdiv" | "bvsrem" | "bvsmod"), 2):
                function = {
                    "bvudiv": "smtUDiv",
                    "bvsdiv": "smtSDiv",
                    "bvsrem": "srem",
                    "bvsmod": "smod",
                }[head]
                return f"(BitVec.{function} {texts[0]} {texts[1]})", sorts[0]
            case "concat", 2:
                return f"({texts[0]} ++ {texts[1]})", sorts[0] + sorts[1]
        raise UnsupportedQuery(f"unsupported operation {head}/{len(args)}")

    def atom(self, node, scope):
        if node in ("true", "false"):
            return node, "Bool"
        if node.startswith("#x"):
            return f"(0x{node[2:]}#{4 * (len(node) - 2)})", 4 * (len(node) - 2)
        if node.startswith("#b"):
            return f"(0b{node[2:]}#{len(node) - 2})", len(node) - 2
        if node in scope:
            return scope[node]
        if node in self.sorts:
            return lean_name(node), self.sorts[node]
        raise UnsupportedQuery(f"unbound symbol {node}")

    def indexed(self, head, args, scope):
        if head[:1] != ["_"]:
            raise UnsupportedQuery(f"unsupported indexed head {head}")
        name, *indices = head[1:]
        if name.startswith("bv") and not args:
            return f"({name[2:]}#{indices[0]})", int(indices[0])
        (text, width), *rest = [self.term(arg, scope) for arg in args]
        if rest:
            raise UnsupportedQuery(f"unexpected arity for {name}")
        match name:
            case "extract":
                high, low = int(indices[0]), int(indices[1])
                return (
                    f"(BitVec.extractLsb' {low} {high - low + 1} {text})",
                    high - low + 1,
                )
            case "zero_extend":
                extended = width + int(indices[0])
                return f"(BitVec.setWidth {extended} {text})", extended
            case "sign_extend":
                extended = width + int(indices[0])
                return f"(BitVec.signExtend {extended} {text})", extended
        raise UnsupportedQuery(f"unsupported indexed operation {name}")

    def let(self, args, scope):
        bindings, body = args
        inner = dict(scope)
        lines = []
        for name, value in bindings:
            text, sort = self.term(value, scope)
            self.fresh += 1
            local = f"t{self.fresh}"
            lines.append(f"let {local} := {text}")
            inner[name] = (local, sort)
        text, sort = self.term(body, inner)
        return "(" + "; ".join(lines) + f"; {text})", sort


def lean_sort(sort):
    if sort == "Bool":
        return "Bool"
    if isinstance(sort, tuple):
        return f"BitVec {sort[1]} → BitVec {sort[2]}"
    return f"BitVec {sort}"


def theorem(name, query, tactic):
    """Return a Lean theorem that holds exactly when the SMT-LIB `query` is UNSAT."""
    translator = Translator()
    variables, assertions = [], []
    for form in parse(query):
        if not isinstance(form, list) or not form:
            raise UnsupportedQuery("unexpected top-level atom")
        match form[0]:
            case "set-logic":
                if form[1] not in ("QF_BV", "QF_ABV"):
                    raise UnsupportedQuery(f"unsupported logic {form[1]}")
            case "declare-fun" | "declare-const":
                variables.append(translator.declare(form))
            case "assert":
                text, sort = translator.term(form[1], {})
                if sort != "Bool":
                    raise UnsupportedQuery("assertions must be Boolean")
                assertions.append(text)
            case "check-sat" | "set-info" | "set-option":
                pass
            case _:
                raise UnsupportedQuery(f"unsupported command {form[0]}")
    if not assertions:
        raise UnsupportedQuery("query has no assertions")
    binders = " ".join(
        f"({variable} : {lean_sort(sort)})" for variable, sort in variables
    )
    statement = " && ".join(assertions)
    proof = "\n".join(
        f"  {line}" if line else "" for line in tactic.strip("\n").splitlines()
    )
    return f"theorem {name} {binders} :\n    ({statement}) = false := by\n{proof}\n"
