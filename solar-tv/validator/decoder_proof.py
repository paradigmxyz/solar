"""Propose the canonical-calldata path through each emitted ABI decoder."""

from dataclasses import dataclass, replace
from itertools import pairwise

from . import metered_prefix
from .callers import require
from .dispatch_proof import Instruction

# The market words precede the scalar parameters. The locations listed here
# are the address parameters checked by each external entry wrapper.
HEADS = {
    "supply": (True, 3, 228),
    "repay": (True, 3, 228),
    "supplyCollateral": (True, 2, 196),
    "liquidate": (True, 3, 164),
    "flashLoan": (False, 2, 4),
}
TAIL = (0x5B, 0x80, 0x35, 0x90, 0x60, 0x01)


@dataclass
class Decoder:
    instructions: list
    pc: int
    stack: list
    memory: list
    gas: int
    offset_at: int
    offset: int
    addresses: tuple

    @property
    def payload(self):
        return self.offset + 36

    def accepts(self, data):
        return (
            len(data) >= self.payload
            and read_word(data, self.offset_at) == self.offset
            and read_word(data, self.offset + 4) <= len(data) - self.payload
            and all(read_word(data, at) < 2**160 for at in self.addresses)
        )

    def replay_state(self, data):
        def resolve(value):
            if isinstance(value, int):
                return value
            kind, offset = value
            require(kind == "load", "unexpected live ABI decoder value")
            return read_word(data, offset)

        return (
            [str(resolve(value)) for value in self.stack],
            list(b"".join(resolve(value).to_bytes(32, "big") for value in self.memory)),
        )

    def suffix(self, pc):
        """Start the decoder at a checked frame after a preceding prefix."""
        at = [i for i, step in enumerate(self.instructions) if step.inst.pc == pc]
        require(len(at) == 1, "ABI decoder cut must select one instruction")
        first = self.instructions[at[0]]
        paid = first.inst.gas
        steps = [
            replace(step, inst=replace(step.inst, gas=step.inst.gas - paid))
            for step in self.instructions[at[0] :]
        ]
        return replace(self, instructions=steps, gas=self.gas - paid)


def read_word(data, at):
    return int.from_bytes(bytes(data[at : at + 32]).ljust(32, b"\0"), "big")


@dataclass
class Step:
    inst: Instruction
    memory: list
    growth: int = 0
    fact: str | int = ""


def word(value):
    if isinstance(value, int):
        return str(value)
    kind, n = value
    if kind == "load":
        return f"(Execution.readWord env.input {n})"
    require(kind == "size", "unknown ABI decoder word")
    size = "env.input.length" if n == 0 else f"(env.input.length - {n})"
    return f"(BitVec.ofNat 256 {size})"


def words(values):
    return "[" + ", ".join(map(word, values)) + "]"


def compute(opcode, a, b, addresses, payload):
    if opcode == 0x01 and isinstance(a, int) and isinstance(b, int):
        require(a + b < 2**256, "overflowing ABI decoder constant addition")
        return a + b, ""
    if opcode == 0x03 and a == ("size", 0) and isinstance(b, int) and b <= payload:
        return ("size", b), f"sizeSub env.input.length {b} small (by omega)"
    if (
        opcode == 0x10
        and isinstance(a, tuple)
        and a[0] == "size"
        and isinstance(b, int)
    ):
        n = a[1]
        require(n + b <= payload, "unexpected ABI decoder length guard")
        if n == 0:
            return 0, f"ltSize env.input.length {b} small (by omega)"
        return 0, f"ltSize (env.input.length - {n}) {b} (by omega) (by omega)"
    if opcode == 0x11 and a == ("load", payload - 32) and b == ("size", payload):
        return (
            0,
            f"fitsSlice (Execution.readWord env.input {payload - 32}) (env.input.length - {payload}) (by omega) slice",
        )
    if opcode == 0x1C and a == 160 and isinstance(b, tuple) and b[0] == "load":
        require(b[1] in addresses, "unexpected ABI decoder address guard")
        return (
            0,
            f"addressShift (Execution.readWord env.input {b[1]}) (addresses {b[1]} (by decide))",
        )
    if opcode == 0x17 and a == b == 0:
        return 0, ""
    require(False, f"unexpected ABI decoder operation {opcode:#x} on {a}, {b}")


def plan(runtime, site, pcs, catalog):
    market, arity, parameter_address = HEADS[site.name]
    head_words = (5 if market else 0) + arity + 1
    offset = 32 * head_words
    offset_at = 4 + offset - 32
    payload = offset + 36
    addresses = (4, 36, 68, 100, parameter_address) if market else (parameter_address,)
    pcs = pcs[pcs.index(site.abi_pc) :]
    stack, memory, path = [site.selector], [], []
    gas = 0
    walked = []
    for pc, following in pairwise(pcs):
        opcode = runtime[pc]
        spec = catalog[opcode]
        pricing = spec["forks"]["cancun"]
        fee = pricing["base_gas"]
        schema = (
            pricing["available"],
            fee,
            pricing["dynamic_gas"],
            *(spec["stack"] or (0, 0)),
        )
        inst = Instruction(pc, opcode, gas, schema, stack.copy(), None, None)
        step = Step(inst, memory.copy())
        path.append(step)
        walked.append(opcode)
        next_pc = pc + 1
        if 0x60 <= opcode <= 0x7F:
            width = opcode - 0x5F
            next_pc += width
            inst.immediate = int.from_bytes(runtime[pc + 1 : next_pc], "big")
            stack.insert(0, inst.immediate)
        elif opcode == 0x5F:
            stack.insert(0, 0)
        elif opcode == 0x36:
            stack.insert(0, ("size", 0))
        elif opcode == 0x35:
            address = stack[0]
            require(isinstance(address, int), "dynamic ABI decoder address")
            if address == offset_at:
                stack[0] = offset
                step.fact = "offsetWord"
            else:
                stack[0] = ("load", address)
        elif 0x80 <= opcode <= 0x8F:
            stack.insert(0, stack[opcode - 0x80])
        elif 0x90 <= opcode <= 0x9F:
            k = opcode - 0x8F
            stack[0], stack[k] = stack[k], stack[0]
        elif opcode in {0x01, 0x03, 0x10, 0x11, 0x17, 0x1C}:
            value, step.fact = compute(opcode, *stack[:2], addresses, payload)
            stack[:2] = [value]
        elif opcode == 0x52:
            address, value = stack[:2]
            del stack[:2]
            require(
                isinstance(address, int) and address % 32 == 0 and address <= 256,
                "unexpected ABI decoder store address",
            )
            index = address // 32
            growth = max(0, index + 1 - len(memory))
            step.growth = 3 * growth  # The checked arena uses at most nine words.
            memory.extend([0] * growth)
            memory[index] = value
            step.fact = index
        elif opcode == 0x56:
            inst.target = next_pc = stack.pop(0)
            require(isinstance(next_pc, int), "unknown ABI return destination")
        elif opcode == 0x57:
            require(stack[1] == 0, "unexpected ABI decoder branch")
            del stack[:2]
        else:
            require(opcode == 0x5B, f"unmodeled ABI decoder opcode {opcode:#x}")
        gas += fee + step.growth
        require(next_pc == following, "ABI decoder path disagrees with raw replay")
        if tuple(walked[-len(TAIL) :]) == TAIL and path[-2].inst.immediate == 32:
            expected = [
                payload,
                ("load", offset + 4),
                128 if market else ("load", 36),
                site.selector,
            ]
            expected_memory = (
                [0, 0, 288, 0, *[("load", at) for at in (4, 36, 68, 100, 132)]]
                if market
                else [0, 0, 128]
            )
            require(
                stack == expected and memory == expected_memory,
                "ABI decoder live-in layout changed",
            )
            return Decoder(
                path, following, stack, memory, gas, offset_at, offset, addresses
            )
    require(False, "missing physical ABI decoder exit")


def frame(pc, stack, memory, fee):
    return f"decoderFrame env (surplus + {fee}) {pc} {words(stack)} {words(memory)}"


def input_binders(decoder):
    required = decoder.payload
    addresses = list(decoder.addresses)
    return f"""(env : World.Environment) (journal : Execution.Journal) (surplus : Nat)
    (room : {required} ≤ env.input.length) (small : env.input.length < 2^256)
    (offsetWord : Execution.readWord env.input {decoder.offset_at} = {decoder.offset})
    (slice : (Execution.readWord env.input {required - 32}).toNat ≤ env.input.length - {required})
    (addresses : ∀ i, i ∈ ({addresses} : List Nat) → (Execution.readWord env.input i).toNat < 2^160)"""


def entry_alias(helper, fuel, start, proof):
    return f"""
abbrev physicalStart := {start}
theorem entry_concrete :
    runBytes {helper} 2048 tvWorldConfig initial =
      Execution.Trace.run (chunkedByteStep {helper} tvWorldConfig code) {fuel} physicalStart :=
  {proof}
"""


def steps_source(decoder, dispatch, helper, binders, args):
    first = decoder.instructions[0]
    require(
        first.inst.pc == dispatch.pc and first.inst.stack == dispatch.stack,
        "ABI decoder prefix and suffix disagree on the entry frame",
    )
    out = """
open MIR.TransferFromABI

def decoderFrame (env : World.Environment) (gas pc : Nat) (stack memory : List Word) : EVM.State.Frame :=
  ⟨env,code.bytes,pc,stack,wordArena memory,[],gas⟩
"""
    pcs = [inst.pc for inst in reversed(dispatch.instructions)]
    fuel = 2048 - len(dispatch.instructions)
    debits = []
    for index, step in enumerate(decoder.instructions):
        inst = step.inst
        remaining = decoder.gas - inst.gas
        after = (
            decoder.instructions[index + 1]
            if index + 1 < len(decoder.instructions)
            else None
        )
        after_pc, after_stack, after_memory, after_gas = (
            (
                after.inst.pc,
                after.inst.stack,
                after.memory,
                decoder.gas - after.inst.gas,
            )
            if after
            else (decoder.pc, decoder.stack, decoder.memory, 0)
        )
        after_pcs = [inst.pc, *pcs]
        name = f"decodeStep{index}"
        charge = remaining - after_gas
        zero = f"decoderFrame env 0 {inst.pc} {words(inst.stack)} {words(step.memory)}"
        next_zero = (
            f"decoderFrame env 0 {after_pc} {words(after_stack)} {words(after_memory)}"
        )
        out += metered_prefix.debit(name, binders, zero, next_zero, inst.opcode, charge)
        out += f"""
  have schema : EVM.State.available tvWorldConfig {inst.opcode} =
    ⟨{", ".join(str(value).lower() for value in inst.schema)}⟩ := by rfl
"""
        facts = ["schema", "unpaid"]
        if inst.immediate is not None:
            out += f"  have immediate : code.immediate {inst.pc} {inst.opcode - 0x5F} = {inst.immediate} := by decide +kernel\n"
            facts.append("immediate")
        if inst.target is not None:
            out += (
                f"  have target : {inst.target} ∈ code.jumps 0 := by decide +kernel\n"
            )
            facts.append("target")
        if inst.opcode == 0x52:
            slot = step.fact
            memory = words(step.memory)
            out += f"""
  have charge : EVM.State.memoryCharge (wordArena {memory}) {slot * 32} 32 = {step.growth} := by
    change EVM.State.memoryCharge (wordArena {memory}) (32 * {slot}) 32 = _
    rw [EVM.State.memoryCharge,requiredWordStore,Execution.memoryWords,wordArena_length]
    simp only [List.length_cons,List.length_nil] <;> decide
  have extended : Execution.expand (wordArena {memory}) {slot * 32} 32 =
      .ok (wordArena (activeWords {memory} {slot})) :=
    expandArena {memory} {slot} (by change 32 * max {len(step.memory)} ({slot}+1) ≤ 65536; decide)
  have stored : Execution.writeBytes (wordArena (activeWords {memory} {slot})) {slot * 32}
      (Execution.wordBytes {word(inst.stack[1])}) = wordArena (storeArena {memory} {slot} {word(inst.stack[1])}) :=
    writeArena {memory} {slot} {word(inst.stack[1])}
  simp at charge extended stored
"""
            facts.extend(
                (
                    "EVM.State.withMemory",
                    "charge",
                    "extended",
                    "stored",
                    "Except.bind",
                    "Bind.bind",
                )
            )
        elif step.fact == "offsetWord":
            facts.append(step.fact)
        elif step.fact:
            out += f"  have value := {step.fact}\n"
            facts.append("value")
        close = (
            "exact value" if step.fact and inst.opcode not in (0x52, 0x35) else "rfl"
        )
        out += f"""  by_cases unpaid : balance < {charge} <;>
    simp [decoderFrame,EVM.State.instructionWithImage,dispatch_size,
    EVM.State.next,EVM.step,EVM.Context.ofEnvironment,EVM.State.pureOp,
    Fork.atLeast,Fork.ordinal,{",".join(facts)}] <;> {close}
"""
        debit = metered_prefix.Debit(name, inst.pc, inst.opcode, charge, next_zero)
        if helper is not None:
            out += metered_prefix.paid_step(
                debit,
                binders,
                args,
                helper,
                fuel - index - 1,
                frame(inst.pc, inst.stack, step.memory, remaining),
                frame(after_pc, after_stack, after_memory, after_gas),
                pcs,
                f"surplus+{remaining}",
                f"surplus+{after_gas}",
                "decoderFrame",
            )
        debits.append(debit)
        pcs = after_pcs
    out += metered_prefix.certify(
        "metered_decoder",
        binders,
        args,
        decoder.gas,
        f"decoderFrame env 0 {dispatch.pc} {words(dispatch.stack)} {words(first.memory)}",
        f"decoderFrame env 0 {decoder.pc} {words(decoder.stack)} {words(decoder.memory)}",
        debits,
    )
    return out, pcs


def source(decoder, dispatch, helper, concrete):
    required = decoder.payload
    addresses = list(decoder.addresses)
    binders = input_binders(decoder)
    args = "env journal surplus room small offsetWord slice addresses"
    out, pcs = steps_source(decoder, dispatch, helper, binders, args)
    fuel = 2048 - len(dispatch.instructions)
    width = len(decoder.instructions)
    market = decoder.memory[4:]
    first_parameter = 164 if market else 4
    parameters = f"""(parameterWidths caller).zipIdx.map
      (fun (bits,i) => (bits, Execution.readWord env.input ({first_parameter} + 32*i)))"""
    guards = "\n".join(
        f"  have clean{i} := addresses {i} (by decide)" for i in addresses
    )
    guard_facts = ",".join(f"clean{i}" for i in addresses)
    arena = (
        f"marketEntryArena caller journal {' '.join(map(word, market))} parameters _ _ (by decide)"
        if market
        else "flashEntryArena journal parameters _ _"
    )
    out += f"""
def decodedFrame (env : World.Environment) (journal : Execution.Journal) (surplus : Nat) : ByteCut :=
  ⟨{frame(decoder.pc, decoder.stack, decoder.memory, 0)},journal,[],[],{pcs}⟩

theorem decoded_memory (env : World.Environment) (journal : Execution.Journal) (surplus : Nat)
    (parameters : List (Nat × Word)) :
    (entryFrame caller journal {words(market)} parameters
      {required} (Execution.readWord env.input {required - 32}).toNat).map (·.memory) =
      .ok (decodedFrame env journal surplus).frame.memory :=
  {arena}

theorem typed_abi_frame {binders} (cfg : MIR.State.Config)
    (selected : MIR.TransferFromDispatch.selector env.input = selector) :
    decode selector caller {{cfg with environment := env}} journal =
      entryFrame caller journal {words(market)} ({parameters})
        {required} (Execution.readWord env.input {required - 32}).toNat := by
  have head : ¬ env.input.length < {decoder.offset_at + 32} := by omega
  have fits : ¬ env.input.length < {required} := by omega
  have length : ¬ env.input.length - {required} < (Execution.readWord env.input {required - 32}).toNat :=
    Nat.not_lt_of_ge slice
{guards}
  simp [decode,caller,hasMarket,parameterWidths,canonicalAddresses,List.range_succ,
    MIR.TransferFromDispatch.selector_word,selected,offsetWord,head,fits,length,{guard_facts}]

theorem decoded_mir_memory {binders} (cfg : MIR.State.Config)
    (selected : MIR.TransferFromDispatch.selector env.input = selector) :
    (decode selector caller {{cfg with environment := env}} journal).map (·.memory) =
      .ok (decodedFrame env journal surplus).frame.memory := by
  rw [typed_abi_frame {args} cfg selected]
  exact decoded_memory env journal surplus _

theorem symbolic_decoder {binders} :
    Execution.Trace.segment (chunkedByteStep {helper} tvWorldConfig code) {width} {fuel}
      (dispatched env journal (surplus + {decoder.gas})) = .ok (.inr (decodedFrame env journal surplus)) := by
"""
    for index in range(width):
        out += f"  apply Execution.Trace.segment_next {width - index - 1} {fuel - index - 1} (decodeStep{index} {args})\n"
    out += f"""  rfl

theorem resume_decoder {binders} :
    Execution.Trace.run (chunkedByteStep {helper} tvWorldConfig code) {fuel}
      (dispatched env journal (surplus + {decoder.gas})) =
    Execution.Trace.run (chunkedByteStep {helper} tvWorldConfig code) {fuel - width}
      (decodedFrame env journal surplus) :=
  Execution.Trace.run_segment {width} {fuel} _ _ (symbolic_decoder {args})

theorem resume_abi_prefix {binders}
    (noValue : env.value = 0)
    (selected : MIR.TransferFromDispatch.selector env.input = {dispatch.stack[0]}) :
    runBytes {helper} 2048 tvWorldConfig
      ⟨dispatchFrame env (surplus + {decoder.gas} + {dispatch.gas}) 0 [],journal,[],[],[]⟩ =
    Execution.Trace.run (chunkedByteStep {helper} tvWorldConfig code) {fuel - width}
      (decodedFrame env journal surplus) := by
  rw [resume_dispatch env journal (surplus + {decoder.gas}) (by omega) small noValue selected]
  exact resume_decoder {args}

#print axioms decoded_mir_memory
#print axioms resume_abi_prefix
"""
    out += metered_prefix.abi_failure(
        binders,
        args,
        "dispatchFrame env 0 0 []",
        f"decoderFrame env 0 {decoder.pc} {words(decoder.stack)} {words(decoder.memory)}",
        len(dispatch.instructions) + width,
        dispatch.gas + decoder.gas,
    )
    if concrete:
        out += f"""
def decoderStart := decodedFrame env checkpoint (dispatchStart.frame.gas - {decoder.gas})
theorem decoder_concrete :
    runBytes {helper} 2048 tvWorldConfig initial =
      Execution.Trace.run (chunkedByteStep {helper} tvWorldConfig code) {fuel - width} decoderStart := by
  exact resume_abi_prefix env checkpoint (dispatchStart.frame.gas - {decoder.gas})
    (by decide +kernel) (by decide +kernel) (by decide +kernel) (by decide +kernel)
    (by intro i h; simp only [List.mem_cons,List.not_mem_nil,or_false] at h
        rcases h with {" | ".join("rfl" for _ in addresses)} <;> decide +kernel)
    (by decide +kernel) (by decide +kernel)
"""
    else:
        out += f"""
abbrev decoderStart := dispatchStart
theorem decoder_concrete :
    runBytes {helper} 2048 tvWorldConfig initial =
      Execution.Trace.run (chunkedByteStep {helper} tvWorldConfig code) {fuel} decoderStart :=
  dispatch_concrete
"""
    return out
