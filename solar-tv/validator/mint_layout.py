"""Propose metered symbolic routes through the emitted ERC20Mock mint entry."""

import hashlib
import json
import tempfile
from dataclasses import dataclass, field
from functools import partial
from pathlib import Path

from . import audit, entry_proof, native, world
from .call_budget import decode_runtime
from .common import Inconclusive, Program, Unsupported

SELECTOR = 0x40C10F19
MODULE = "SolarTV.EVMIR.Transforms.MintLayout"
HEADER = """open SolarTV EvmRules Execution EVM.MintLayout EVM.State EVM.State.BudgetedPath
set_option autoImplicit false
set_option maxRecDepth 8192
set_option maxHeartbeats 6000000
set_option Elab.async false
set_option linter.unusedSimpArgs false
set_option linter.unusedVariables false
"""

HEAD = "(readWord env.input 0)"
RECIPIENT = "(readWord env.input 4)"
AMOUNT = "(readWord env.input 36)"
SELECTED = f"(Evm.shr 224 {HEAD})"
SUPPLY = "(readSlot journal env.address 2)"
ADDITION = f"(Evm.add {SUPPLY} {AMOUNT})"
# These are branch predicates, not a separate implementation of mint.
GUARDS = {
    "value": "env.value",
    "abi": f"(Evm.or (Evm.lt (BitVec.ofNat 256 env.input.length) 68) (Evm.shr 160 {RECIPIENT}))",
    "recipient": f"(Evm.iszero {RECIPIENT})",
    "overflow": f"(Evm.lt {ADDITION} {SUPPLY})",
}
CASES = {
    "Nonpayable": {"value": True},
    "Malformed": {"value": False, "abi": True},
    "ZeroAddress": {"value": False, "abi": False, "recipient": True},
    "Overflow": {"value": False, "abi": False, "recipient": False, "overflow": True},
    "Success": {"value": False, "abi": False, "recipient": False, "overflow": False},
}


def args(case):
    return "env journal writable selected " + " ".join(f"h{k}" for k in case)


def parameters(case):
    guards = " ".join(
        f"(h{k} : {GUARDS[k]} {'≠' if taken else '='} 0)" for k, taken in case.items()
    )
    return (
        "(env : World.Environment) (journal : Journal) (writable : env.isStatic = false) "
        f"(selected : {SELECTED} = {SELECTOR}) {guards}"
    )


def list_term(items):
    return "[" + ", ".join(map(str, items)) + "]"


PURE = {
    "add": lambda a, b: (a + b) % 2**256,
    "sub": lambda a, b: (a - b) % 2**256,
    "lt": lambda a, b: int(a < b),
    "gt": lambda a, b: int(a > b),
    "eq": lambda a, b: int(a == b),
    "or": lambda a, b: a | b,
    "shr": lambda a, b: b >> a if a < 256 else 0,
    "iszero": lambda a: int(a == 0),
}


@dataclass
class Step:
    pc: int
    opcode: int
    schema: dict
    inputs: int
    outputs: int
    before: str
    after: str = ""
    journal: str = "journal"
    next_journal: str = "journal"
    cost: int | str = 0
    sentry: bool = False
    defs: list[str] = field(default_factory=list)
    facts: list[str] = field(default_factory=list)
    terminal: str = ""

    @property
    def minimum(self):
        return f"(max 2301 {self.cost})" if self.sentry else self.cost


@dataclass
class Route:
    steps: list[Step]
    definitions: list[str]

    @property
    def exit(self):
        return self.steps[-1].terminal


def propose(image, table, case):
    """Symbolically walk a leaf, with every proposal checked against the bytes."""
    decoded = decode_runtime(image, {op["name"]: op for op in table.values()})
    values = []
    steps = []
    definitions = []
    memory, size = "[]", 0
    journal, loaded = "journal", "journal"
    guards = {expression: name for name, expression in GUARDS.items()}
    seen = set()
    pc = 0
    for _ in range(128):
        if pc not in decoded:
            raise Unsupported(f"mint path leaves the decoded image at PC {pc}")
        op, immediate = decoded[pc]
        bite = image[pc]
        schema = table.get(bite)
        if schema is None or schema["stack"] is None:
            raise Unsupported(f"unsupported mint opcode {op} at PC {pc}")
        fork = schema["forks"]["cancun"]
        inputs, outputs = schema["stack"]
        if not fork["available"] or len(values) < inputs:
            raise Unsupported(f"mint opcode {op} cannot execute at PC {pc}")
        step = Step(
            pc,
            bite,
            fork,
            inputs,
            outputs,
            f"frame code env {pc} {list_term(values)} {memory}",
            journal=journal,
            next_journal=journal,
            cost=fork["base_gas"],
        )
        values_top, rest = values[:inputs], values[inputs:]
        next_pc = pc + 1
        if immediate is not None:
            if bite != 0x5F:
                width = bite - 0x5F
                next_pc += width
                step.facts.append(
                    f"have immediate : code.immediate {pc} {width} = {immediate} := by decide +kernel"
                )
            values = [immediate, *values]
        elif op.startswith("dup"):
            values = [values[bite - 0x80], *values]
        elif op.startswith("swap"):
            i = bite - 0x8F
            values[0], values[i] = values[i], values[0]
        elif op in PURE:
            normal = f"(Evm.{op} {' '.join(map(str, values_top))})"
            if all(isinstance(a, int) for a in values_top):
                word = PURE[op](*values_top)
            elif normal == SELECTED:
                word = SELECTOR
            else:
                word = normal
            values = [word, *rest]
        elif op == "callvalue":
            values = ["env.value", *rest]
        elif op == "calldatasize":
            values = ["(BitVec.ofNat 256 env.input.length)", *rest]
        elif op == "calldataload":
            values = [f"(readWord env.input {values_top[0]})", *rest]
        elif op in {"jump", "jumpi"}:
            target = values_top[0]
            if op == "jumpi":
                condition = values_top[1]
                if isinstance(condition, int):
                    taken = bool(condition)
                else:
                    branch = guards.get(condition)
                    if branch is None or branch not in case:
                        raise Unsupported(
                            f"unrecognized mint condition at PC {pc}: {condition}"
                        )
                    seen.add(branch)
                    taken = case[branch]
            else:
                taken = True
            if taken:
                next_pc = target
                if decoded.get(target) != ("jumpdest", None):
                    raise Unsupported(f"invalid mint destination at PC {pc}")
                step.facts.append(
                    f"have destination : {target} ∈ code.jumps 0 := by decide +kernel"
                )
            values = rest
        elif op == "jumpdest":
            pass
        elif op in {"mstore", "keccak256", "revert", "log3"}:
            offset = values_top[0]
            count = 32 if op == "mstore" else values_top[1]
            if (
                not isinstance(offset, int)
                or not isinstance(count, int)
                or offset + count > 64
            ):
                raise Unsupported("unsupported mint memory extent")
            grown = size if count == 0 else max(size, (offset + count + 31) // 32 * 32)
            expansion = 3 * (grown // 32 - size // 32)
            step.cost += expansion
            if op == "keccak256":
                step.cost += fork["dynamic_gas"] * ((count + 31) // 32)
            if op == "log3":
                step.cost += fork["dynamic_gas"] * count
            step.facts.append(
                f"have extent : ({memory} : List Byte).length = {size} := by simp"
            )
            step.defs.extend(
                [
                    "EVM.State.withMemory",
                    "memoryCharge",
                    "memoryCost",
                    "expand",
                    "requiredWords",
                    "memoryWords",
                    "expandTo",
                ]
            )
            if op == "mstore":
                mem = f"mem{pc}"
                definitions.append(
                    f"def {mem} (env : World.Environment) (journal : Journal) : List Byte :=\n"
                    f"  storedWord {memory} {grown} {offset} {values_top[1]}\n"
                    f"@[simp] theorem {mem}Length (env : World.Environment) (journal : Journal) : "
                    f"({mem} env journal).length = {grown} := by\n"
                    f"  simp [storedWord, {mem}, expandTo, writeBytes, wordBytes]"
                )
                memory = f"({mem} env journal)"
                step.defs.extend(["storedWord", mem])
                values = rest
            else:
                # Each non-store memory observer in this pair reads an active range.
                if grown != size:
                    raise Unsupported("mint memory observation expands memory")
                data = f"(readBytes {memory} {offset} {count})"
                if op == "keccak256":
                    values = [f"(Keccak.hash {data})", *rest]
                elif op == "revert":
                    step.terminal = f"⟨.reverted,{data},{journal}⟩"
                else:
                    step.next_journal = f"(log {journal} env.address {list_term(values_top[2:])} {data})"
                    step.defs.append("log")
                    values = rest
            size = grown
        elif op == "sload":
            slot = values_top[0]
            step.cost = f"(loadPrice cfg {journal} env.address {slot})"
            step.next_journal = f"(warmSlot {journal} env.address {slot})"
            values = [f"(readSlot {loaded} env.address {slot})", *rest]
            step.defs.append("loadPrice")
        elif op == "sstore":
            slot, value = values_top
            step.cost = f"(storePrice cfg {journal} env.address {slot} {value}).gas"
            step.sentry = True
            step.next_journal = f"(storeSlot cfg {journal} env.address {slot} {value})"
            step.defs.append("storeSlot")
            loaded = step.next_journal
            values = rest
        elif op == "stop":
            step.terminal = f"⟨.success,[],{journal}⟩"
        else:
            raise Unsupported(f"unsupported mint opcode {op} at PC {pc}")
        steps.append(step)
        if step.terminal:
            if seen != case.keys():
                raise Unsupported(
                    f"mint route did not consume its branch conditions: {seen}"
                )
            return Route(steps, definitions)
        step.after = f"frame code env {next_pc} {list_term(values)} {memory}"
        journal = step.next_journal
        pc = next_pc
    raise Unsupported("mint path exceeds the certificate's instruction bound")


def checked_step(index, step, case):
    opcode = step.opcode
    schema = step.schema
    # Arity is taken from the reflected opcode, just as in the emitted image.
    binders = parameters(case)
    effect = (
        f".done (({step.terminal} : Exit).result (balance-({step.cost})))"
        if step.terminal
        else f".next {step.next_journal} {{({step.after}) with gas := balance-({step.cost})}}"
    )
    out = f"""
theorem step{index} {binders} (balance : Nat) :
    instruction cfg {step.journal} {{({step.before}) with gas := balance}} =
      if balance < ({step.minimum}) then stepExhausted {step.journal}
      else .ok ({effect}) := by
  refine opcodeAt cfg _ code _ balance {opcode} _ rfl ?_ ?_
  · change code.get? {step.pc} = some {opcode}
    decide +kernel
  have schema : available cfg {opcode} =
    ⟨{str(schema["available"]).lower()}, {schema["base_gas"]}, {schema["dynamic_gas"]}, {step.inputs}, {step.outputs}⟩ := by rfl
"""
    for fact in step.facts:
        out += "  " + fact + "\n"
    facts = [
        "frame",
        "instructionWithImage",
        "runtimeSize",
        "schema",
        "fork",
        "next",
        "pureOp",
        "EVM.step",
        "EVM.Context.ofEnvironment",
        "Fork.atLeast",
        "Fork.ordinal",
        "writable",
        "selected",
        "Bind.bind",
        "Except.bind",
        *step.defs,
    ]
    facts += [f"h{k}" for k in case]
    facts += [fact.split()[1] for fact in step.facts]
    if step.sentry:
        out += """  refine storeInstruction cfg _ code env _ _ _ _ _ balance fork schema writable ?_ ?_
  · simp [runtimeSize]
  · simp
"""
    else:
        out += f"""  by_cases low : balance < ({step.cost}) <;>
    simp [{", ".join(facts)}, low, Exit.result]
  all_goals first | rfl | simp_all
"""
    return out


def leaf_source(module, name, route, case):
    binders = parameters(case)
    call_args = args(case)
    out = f"import {module}\n{HEADER}\nnamespace {name}\n"
    out += "\n".join(route.definitions)
    prices = [step for step in route.steps if isinstance(step.cost, str)]
    for index, step in enumerate(prices):
        out += f"\ndef price{index} (env : World.Environment) (journal : Journal) : Nat := {step.cost}\n"
    price_names = [f"(price{index} env journal)" for index in range(len(prices))]
    out += f"""
def debits (env : World.Environment) (journal : Journal) : List Debit :=
  {ledger(route, price_names)}
def terminal (env : World.Environment) (journal : Journal) : Exit :=
  {route.exit}
"""
    for index, step in enumerate(route.steps):
        out += checked_step(index, step, case)
    out += f"""
theorem certified {binders} :
    Checked cfg journal (frame code env 0 [] []) (debits env journal) (terminal env journal) := by
  unfold debits {" ".join(f"price{i}" for i in range(len(prices)))}
"""
    for index, step in enumerate(route.steps):
        constructor = "halt" if step.terminal else "next"
        bounded = "(Nat.le_max_right _ _)" if step.sentry else "(Nat.le_refl _)"
        out += f"  {'exact' if step.terminal else 'refine'} .{constructor} {bounded} (step{index} {call_args}){'' if step.terminal else ' ?_'}\n"
    out += f"""
theorem executes {binders} (gas : Nat) :
    (EVM.State.execute 128 cfg journal {{(frame code env 0 [] []) with gas}}).map
      (fun result => {{result with work := 0}}) =
    .ok (settle journal (expected (debits env journal) (terminal env journal) journal gas)) :=
  (certified {call_args}).execute {128 - len(route.steps)} gas
#print axioms executes
end {name}
"""
    return out


def image_source(image, fixture, solar, executable, timeout):
    return (
        f"import {MODULE}\n{HEADER}\n"
        + entry_proof.image_definitions(image)
        + world.configuration(solar, "cancun", fixture, timeout, executable)
        + f"""
def cfg := tvWorldConfig
theorem fork : cfg.fork = .cancun := rfl
theorem runtimeSize : code.size = {len(image)} := by decide +kernel
"""
    )


def prove_image(directory, label, image, fixture, solar, executable, timeout, routes):
    prove = partial(entry_proof.lean, directory, namespace=label)
    module = f"{label}MintImage"
    entry_proof.success(
        prove(
            module,
            image_source(image, fixture, solar, executable, timeout),
            timeout,
            module=True,
        ),
        module,
    )
    for name, route in routes.items():
        case = CASES[name]
        proof = f"{label}Mint{name}"
        checked = entry_proof.success(
            prove(proof, leaf_source(module, name, route, case), timeout, module=True),
            proof,
        )
        entry_proof.check_axioms(checked.splitlines(), 1)


def ledger(route, prices):
    dynamic = iter(prices)
    fees = []
    for step in route.steps:
        cost = str(step.cost) if isinstance(step.cost, int) else next(dynamic)
        minimum = f"(max 2301 {cost})" if step.sentry else cost
        fees.append(f"⟨{minimum},{cost}⟩")
    return list_term(fees)


def bounds_source(name, routes):
    old, new = (f"Old.{name}", f"New.{name}")
    old_prices, new_prices = (
        [step for step in route.steps if isinstance(step.cost, str)] for route in routes
    )
    if len(old_prices) != len(new_prices):
        raise Unsupported(f"{name} has a different number of dynamic charges")
    variables = [f"p{i}" for i in range(len(old_prices))]
    before, after = (ledger(route, variables) for route in routes)
    params = f"({' '.join(variables)} : Nat)" if variables else ""
    out = f"""import OldMint{name}
import NewMint{name}
{HEADER}
private theorem {name}BoundsArithmetic {params} :
    required {after} ≤ required {before} ∧ charged {after} ≤ charged {before} := by
  constructor
  · simp only [required_exact, required_sentry, required_nil]
    omega
  · simp only [charged, List.map_cons, List.map_nil, List.sum_cons, List.sum_nil, Debit.cost]
    omega
theorem {name}Budget (env : World.Environment) (journal : Journal) :
    required ({new}.debits env journal) ≤ required ({old}.debits env journal) ∧
    charged ({new}.debits env journal) ≤ charged ({old}.debits env journal) := by
  unfold {old}.debits {new}.debits
"""
    for index in range(len(variables)):
        out += f"  rw [show {new}.price{index} env journal = {old}.price{index} env journal from rfl]\n"
    out += (
        f"  exact {name}BoundsArithmetic "
        + " ".join(
            f"({old}.price{index} env journal)" for index in range(len(variables))
        )
        + "\n"
    )
    memory_defs = [
        f"{side}.{definition.split()[1]}"
        for route, side in zip(routes, (old, new), strict=True)
        for definition in route.definitions
    ]
    simplify = ", ".join([f"{old}.terminal", f"{new}.terminal", *memory_defs])
    out += f"""
theorem {name}Terminal (env : World.Environment) (journal : Journal) :
    {old}.terminal env journal = {new}.terminal env journal := by
  simp only [{simplify}] <;> rfl
"""
    return out


def comparison_source(modules):
    out = entry_proof.import_modules(modules) + HEADER
    for name, label in (("before", "Old"), ("after", "New")):
        out += f"""
def {name} (env : World.Environment) (journal : Journal) (gas : Nat) : Run :=
  (execute 128 {label}.cfg journal {{(frame {label}.code env 0 [] []) with gas}}).map
    (fun result => {{result with work := 0}})

def {name}Plan (env : World.Environment) (journal : Journal) : Plan :=
"""
        for index, (case, guard) in enumerate(zip(CASES, GUARDS.values())):
            branch = "if" if index == 0 else "else if"
            out += f"  {branch} {guard} ≠ 0 then\n"
            out += f"    ({label}.{case}.debits env journal,{label}.{case}.terminal env journal)\n"
        out += f"  else ({label}.Success.debits env journal,{label}.Success.terminal env journal)\n"
    for name, case in CASES.items():
        call_args = args(case)
        out += f"""
private theorem {name}Compared {parameters(case)} (gas : Nat) :
    Compared (before env journal gas) (after env journal gas) journal gas
      (Old.{name}.debits env journal, Old.{name}.terminal env journal)
      (New.{name}.debits env journal, New.{name}.terminal env journal) :=
  ⟨Old.{name}.executes {call_args} gas, New.{name}.executes {call_args} gas,
    ({name}Budget env journal).1, ({name}Budget env journal).2, {name}Terminal env journal⟩
"""
    out += f"""
theorem mint_layout_exits {parameters({})} (gas : Nat) :
    Compared (before env journal gas) (after env journal gas) journal gas
      (beforePlan env journal) (afterPlan env journal) := by
"""
    for depth, (name, condition) in enumerate(zip(CASES, GUARDS), 1):
        indent = "  " * depth
        prefix = indent if depth == 1 else indent[:-2] + "· "
        out += prefix + f"by_cases h{condition} : {GUARDS[condition]} ≠ 0\n"
        case = CASES[name]
        hs = ", ".join(
            f"ite_eq_{'left' if taken else 'right'} h{k}" for k, taken in case.items()
        )
        out += indent + f"· simp only [beforePlan,afterPlan,{hs}]\n"
        for prior in list(case)[:-1]:
            out += (
                indent
                + f"  have h{prior} : {GUARDS[prior]} = 0 := by simpa using h{prior}\n"
            )
        out += indent + f"  exact {name}Compared {args(case)} gas\n"
    indent = "  " * len(GUARDS)
    hs = ", ".join(f"ite_eq_right h{k}" for k in GUARDS)
    out += indent + f"· simp only [beforePlan,afterPlan,{hs}]\n"
    for key, guard in GUARDS.items():
        out += indent + f"  have h{key} : {guard} = 0 := by simpa using h{key}\n"
    out += indent + f"  exact SuccessCompared {args(CASES['Success'])} gas\n"
    out += f"""
theorem mint_layout_refines {parameters({})} (gas : Nat) :
    GasRefines (before env journal gas) (after env journal gas) :=
  (mint_layout_exits env journal writable selected gas).refinement
#print axioms mint_layout_exits
#print axioms mint_layout_refines
"""
    return out


def certify(directory, images, fixture, solar, executable, timeout):
    table = {row["opcode"]: row for row in native.catalog(executable, timeout)["evmir"]}
    routes = {
        name: [propose(image, table, case) for image in images]
        for name, case in CASES.items()
    }
    for index, (side, image) in enumerate(zip(("Old", "New"), images, strict=True)):
        prove_image(
            directory,
            side,
            image,
            fixture,
            solar,
            executable,
            timeout,
            {case: sides[index] for case, sides in routes.items()},
        )
    bounds = []
    for name, sides in routes.items():
        module = f"Mint{name}Budget"
        entry_proof.success(
            entry_proof.lean(
                directory, module, bounds_source(name, sides), timeout, module=True
            ),
            module,
        )
        bounds.append(module)
    checked = entry_proof.success(
        entry_proof.lean(
            directory,
            "MintComparison",
            comparison_source(bounds),
            timeout,
            module=True,
        ),
        "mint gas refinement",
    )
    entry_proof.check_axioms(checked.splitlines(), 2)
    return routes


def replay(directory, images, fixture, executable, timeout):
    """Check the two admission boundaries in both interpreters."""
    if fixture["static"] or fixture["symbols"]:
        raise Unsupported("mint replay requires concrete storage in a writable frame")
    if bytes(fixture["calldata"][:4]) != SELECTOR.to_bytes(4, "big"):
        raise Unsupported("mint replay requires the mint(address,uint256) selector")
    warm = ", ".join(world.warm_addresses(fixture, "cancun"))
    slots = ", ".join(f"({a},{k})" for a, k in fixture["warm_slots"])
    source = (
        "import MintComparison\n"
        + HEADER
        + f"""
def env : World.Environment := {world.environment_expression(fixture)}
def accounts : List Account := {world.accounts_expression(fixture)}
def journal : Journal := ⟨accounts,accounts,[{warm}],[{slots}],[],0⟩

#eval do
  let old := beforePlan env journal
  let new := afterPlan env journal
  let prior := required old.1
  let optimized := required new.1
  IO.println s!"{{prior}} {{optimized}}"
  for gas in [optimized-1,optimized,prior-1,prior,{fixture["gas"]}].eraseDups do
    let left := EVM.State.describe Old.cfg .exact (before env journal gas)
    let right := EVM.State.describe New.cfg .exact (after env journal gas)
    let oldPlan := settle journal (expected old.1 old.2 journal gas)
    let newPlan := settle journal (expected new.1 new.2 journal gas)
    unless left == EVM.State.describe Old.cfg .exact (.ok oldPlan) &&
        right == EVM.State.describe New.cfg .exact (.ok newPlan) do
      throw (IO.userError s!"mint budget prediction differs from bytecode at {{gas}}")
    IO.println s!"{{gas}}\\t{{left}}\\t{{right}}"
"""
    )
    checked = entry_proof.success(
        entry_proof.lean(directory, "MintReplay", source, timeout), "mint budget replay"
    )
    threshold, *rows = checked.splitlines()
    old, new = map(int, threshold.split())
    samples = []
    for row in rows:
        gas, before, after = row.split("\t")
        budget = int(gas)
        programs = [
            Program(
                ("world-entry", "evmir"),
                [],
                "",
                "",
                gas="exact",
                evm_version="cancun",
                world={
                    **world.with_runtime(fixture, image),
                    "gas": budget,
                    "gas_policy": "exact",
                },
            )
            for image in images
        ]
        # The raw helper gets one runtime through the world. Bind and run each side
        # independently before comparing its actual receipt to Lean's observation.
        observations = [
            json.loads(
                native.replay(
                    program,
                    program,
                    [],
                    executable=executable,
                    timeout=timeout,
                    raw_runtime=True,
                )["observations"][0]
            )
            for program in programs
        ]
        if observations != [json.loads(before), json.loads(after)]:
            raise Inconclusive(f"independent mint replay differs at gas {budget}")
        samples.append({"gas": budget, "observations": observations})
    return {"minimum_gas_before": old, "minimum_gas_after": new, "samples": samples}


def check(paths, fixture_path, solar, executable, version, timeout):
    if version != "cancun":
        raise Unsupported("--mint-layout requires Cancun")
    if any(
        path.suffix != ".evmir" or path.stat().st_size > 1024 * 1024 for path in paths
    ):
        raise Unsupported("mint layout requires .evmir files of at most 1024 KiB")
    fixture = world.read(fixture_path, solar, None)
    sources = [path.read_text() for path in paths]
    images = [
        world.runtime(source, "evmir", solar, version, timeout) for source in sources
    ]
    with tempfile.TemporaryDirectory(prefix="solar-tv-mint-layout-") as folder:
        directory = Path(folder)
        routes = certify(directory, images, fixture, solar, executable, timeout)
        witness = replay(directory, images, fixture, executable, timeout)
    pair = tuple(
        Program(
            ("world-entry", "evmir"),
            [],
            "",
            "",
            canonical_ir=source,
            gas="refine",
            evm_version=version,
            scope="mint-layout",
            selector=f"{SELECTOR:#010x}",
            import_evidence={
                "kind": "symbolic-mint-budget",
                "runtime_sha256": hashlib.sha256(image).hexdigest(),
                "path_steps": {
                    case: len(sides[index].steps) for case, sides in routes.items()
                },
            },
        )
        for index, (source, image) in enumerate(zip(sources, images, strict=True))
    )
    return audit.Result(
        "verified",
        stdout=(
            "verified: mint gas refinement over symbolic calldata and storage\n"
            f"replay minimum gas: {witness['minimum_gas_before']} → {witness['minimum_gas_after']}\n"
        ),
        program=pair[0],
        pair=pair,
        witness=witness,
    )
