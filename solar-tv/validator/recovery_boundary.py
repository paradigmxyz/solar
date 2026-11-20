"""Bind the recovery gas and call proof to the final Morpho runtime."""

from dataclasses import dataclass

from . import entry_proof, native, world
from .callers import require
from .check import build_model

MODULE = "SolarTV.MIR.Transforms.StaticCallCut"
NAME = "MorphoRecoveryBoundary"


@dataclass
class Site:
    gas: int
    call: int


def find(decoded):
    points = list(decoded)
    sites = [
        Site(gas, call)
        for address, gas, call in zip(points, points[1:], points[2:], strict=False)
        if [decoded[pc] for pc in (address, gas, call)]
        == [("push1", 1), ("gas", None), ("staticcall", None)]
        and call == gas + 1
    ]
    require(len(sites) == 1, "expected one adjacent recovery GAS and STATICCALL")
    return sites[0]


def source(site, configuration, base):
    return f"""import EntryImage
import {MODULE}
{entry_proof.HEADER}
open SolarTV.EVM.MessageBoundary
open MIR.StaticCallCut (project priced)
{configuration}
def samplePC : Nat := {site.gas}
def recoveryPC : Nat := {site.call}

theorem executed (journal : Execution.Journal) (frame : EVM.State.Frame)
    (offset length : Word) (rest : List Word)
    (image : frame.code = code.bytes) (pc : frame.pc = samplePC)
    (stack : frame.stack = 1 :: offset :: length :: 0 :: 32 :: rest)
    (space : rest.length+6 ≤ 1024) :
    EVM.State.instruction tvWorldConfig journal frame =
      (if frame.gas < 2 then EVM.State.stepExhausted journal
        else .ok (.next journal (afterGas frame))) ∧
    EVM.State.instruction tvWorldConfig journal (afterGas frame) =
      staticAction tvWorldConfig journal (afterGas frame)
        ((BitVec.ofNat 256 (frame.gas-2)).toNat) 1 offset length 0 32 rest := by
  apply recovery tvWorldConfig journal frame {base} offset length rest stack
  · rw [image, pc, ← EVM.State.CodeChunks.get_eq]
    decide +kernel
  · rw [image, pc, ← EVM.State.CodeChunks.get_eq]
    decide +kernel
  · rfl
  · rfl
  · decide
  · exact space

theorem contextual (cfg : MIR.State.Config) (work : Nat)
    (mir : MIR.State.Frame) (frame : EVM.State.Frame)
    (rest : List Word) (address arg : MIR.State.Operand)
    (offset length : MIR.State.Scalar) (scratch input active : List Execution.Byte)
    (image : frame.code = code.bytes) (pc : frame.pc = samplePC)
    (stack : frame.stack = 1 :: offset.word :: length.word :: 0 :: 32 :: rest)
    (space : rest.length+6 ≤ 1024)
    (readAddress : MIR.State.read mir address = .ok 1)
    (readSlice : MIR.State.readValue mir arg = .ok (.aggregate [.scalar offset,.scalar length]))
    (available : cfg.available.contains 0xfa = true)
    (fork : cfg.fork = .cancun) (bound : cfg.externalConfig = some tvWorldConfig)
    (environment : frame.environment = cfg.environment)
    (paid : 2 ≤ frame.gas) (boundedGas : frame.gas-2 ≤ 30000000)
    (cleared : Execution.expand mir.memory 0 32 = .ok scratch)
    (memory : frame.memory = Execution.writeBytes scratch 0 (Execution.wordBytes 0))
    (inputOk : Execution.expand frame.memory offset.word.toNat length.word.toNat = .ok input)
    (outputOk : Execution.expand input 0 32 = .ok active) :
    let atCall := afterGas frame
    project {{mir with memory := frame.memory}} (do
      let action ← EVM.State.instruction tvWorldConfig mir.world atCall
      EVM.State.advance tvWorldConfig (EVM.State.run work tvWorldConfig) action) =
    MIR.LowerReturndata.invoke (priced cfg mir frame) work
      (MIR.State.messageArguments (priced cfg mir frame) mir
        (.address 0xfa false false true true true) [address,arg]) := by
  have action := (executed mir.world frame offset.word length.word rest image pc stack space).2
  have bounded : (BitVec.ofNat 256 (frame.gas-2)).toNat = (afterGas frame).gas := by
    simp [afterGas, Nat.mod_eq_of_lt (by omega : frame.gas-2 < 2^256)]
  dsimp only
  rw [action, bounded]
  exact MIR.StaticCallCut.recovery cfg tvWorldConfig work mir frame rest address arg offset length
    scratch input active readAddress readSlice available fork rfl bound environment (by decide)
    paid boundedGas cleared memory inputOk outputOk

#print axioms executed
#print axioms contextual
"""


def certify(directory, site, fixture, solar, executable, timeout):
    built = build_model(timeout, MODULE)
    entry_proof.success(built, MODULE)
    configuration = world.configuration(solar, "cancun", fixture, timeout, executable)
    base = next(
        row["forks"]["cancun"]["base_gas"]
        for row in native.catalog(executable, timeout)["evmir"]
        if row["opcode"] == 0xFA
    )
    checked = entry_proof.lean(
        directory, NAME, source(site, configuration, base), timeout
    )
    output = entry_proof.success(checked, NAME)
    entry_proof.check_axioms(output.splitlines(), 2)
    return {
        "proof": f"kernel/{NAME}.lean",
        "gas_pc": site.gas,
        "call_pc": site.call,
        "scope": "matched physical and MIR frames from emitted GAS through STATICCALL",
    }
