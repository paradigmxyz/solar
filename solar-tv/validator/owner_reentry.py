"""Certify a Morpho getter executed by imported MIR in a nested EVM frame."""

from dataclasses import dataclass

from . import entry_proof, mir_state, world
from .callers import require
from .check import build_model

MODULE = "SolarTV.MIR.Transforms.ReentrantOwner"
NAME = "MorphoReentrantOwner"
SELECTOR = 0x8DA5CB5B
OPS = (
    ("push0", 0),
    ("sload", None),
    ("push1", 96),
    ("shl", None),
    ("push1", 96),
    ("shr", None),
    ("push0", 0),
    ("mstore", None),
    ("push1", 32),
    ("push0", 0),
    ("return", None),
)
LOADED = "(loaded env journal)"
SHIFTED = f"(Evm.shl 96 {LOADED})"
OWNER = "(owner env journal)"
WARMED = "(Execution.warmSlot journal env.address 0)"
MEMORY = f"(Execution.wordBytes {OWNER})"


@dataclass
class Step:
    pc: int
    opcode: int
    gas: int
    dynamic: bool
    stack: tuple
    warm: bool
    memory: bool
    schema: tuple
    immediate: int | None

    def balance(self):
        terms = ["surplus", str(self.gas)]
        if self.dynamic:
            terms.append("price env journal")
        return f"({' + '.join(terms)})"

    def frame(self):
        stack = "[" + ", ".join(map(str, self.stack)) + "]"
        return f"frameAt env {self.balance()} {self.pc} {stack} {MEMORY if self.memory else '[]'}"

    @property
    def world(self):
        return WARMED if self.warm else "journal"


def steps(runtime, decoded, catalog):
    points = list(decoded)
    matches = [
        points[i : i + len(OPS)]
        for i in range(len(points) - len(OPS) + 1)
        if tuple(decoded[pc] for pc in points[i : i + len(OPS)]) == OPS
    ]
    require(len(matches) == 1, "expected one emitted owner getter")
    stack = [SELECTOR]
    charged = []
    for pc, (name, immediate) in zip(matches[0], OPS, strict=True):
        opcode = runtime[pc]
        row = catalog[name]
        pricing = row["forks"]["cancun"]
        fixed = pricing["base_gas"] + (3 if name == "mstore" else 0)
        if name == "sload":
            fixed = 0
        charged.append(
            (
                pc,
                opcode,
                fixed,
                (
                    pricing["available"],
                    pricing["base_gas"],
                    pricing["dynamic_gas"],
                    *row["stack"],
                ),
                immediate if name == "push1" else None,
            )
        )
    remaining = sum(row[2] for row in charged)
    dynamic, memory = True, False
    result = []
    for (name, immediate), (pc, opcode, fixed, schema, pushed) in zip(
        OPS, charged, strict=True
    ):
        result.append(
            Step(
                pc,
                opcode,
                remaining,
                dynamic,
                tuple(stack),
                not dynamic,
                memory,
                schema,
                pushed,
            )
        )
        remaining -= fixed
        if immediate is not None:
            stack.insert(0, immediate)
        elif name == "sload":
            stack[0] = LOADED
            dynamic = False
        elif name == "shl":
            stack[:2] = [SHIFTED]
        elif name == "shr":
            stack[:2] = [OWNER]
        elif name == "mstore":
            del stack[:2]
            memory = True
    return result


def source(rows, config, tables, catalog):
    start = rows[0]
    before, after = [
        mir_state.function_body(table["owner"], catalog) for table in tables
    ]
    out = f"""import EntryImage
import {MODULE}
{entry_proof.HEADER}
open MIR.ReentrantOwner
set_option linter.unusedSimpArgs false
{config}
def ownerPC : Nat := {start.pc}
def ownerSelector : Word := {SELECTOR}
def ownerSteps : Nat := {len(rows)}
def ownerFixed : Nat := {start.gas}
def originalOwner : Nat × MIR.State.Function := {before}
def loweredOwner : Nat × MIR.State.Function := {after}
theorem originalShape : originalOwner.2 = ownerBody := rfl
theorem loweredShape : loweredOwner.2 = ownerBody := rfl

def frameAt (env : World.Environment) (gas pc : Nat) (stack : List Word)
    (memory : List Execution.Byte) : EVM.State.Frame :=
  ⟨env,code.bytes,pc,stack,memory,[],gas⟩

def reply (env : World.Environment) (journal : Execution.Journal) (surplus work : Nat) :
    EVM.State.Result :=
  ⟨.success, {MEMORY}, {WARMED}, surplus,work⟩
"""
    for index, row in enumerate(rows):
        final = index == len(rows) - 1
        if final:
            result = ".done (reply env journal surplus 0)"
        else:
            nxt = rows[index + 1]
            result = f".next {nxt.world} ({nxt.frame()})"
        schema = ", ".join(str(field).lower() for field in row.schema)
        out += f"""
theorem ownerStep{index} (env : World.Environment) (journal : Execution.Journal) (surplus : Nat) :
    EVM.State.instruction tvWorldConfig {row.world} ({row.frame()}) = .ok ({result}) := by
  have opcode : code.get? {row.pc} = some {row.opcode} := by decide +kernel
  have reachable : {row.pc} < code.bytes.length := by
    rw [EVM.State.CodeChunks.size_eq]
    decide +kernel
  have schema : EVM.State.available tvWorldConfig {row.opcode} = ⟨{schema}⟩ := rfl
"""
        facts = ["schema", "paid", "Bind.bind", "Except.bind"]
        fee = (
            "price env journal"
            if row.opcode == 0x54
            else str(row.gas - (rows[index + 1].gas if not final else 0))
        )
        out += f"  have paid : ¬ {row.balance()} < {fee} := by omega\n"
        if row.immediate is not None:
            out += f"  have operand : code.immediate {row.pc} {row.opcode - 0x5F} = {row.immediate} := by decide +kernel\n"
            facts.append("operand")
        if row.opcode in (0x52, 0xF3):
            out += f"  have data := outputMemory {OWNER}\n"
            facts.extend(
                [
                    "data.1",
                    "data.2.1",
                    "data.2.2.1",
                    "data.2.2.2",
                    "EVM.State.withMemory",
                    "EVM.State.memoryCharge",
                    "EVM.State.memoryCost",
                    "Execution.requiredWords",
                    "Execution.memoryWords",
                    "Execution.wordBytes_length",
                ]
            )
        if row.opcode == 0x54:
            out += "  have fork : tvWorldConfig.fork = .cancun := rfl\n"
            facts.extend(["fork", "price", "loaded"])
        elif row.opcode == 0x1C:
            out += f"  have masked : Evm.shr 96#256 (Evm.shl 96#256 {LOADED}) = {OWNER} := canon _\n"
            facts.append("masked")
        remaining = "exact data.2.1" if row.opcode == 0x52 else "omega"
        out += f"""  simp only [frameAt, EVM.State.instruction, ← EVM.State.CodeChunks.get_eq,
    opcode, Option.getD_some]
  unfold EVM.State.instructionWithImage
  rw [ite_eq_right (Nat.not_le_of_gt reachable)]
  simp [← EVM.State.CodeChunks.immediate_eq,
    EVM.State.pureOp, EVM.State.next, EVM.step,
    Fork.atLeast, Fork.ordinal, reply, {", ".join(facts)}] <;> {remaining}
"""
    out += f"""
theorem bodyRun (env : World.Environment) (journal : Execution.Journal) (surplus extra : Nat) :
    EVM.State.run (extra+ownerSteps) tvWorldConfig journal ({start.frame()}) =
      .ok (reply env journal surplus ownerSteps) := by
  unfold ownerSteps
  simp only [
"""
    for index in range(len(rows)):
        kind = "run_done" if index == len(rows) - 1 else "run_next"
        left = len(rows) - index - 1
        budget = f"(extra+{left})" if left else "extra"
        out += (
            f"    EVM.State.{kind} {budget} (ownerStep{index} env journal surplus),\n"
        )
    out += """    Bind.bind, Except.bind, Pure.pure, Except.pure]
  rfl

def route (entry : Nat × MIR.State.Function) : EVM.State.Reentry.Route := fun work _ journal frame =>
  let fee := ownerFixed + price frame.environment journal
  if frame.pc = ownerPC ∧ frame.stack = [ownerSelector] ∧ frame.memory = [] ∧
      frame.returnData = [] ∧ fee ≤ frame.gas ∧ ownerSteps ≤ work ∧ frame.code = code.bytes then
    some (finishOwner tvWorldConfig entry.1 fee ownerSteps entry.2 journal frame)
  else none

theorem routeSound (entry : Nat × MIR.State.Function) (shape : entry.2 = ownerBody) :
    EVM.State.Reentry.Sound tvWorldConfig (route entry) := by
  intro work execute _ journal frame result chosen
  unfold route at chosen
  dsimp only at chosen
  split at chosen
  · rename_i facts
    obtain ⟨pc,stack,memory,returndata,paid,steps,image⟩ := facts
    simp only [Option.some.injEq] at chosen
    rw [← chosen, shape, finish_eq]
    have frameEq : frameAt frame.environment
        (frame.gas-ownerFixed-price frame.environment journal + ownerFixed + price frame.environment journal)
        ownerPC [ownerSelector] [] = frame := by
      have gas : frame.gas-ownerFixed-price frame.environment journal + ownerFixed +
          price frame.environment journal = frame.gas := by omega
      cases frame
      simp_all [frameAt]
    have fuelEq : work+1-ownerSteps+ownerSteps = work+1 := by omega
    have checked := bodyRun frame.environment journal (frame.gas-ownerFixed-price frame.environment journal)
      (work+1-ownerSteps)
    change EVM.State.run _ _ _ (frameAt frame.environment (_ + ownerFixed + price frame.environment journal)
      ownerPC [ownerSelector] []) = _ at checked
    rw [fuelEq, frameEq] at checked
    rw [checked]
    simp [reply]
    omega
  · contradiction

theorem nested (work : Nat) (journal : Execution.Journal) (frame : EVM.State.Frame) :
    EVM.State.Reentry.runWith tvWorldConfig (route originalOwner) work journal frame =
    EVM.State.Reentry.runWith tvWorldConfig (route loweredOwner) work journal frame := by
  rw [EVM.State.Reentry.correspondence _ _ (routeSound _ originalShape),
    EVM.State.Reentry.correspondence _ _ (routeSound _ loweredShape)]

#print axioms nested
"""
    return out


def certify(directory, rows, configuration, tables, catalog, timeout):
    built = build_model(timeout, MODULE)
    entry_proof.success(built, MODULE)
    checked = entry_proof.lean(
        directory,
        NAME,
        source(rows, configuration, tables, catalog),
        timeout,
        module=True,
    )
    output = entry_proof.success(checked, NAME)
    entry_proof.check_axioms(output.splitlines(), 1)
    return {
        "proof": f"kernel/{NAME}.lean",
        "pc": rows[0].pc,
        "fixed_gas": rows[0].gas,
        "scope": "captured owner getter MIR at its reached bytecode entry",
    }


def replay_source(state):
    address = state["address"]
    seed = {
        **state,
        "accounts": [
            {**account, "code": []} if account["address"] == address else account
            for account in state["accounts"]
        ],
    }
    warm = ", ".join(world.warm_addresses(state, "cancun"))
    slots = ", ".join(f"({owner},{key})" for owner, key in state["warm_slots"])
    return f"""import {NAME}
open SolarTV EvmRules
open EVM.State.Reentry
def accounts : List Execution.Account :=
  ({world.accounts_expression(seed)} : List Execution.Account).map fun a =>
    if a.address = {address} then {{a with code := code.bytes}} else a
def checkpoint : Execution.Journal := ⟨accounts,accounts,[{warm}],[{slots}],[],0⟩
def start : EVM.State.Frame :=
  frameAt {world.environment_expression(state)} {state["gas"]} 0 [] []
def observe (pick : Route) : String :=
  EVM.State.describe tvWorldConfig .exact (executeWith tvWorldConfig pick 2048 checkpoint start)
def broken : Route := fun work step journal frame =>
  (route originalOwner work step journal frame).map fun result => do
    let result ← result
    return {{result with data := Execution.wordBytes 0x78}}
#eval IO.println ("[" ++ observe (route originalOwner) ++ "," ++
  observe (route loweredOwner) ++ "," ++ observe broken ++ "]")
"""
