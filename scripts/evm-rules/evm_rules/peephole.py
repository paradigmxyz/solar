"""Prove every rule of `evm-ir/peephole.isle` as an equivalence of instruction windows.

A rule's window and `Edit` become two instruction sequences that run on one symbolic
stack. They must leave every touched stack word, the memory, the ordered effects and
the branch decision of a `JUMPI` the same, and may not need more input words or change
the stack height. Memory is a function from words to words that `MSTORE` updates and
`MLOAD` reads; an initial read is a free word, and equal addresses read equal words.
Each opcode class (`absorbs_zero`, `is_sink`, ...) is checked for every opcode the Rust
classification lists in `peephole/classes.snap`, and each window with a variable depth
for every legal depth. Extractors with nontrivial logic stay trusted contracts; the
proof covers the identity each contract implies. Unknown forms, edits, guards and
opcodes fail.
"""

import re
from itertools import product

from .expr import MASK, SIGN, Cond, Expr, Unsupported, concrete
from .isle import ROOT, Rule, forms
from .lean import OPERATIONS, canonical

CLASSES = ROOT / "crates/codegen/src/backend/evm/ir/passes/peephole/classes.snap"

# Words the snapshot names; any other word is hexadecimal.
WORDS = {
    "zero": 0,
    "one": 1,
    "two": 2,
    "31": 31,
    "32": 32,
    "max": MASK,
    "smin": SIGN,
    "smax": SIGN - 1,
}
# Opcodes whose word semantics the Lean model defines, with their arity.
WORD_OPS = {
    name: arity for name, arity in OPERATIONS.items() if name not in ("ne", "select")
}
# Opcodes the machine models directly: memory words and the branch decision.
STRUCTURAL_OPS = {"mstore": (2, 0), "mload": (1, 1), "jumpi": (2, 0)}
CLASS_GUARDS = {
    "absorbs_zero",
    "zero_identity",
    "is_commutative",
    "is_noncommutative_binop",
    "is_sink",
}
# The class of rules that ends in each kind of instruction, and each extractor's own.
TAIL_ROOTS = {
    "dup": "peep_dup",
    "swap": "peep_swap",
    "exchange": "peep_exchange",
    "pop": "peep_pop",
    "op": "peep_op",
    "class": "peep_op",
}
EXTRACTOR_ROOTS = {
    "swap_pop_chain": "peep_pop",
    "swap_discard_chain": "peep_pop",
    "duplicates_known_zero": "peep_dup",
    "fold_constants": "peep_op",
    "fold_unary_constant": "peep_op",
    "untaken_jump": "peep_op",
    "noop_stack_suffix": "peep_pop",
    "invert_comparison": "peep_op",
}
WINDOW = re.compile(r"(unprotected_)?last(\d)")
DEPTHS = range(1, 236)
ZERO = Expr.const(0)
ONE = Expr.const(1)

# Trusted contracts of the peephole rules; reports list them with the proofs.
CONTRACTS = {
    "all": [
        "canonical physical stack facets, legal depth encodings and window extents",
        "Edit::apply keeps the instructions the model keeps and overwrites opcodes only",
        "an overwritten POP or SWAP1 opcode lowers to the same physical stack operation",
        "sufficient input stack and gas; untouched deeper stack prefix",
        "the window extractors decide protected boundaries and push removability",
    ],
    "memory": [
        "memory is word-granular: only reads and writes of one address are related",
        "both sequences touch the same addresses, so memory expansion agrees",
    ],
    "branch": [
        "the retained label push keeps its target; JUMPI pops its operands on both paths",
    ],
    "classes": [
        "classes.snap lists the opcodes of each Rust classification",
    ],
    "peak": [
        "a rewrite that raises the stack peak relies on the earlier 1024-word limit check",
    ],
    "swap_pop_chain": [
        "swap_pop_chain matches SWAPn, n POPs, SWAP1 and POP with n + 1 encodable",
    ],
    "swap_discard_chain": [
        "swap_discard_chain matches SWAPn and n + 1 POPs",
    ],
    "duplicates_known_zero": [
        "duplicates_known_zero matches a DUPn of a word that is a known zero",
    ],
    "noop_stack_suffix": [
        "noop_stack_suffix reads only words its own pushes created and ends at depth zero",
    ],
    "untaken_jump": ["untaken_jump matches PUSH 0, a block push and JUMPI"],
    "fold_constants": [
        "the folded word equals the constant evaluator's result in pop order",
        "materialize_immediate pushes exactly the folded word; its stack peak is not rechecked",
    ],
    "invert_comparison": [
        "invert_comparison keeps one operand block that cannot observe the constant",
        "classes.snap lists its bound adjustment, flipped opcode and rejected wrapping bounds",
    ],
}
CONTRACTS["fold_unary_constant"] = CONTRACTS["fold_constants"]


class Classes:
    """The Rust classification of opcodes and the extractors' constant tables."""

    def __init__(self, text):
        self.members = {}
        self.flips = {}
        self.folds = {}
        self.inverts = {}
        self.edges = {}
        for line in text.splitlines():
            kind, *fields = line.split()
            if kind == "class":
                name, mnemonic, io = fields
                inputs, outputs = io.split("/")
                self.members.setdefault(name, {})[mnemonic] = (
                    int(inputs),
                    int(outputs),
                )
            elif kind == "flip":
                self.flips[fields[0]] = fields[1]
            elif kind in ("fold1", "fold2"):
                key = kind, fields[0]
                self.folds.setdefault(key, {})[tuple(fields[1:-1])] = fields[-1]
            elif kind == "invert":
                mnemonic, depth, delta, flipped = fields
                self.inverts[mnemonic, int(depth)] = (delta, flipped)
            elif kind == "edge":
                mnemonic, depth, edge, verdict = fields
                self.edges[mnemonic, int(depth), edge] = verdict
            else:
                raise ValueError(f"unknown snapshot row: {line}")
        self.io = {
            name: io
            for members in self.members.values()
            for name, io in members.items()
        }

    def members_of(self, name):
        if not self.members.get(name):
            raise Unsupported(f"no opcodes listed for class {name}")
        return self.members[name]


def read_classes():
    return Classes(CLASSES.read_text())


def read_word(token):
    return WORDS[token] if token in WORDS else int(token, 16)


def peephole_rules(path):
    """Read the rules of the peephole rule file; declarations are skipped."""
    rules = [
        Rule(form, line, str(path))
        for form, line in forms(path.read_text())
        if form[0] == "rule"
    ]
    if not rules:
        raise ValueError("peephole proof file contains no rules")
    return rules


def is_variable(token):
    return (
        isinstance(token, str)
        and token != "_"
        and not token.startswith("$")
        and not token.isdecimal()
    )


def instruction_of(mnemonic):
    """The instruction of an opcode, with the stack operations in their physical form."""
    if mnemonic == "pop":
        return ("pop",)
    for kind in ("dup", "swap"):
        suffix = mnemonic.removeprefix(kind)
        if suffix != mnemonic and suffix.isdecimal() and 1 <= int(suffix) <= 16:
            return (kind, int(suffix))
    return ("op", mnemonic)


def legal(instruction):
    name, *args = instruction
    if name in ("dup", "swap"):
        return 1 <= args[0] <= 235
    if name == "exchange":
        return 1 <= args[0] < args[1] and sum(args) <= 30
    return True


def shape(instruction, io):
    """The input words an instruction requires, the words it pops and the words it pushes."""
    name, *args = instruction
    if name in ("push", "block"):
        return 0, 0, 1
    if name == "dup":
        return args[0], 0, 1
    if name == "swap":
        return args[0] + 1, 0, 0
    if name == "exchange":
        return max(args) + 1, 0, 0
    if name == "pop":
        return 1, 1, 0
    mnemonic = args[0]
    if mnemonic in WORD_OPS:
        return WORD_OPS[mnemonic], WORD_OPS[mnemonic], 1
    inputs, outputs = STRUCTURAL_OPS.get(mnemonic) or io.get(mnemonic) or (None, None)
    if inputs is None:
        raise Unsupported(f"unmodeled opcode: {mnemonic}")
    return inputs, inputs, outputs


def requirements(sequence, io):
    """The input words a sequence needs, its peak growth and its net growth."""
    needed = height = peak = 0
    for instruction in sequence:
        required, popped, pushed = shape(instruction, io)
        needed = max(needed, required - height)
        height += pushed - popped
        peak = max(peak, height)
    return needed, peak, height


def read(memory, address, atoms):
    """The word at `address` after a chain of stores to the initial memory."""
    if memory is None:
        return atoms.setdefault(address, Expr.var(f"@m{len(atoms)}"))
    _, previous, stored, value = memory
    return Expr(
        "select",
        (Expr("eq", (address, stored)), value, read(previous, address, atoms)),
    )


class Run:
    """What a sequence leaves: stack, memory, ordered effects and the branch."""

    def __init__(self, inputs):
        self.stack = list(inputs)
        self.memory = None
        self.touched = set()
        self.effects = []
        self.jump = None


def execute(sequence, inputs, atoms, io):
    run = Run(inputs)
    stack = run.stack
    for instruction in sequence:
        if run.jump is not None:
            raise Unsupported("instruction after a jump")
        name, *args = instruction
        if name == "push":
            stack.append(args[0])
        elif name == "block":
            stack.append(Expr.var(f"@block:{args[0]}"))
        elif name == "dup":
            stack.append(stack[-args[0]])
        elif name == "swap":
            index = -args[0] - 1
            stack[-1], stack[index] = stack[index], stack[-1]
        elif name == "exchange":
            a, b = (-index - 1 for index in args)
            stack[a], stack[b] = stack[b], stack[a]
        elif name == "pop":
            stack.pop()
        elif args[0] in WORD_OPS:
            operands = tuple(stack.pop() for _ in range(WORD_OPS[args[0]]))
            stack.append(Expr(args[0], operands))
        elif args[0] == "mstore":
            address, value = stack.pop(), stack.pop()
            run.memory = ("store", run.memory, address, value)
            run.touched.add(address)
        elif args[0] == "mload":
            address = stack.pop()
            stack.append(read(run.memory, address, atoms))
            run.touched.add(address)
        elif args[0] == "jumpi":
            run.jump = (stack.pop(), stack.pop())
        else:
            popped, _, pushed = shape(instruction, io)
            operands = tuple(stack.pop() for _ in range(popped))
            run.effects.append((args[0], operands))
            stack.extend(
                Expr.var(f"@effect:{len(run.effects)}:{i}") for i in range(pushed)
            )
    return run


def different(a, b):
    return [] if a == b else [Expr("xor", (a, b))]


def decision(condition):
    return Expr("ne", (condition, ZERO))


def obligation(before, after, io, assumptions=()):
    """Compare two sequences; return the word that must be zero and its preconditions."""
    needed, peak, delta = requirements(before, io)
    new_needed, new_peak, new_delta = requirements(after, io)
    if new_needed > needed or new_delta != delta:
        raise Unsupported("replacement needs more input words or changes the height")
    inputs = [Expr.var(f"@s{i}") for i in range(needed)]
    atoms = {}
    lhs = execute(before, inputs, atoms, io)
    rhs = execute(after, inputs, atoms, io)
    differences = []
    for a, b in zip(lhs.stack, rhs.stack, strict=True):
        differences += different(a, b)
    memory = bool(lhs.touched or rhs.touched)
    if memory:
        if lhs.effects or rhs.effects:
            raise Unsupported("memory operations and other effects in one window")
        if len(lhs.touched | rhs.touched) != 1 or lhs.touched != rhs.touched:
            raise Unsupported("memory rules must touch exactly one address")
        witness = Expr.var("@k")
        differences += different(
            read(lhs.memory, witness, atoms), read(rhs.memory, witness, atoms)
        )
    else:
        if [name for name, _ in lhs.effects] != [name for name, _ in rhs.effects]:
            raise Unsupported("replacement changes the effects")
        for (_, a), (_, b) in zip(lhs.effects, rhs.effects, strict=True):
            for x, y in zip(a, b, strict=True):
                differences += different(x, y)
    if lhs.jump and rhs.jump:
        differences += different(lhs.jump[0], rhs.jump[0])
        differences += different(decision(lhs.jump[1]), decision(rhs.jump[1]))
    elif lhs.jump:
        # The jump disappears, so its condition must never hold.
        differences.append(decision(lhs.jump[1]))
    elif rhs.jump:
        raise Unsupported("replacement adds a jump")
    # Equal addresses read equal words.
    accounts = list(atoms.items())
    congruence = [
        Cond(
            "implies",
            (Cond("eq", (address, other)), Cond("eq", (word, other_word))),
        )
        for i, (address, word) in enumerate(accounts)
        for other, other_word in accounts[i + 1 :]
    ]
    difference = ZERO
    for item in differences:
        difference = Expr("or", (difference, item))
    return {
        "difference": difference,
        "preconditions": [*assumptions, *congruence],
        "peak_growth": max(0, new_peak - peak),
        "memory": memory,
        "branch": bool(lhs.jump),
    }


def eval_token(token, bindings):
    """A number, a bound variable, or `(u8_add token number)`."""
    if isinstance(token, int):
        return token
    if isinstance(token, tuple):
        if len(token) != 3 or token[0] != "u8_add" or not token[2].isdecimal():
            raise Unsupported(f"unmodeled expression: {token}")
        total = eval_token(token[1], bindings) + int(token[2])
        if total > 255:
            raise Unsupported("u8 overflow")
        return total
    if token.isdecimal():
        return int(token)
    if isinstance(bindings.get(token), int):
        return bindings[token]
    raise Unsupported(f"unbound number: {token}")


def depth_token(token, index):
    """A literal depth, or the variable a wildcard or name stands for."""
    if token.isdecimal():
        return int(token)
    return f"@wild{index}" if token == "_" else token


def template(pattern, index):
    """One window pattern as an instruction template over the rule's variables."""
    head, *args = pattern
    if head == "push" and len(args) == 1 and is_variable(args[0]):
        return ("push", args[0])
    if head == "any_push" and not args:
        return ("push", f"@any{index}")
    if head == "push_block" and not args:
        return ("block", f"b{index}")
    if head in ("dup", "swap") and len(args) == 1:
        return (head, depth_token(args[0], index))
    if head == "exchange" and len(args) == 2:
        return ("exchange", *(depth_token(a, index) for a in args))
    if head == "pop" and not args:
        return ("pop",)
    if head == "opcode" and len(args) == 1:
        if args[0].startswith("$"):
            return instruction_of(args[0].removeprefix("$").lower())
        if is_variable(args[0]):
            return ("class", args[0])
    raise Unsupported(f"unmodeled instruction pattern: {pattern}")


def read_guards(guards):
    """The constants, opcode classes and flipped comparisons that the guards fix."""
    constants, classes, flips = {}, {}, {}
    for guard in guards:
        if len(guard) != 3 or guard[0] != "if-let" or len(guard[2]) != 2:
            raise Unsupported(f"unmodeled guard: {guard}")
        _, bound, (function, argument) = guard
        if not is_variable(argument) or argument in {
            *constants,
            *classes,
            *flips.values(),
        }:
            raise Unsupported(f"unmodeled guard: {guard}")
        if bound == "true" and function in ("u256_is_zero", "u256_is_one"):
            constants[argument] = int(function == "u256_is_one")
        elif bound == "true" and function in CLASS_GUARDS:
            classes[argument] = function
        elif function == "flipped_comparison" and is_variable(bound):
            flips[bound] = argument
        else:
            raise Unsupported(f"unmodeled guard: {guard}")
    return constants, classes, flips


def opcode_choices(templates, classes, flips, snapshot):
    """The assignments of each opcode variable of a window, one list per variable."""
    used = {t[1] for t in templates if t[0] == "class"}
    if used != {*classes, *flips.values()}:
        raise Unsupported("opcode variable without a class guard")
    choices = []
    for variable, name in sorted(classes.items()):
        members = snapshot.members_of(name)
        choices.append([{variable: member} for member in members])
    for result, variable in sorted(flips.items()):
        if not snapshot.flips:
            raise Unsupported("no flipped comparisons listed")
        choices.append(
            [{variable: m, result: flipped} for m, flipped in snapshot.flips.items()]
        )
    for options in choices:
        if any(instruction_of(m)[0] != "op" for o in options for m in o.values()):
            raise Unsupported("opcode class contains a stack operation")
    return choices


def assignments(templates, choices):
    """Every assignment of the depth and opcode variables of a window."""
    depth_variables = sorted(
        {
            arg
            for t in templates
            if t[0] in ("dup", "swap", "exchange")
            for arg in t[1:]
            if isinstance(arg, str)
        }
    )
    if len(depth_variables) > 2:
        raise Unsupported("stack rule has more than two depth bindings")
    for depths in product(DEPTHS, repeat=len(depth_variables)):
        for picks in product(*choices):
            bindings = dict(zip(depth_variables, depths))
            for pick in picks:
                bindings.update(pick)
            yield bindings


def resolve(template, bindings, constants):
    """A window template under one assignment."""
    name, *args = template
    if name == "push":
        value = constants.get(args[0])
        return ("push", Expr.var(args[0]) if value is None else Expr.const(value))
    if name in ("dup", "swap", "exchange"):
        return (name, *(eval_token(arg, bindings) for arg in args))
    if name == "class":
        return instruction_of(bindings[args[0]])
    return template


def opcode_of(token, bindings):
    """The instruction an edit names by a literal or a variable of the rule."""
    if token.startswith("$"):
        return instruction_of(token.removeprefix("$").lower())
    if isinstance(bindings.get(token), str):
        return instruction_of(bindings[token])
    raise Unsupported(f"unbound opcode: {token}")


def swap_of(token, bindings):
    if not isinstance(token, tuple) or len(token) != 2 or token[0] != "swap_op":
        raise Unsupported(f"unmodeled stack operation: {token}")
    return ("swap", eval_token(token[1], bindings))


def apply_edit(edit, before, bindings):
    """The instructions `Edit::apply` leaves for a window of `before`."""
    name, *args = edit
    count = len(before)

    def fits(size, *kinds, exact=False):
        if count != size if exact else count < size:
            raise Unsupported(f"edit {name} does not fit its window")
        if any(before[i][0] != kind for i, kind in kinds):
            raise Unsupported(f"edit {name} does not fit its window")

    match name, args:
        case "Edit.Keep", [length] if length.isdecimal() and int(length) <= count:
            return before[: int(length)]
        case "Edit.OverwritePush0", []:
            fits(1, (0, "dup"), exact=True)
            return [("push", ZERO)]
        case "Edit.RemoveFirstKeepOne", []:
            fits(2)
            return before[1:2]
        case "Edit.RemoveFirstKeepTwo", []:
            fits(3)
            return before[1:3]
        case "Edit.RemoveFirstOverwrite", [opcode]:
            fits(2, (1, "op"))
            return [opcode_of(opcode, bindings), *before[2:]]
        case "Edit.SwapOverwrite", [opcode]:
            fits(2, (1, "op"))
            return [opcode_of(opcode, bindings), before[0], *before[2:]]
        case "Edit.OverwriteOne", [opcode]:
            fits(1)
            return [opcode_of(opcode, bindings)]
        case "Edit.OverwriteTwo", [opcode]:
            fits(2)
            return [("swap", 1), opcode_of(opcode, bindings)]
        case "Edit.MergeSwapPop", [depth]:
            fits(3)
            return [
                ("swap", eval_token(depth, bindings)),
                *before[1 : count - 2],
                ("pop",),
            ]
        case "Edit.DropDiscardedSwap", []:
            fits(2, (0, "swap"))
            return before[1:]
        case "Edit.ZeroLessIszero", []:
            fits(3, exact=True)
            return [("op", "iszero")]
        case "Edit.ReloadStoredValue", []:
            fits(4, (3, "op"), exact=True)
            return [("dup", 1), before[2], before[1]]
        case "Edit.DropNonzeroTest", []:
            fits(4, (2, "block"), (3, "op"), exact=True)
            return [before[2], ("op", "jumpi")]
        case "Edit.EqIszeroJumpi", []:
            fits(4, (0, "op"), (1, "op"), (2, "block"), (3, "op"), exact=True)
            return [("op", "sub"), before[2], ("op", "jumpi")]
        case "Edit.StackOps", [first, second]:
            fits(2)
            return [swap_of(first, bindings), swap_of(second, bindings)]
    raise Unsupported(f"unmodeled edit: {edit}")


def check_folds(snapshot, kind, mnemonic):
    """The evaluator's results must agree with the word model on every listed input."""
    arity = 1 if kind == "fold1" else 2
    rows = snapshot.folds[kind, mnemonic]
    if WORD_OPS.get(mnemonic) != arity:
        raise Unsupported(f"unmodeled folded opcode: {mnemonic}")
    if len(rows) != len(WORDS) ** arity:
        raise Unsupported(f"constant evaluator is partial for {mnemonic}")
    for words, result in rows.items():
        arguments = tuple(Expr.const(read_word(word)) for word in words)
        if concrete(Expr(mnemonic, arguments), {}) != read_word(result):
            raise Unsupported(
                f"constant evaluator disagrees with the model on {mnemonic}"
            )


def inversion(snapshot, mnemonic, depth):
    """The window `PUSH c; operands; compare; ISZERO`, its replacement and the no-wrap guard."""
    delta, flipped = snapshot.inverts[mnemonic, depth]
    if delta not in ("+1", "-1") or snapshot.flips.get(mnemonic) != flipped:
        raise Unsupported("unmodeled comparison inversion")
    signed = mnemonic in ("slt", "sgt")
    bias = Expr.const(SIGN)
    constant = Expr.var("@c")
    # Signed bounds move in the biased unsigned order, as the extractor does.
    ordered = Expr("xor", (constant, bias)) if signed else constant
    bound = Expr("add" if delta == "+1" else "sub", (ordered, ONE))
    if signed:
        bound = Expr("xor", (bound, bias))
    # The bound wraps at the largest ordered word for +1 and at zero for -1.
    wrap = MASK if delta == "+1" else 0
    edge = wrap ^ (SIGN if signed else 0)
    name = {0: "zero", MASK: "max", SIGN: "smin", SIGN - 1: "smax"}[edge]
    if snapshot.edges.get((mnemonic, depth, name)) != "rejected":
        raise Unsupported(f"the wrapping bound of {mnemonic} is not rejected")
    # The operand block cannot observe the constant; at depth one it leaves one word.
    operands = [("push", Expr.var("@y"))] * depth
    before = [("push", constant), *operands, ("op", mnemonic), ("op", "iszero")]
    after = [("push", bound), *operands, ("op", flipped)]
    return before, after, Cond("ne", (ordered, Expr.const(wrap)))


def variant(bindings, before, after, io, assumptions=()):
    return {"bindings": bindings, **obligation(before, after, io, assumptions)}


def extractor_variants(name, args, skip, edit, snapshot, io):
    """Variants of a window that a Rust extractor matches, under its contract."""
    if name == "swap_pop_chain":
        (depth,) = args
        for n in range(1, 235):
            bindings = {depth: n}
            before = [("swap", n), *[("pop",)] * n, ("swap", 1), ("pop",)]
            if eval_token(skip, bindings) != len(before):
                raise Unsupported("unmodeled window or rewrite extent")
            yield variant(bindings, before, apply_edit(edit, before, bindings), io)
    elif name == "swap_discard_chain":
        (pops,) = args
        for count in range(2, 237):
            bindings = {pops: count}
            before = [("swap", count - 1), *[("pop",)] * count]
            if eval_token(skip, bindings) != len(before):
                raise Unsupported("unmodeled window or rewrite extent")
            yield variant(bindings, before, apply_edit(edit, before, bindings), io)
    elif name == "duplicates_known_zero":
        if args or skip != "1" or edit != ("Edit.OverwritePush0",):
            raise Unsupported("unmodeled window or rewrite extent")
        for n in DEPTHS:
            # The duplicated word is the deepest input.
            known = Cond("eq", (Expr.var("@s0"), ZERO))
            yield variant({"depth": n}, [("dup", n)], [("push", ZERO)], io, [known])
    elif name == "noop_stack_suffix":
        (length,) = args
        if skip != length or edit != ("Edit.Keep", "0"):
            raise Unsupported("unmodeled window or rewrite extent")
        # The window reads only words it pushed, so it never touches the incoming stack.
        yield variant({}, [], [], io)
    elif name == "untaken_jump":
        if args or skip != "3" or edit != ("Edit.Keep", "0"):
            raise Unsupported("unmodeled window or rewrite extent")
        before = [("push", ZERO), ("block", "b"), ("op", "jumpi")]
        yield variant({}, before, [], io)
    elif name in ("fold_constants", "fold_unary_constant"):
        (value,) = args
        kind, arity = ("fold2", 2) if name == "fold_constants" else ("fold1", 1)
        if skip != str(arity + 1) or edit != ("Edit.FoldConstants", value):
            raise Unsupported("unmodeled window or rewrite extent")
        mnemonics = sorted(m for k, m in snapshot.folds if k == kind)
        if not mnemonics:
            raise Unsupported("no folded opcodes listed")
        for mnemonic in mnemonics:
            check_folds(snapshot, kind, mnemonic)
            pushes = [("push", Expr.var("@l")), ("push", Expr.var("@r"))][:arity]
            before = [*pushes, ("op", mnemonic)]
            # The folded word is the operation's result on the constants.
            after = [("push", execute(before, [], {}, io).stack[0])]
            yield variant({"op": mnemonic}, before, after, io)
    elif name == "invert_comparison":
        count, value, opcode = args
        if skip != count or edit != ("Edit.InvertComparison", value, opcode):
            raise Unsupported("unmodeled window or rewrite extent")
        if not snapshot.inverts:
            raise Unsupported("no comparison inversions listed")
        for mnemonic, depth in sorted(snapshot.inverts):
            before, after, guard = inversion(snapshot, mnemonic, depth)
            yield variant({"op": mnemonic, "depth": depth}, before, after, io, [guard])
    else:
        raise Unsupported(f"unmodeled window: {name}")


def rule_parts(rule):
    form = rule.form
    body = form[2:] if isinstance(form[1], str) and form[1].isdecimal() else form[1:]
    if len(body) < 2:
        raise Unsupported("unmodeled rule shape")
    (root, window), *guards, rewrite = body
    if len(rewrite) != 3 or rewrite[0] != "rewrite":
        raise Unsupported("unmodeled rewrite")
    return root, window, guards, rewrite[1], rewrite[2]


def peephole_variants(rule, snapshot):
    """Return every variant of a rule and the contracts it relies on."""
    root, (name, *args), guards, skip, edit = rule_parts(rule)
    constants, classes, flips = read_guards(guards)
    io = snapshot.io
    extractor = name in EXTRACTOR_ROOTS
    if extractor:
        if constants or classes or flips:
            raise Unsupported("guards on an extractor window")
        expected = EXTRACTOR_ROOTS[name]
        variants = list(extractor_variants(name, args, skip, edit, snapshot, io))
    else:
        match = WINDOW.fullmatch(name)
        if (
            not match
            or int(match[2]) != len(args)
            or (match[1] and len(args) not in (4, 5))
        ):
            raise Unsupported(f"unmodeled window: {name}")
        templates = [template(pattern, i) for i, pattern in enumerate(args)]
        if templates[-1][0] in ("push", "block"):
            raise Unsupported("no rule ends at a push")
        expected = TAIL_ROOTS[templates[-1][0]]
        if skip != str(len(templates)):
            raise Unsupported("unmodeled window or rewrite extent")
        choices = opcode_choices(templates, classes, flips, snapshot)
        variants = []
        for bindings in assignments(templates, choices):
            before = [resolve(t, bindings, constants) for t in templates]
            if all(map(legal, before)):
                after = apply_edit(edit, before, bindings)
                if not all(map(legal, after)):
                    raise Unsupported("edit builds an illegal stack operation")
                variants.append(variant(bindings, before, after, io))
    if root != expected:
        raise Unsupported("unmodeled window or rewrite extent")
    if not variants:
        raise Unsupported("no applicable bindings")
    contracts = list(CONTRACTS["all"])
    for condition, label in (
        (any(v["memory"] for v in variants), "memory"),
        (any(v["branch"] for v in variants), "branch"),
        (classes or flips, "classes"),
        (any(v["peak_growth"] for v in variants), "peak"),
        (extractor, name),
    ):
        if condition:
            contracts += CONTRACTS[label]
    return variants, contracts


def rename(item, names):
    """Rename variables in first-use order, so equal shapes share one theorem."""
    if isinstance(item, Expr):
        return canonical(item, names)
    args = (rename(a, names) if isinstance(a, (Expr, Cond)) else a for a in item.args)
    return Cond(item.op, tuple(args))


def peephole_theorems(rule, snapshot):
    """Return a rule's distinct (word, preconditions) theorems, its variants and contracts."""
    variants, contracts = peephole_variants(rule, snapshot)
    shapes = {}
    for entry in variants:
        names = {}
        difference = rename(entry["difference"], names)
        preconditions = tuple(rename(c, names) for c in entry["preconditions"])
        shapes[difference, preconditions] = None
    growth = max(entry["peak_growth"] for entry in variants)
    return shapes, len(variants), growth, contracts
