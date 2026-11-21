"""Bind Cancun message debits to the calls reached before Morpho's helper."""

from . import entry_proof
from .callers import require
from .check import build_model

MODULE = "SolarTV.MIR.Transforms.PreHelperCall"
NAME = "MorphoPreHelperCalls"


def source(sites, config, catalog):
    calls = sorted({pc for rows in sites.values() for pc, op in rows if op == "call"})
    static = sorted(
        {pc for rows in sites.values() for pc, op in rows if op == "staticcall"}
    )
    require(calls and static, "missing pre-helper messages")
    base = catalog[0xF1]["forks"]["cancun"]["base_gas"]
    static_base = catalog[0xFA]["forks"]["cancun"]["base_gas"]
    return f"""
import EntryImage
import {MODULE}
{entry_proof.HEADER}
open SolarTV.Execution MIR.State MIR.PreHelperCall MIR.StaticCallCut
open MIR.FinalCallBudget (withBudget)
{config}
def callSites : List Nat := {calls}
def oracleSites : List Nat := {static}

theorem callOpcode (pc : Nat) (site : pc ∈ callSites) : code.get? pc = some 0xf1 := by
  simp only [callSites,List.mem_cons,List.not_mem_nil,or_false] at site
  rcases site with {" | ".join("rfl" for _ in calls)} <;> decide +kernel

theorem oracleOpcode (pc : Nat) (site : pc ∈ oracleSites) : code.get? pc = some 0xfa := by
  simp only [oracleSites,List.mem_cons,List.not_mem_nil,or_false] at site
  rcases site with {" | ".join("rfl" for _ in static)} <;> decide +kernel

theorem pricedCall (cfg : Config) (work : Nat)
    (mir : Frame) (physical : EVM.State.Frame) (rest : List Word)
    (request target offset length out outSize : Word) (input active : List Byte)
    (image : physical.code = code.bytes) (site : physical.pc ∈ callSites)
    (stack : physical.stack = request :: target :: 0 :: offset :: length :: out :: outSize :: rest)
    (space : rest.length+7 ≤ 1024) (fork : cfg.fork = .cancun)
    (bound : cfg.externalConfig = some tvWorldConfig)
    (environment : physical.environment = cfg.environment)
    (memory : physical.memory = mir.memory)
    (requestFits : request.toNat ≤ 30000000)
    (inputOk : expand mir.memory offset.toNat length.toNat = .ok input)
    (outputOk : expand input out.toNat outSize.toNat = .ok active) :
    project mir (do
      let action ← EVM.State.instruction tvWorldConfig mir.world physical
      EVM.State.advance tvWorldConfig (EVM.State.run work tvWorldConfig) action) =
    external work (withBudget cfg mir physical.gas) mir 0xf1
      [target,0,offset,length,out,outSize] (some request) := by
  have opcode : physical.code[physical.pc]? = some 0xf1 := by
    rw [image, ← EVM.State.CodeChunks.get_eq]
    exact callOpcode _ site
  rw [decodeCall tvWorldConfig mir.world physical {base} request target offset length out outSize rest
    stack opcode rfl (by decide) space]
  exact preparedCall cfg tvWorldConfig work mir physical rest request target offset length out outSize
    input active fork rfl bound environment memory (by intro conflict; cases conflict) requestFits inputOk outputOk

theorem pricedOracle (cfg : Config) (work : Nat)
    (mir : Frame) (physical : EVM.State.Frame) (rest : List Word)
    (target offset length : Word) (input active : List Byte)
    (image : physical.code = code.bytes) (site : physical.pc ∈ oracleSites)
    (stack : physical.stack = BitVec.ofNat 256 physical.gas :: target :: offset :: length :: 0 :: 32 :: rest)
    (space : rest.length+6 ≤ 1024) (fork : cfg.fork = .cancun)
    (bound : cfg.externalConfig = some tvWorldConfig)
    (environment : physical.environment = cfg.environment)
    (memory : physical.memory = mir.memory)
    (requestFits : physical.gas ≤ 30000000)
    (inputOk : expand mir.memory offset.toNat length.toNat = .ok input)
    (outputOk : expand input 0 32 = .ok active) :
    project mir (do
      let action ← EVM.State.instruction tvWorldConfig mir.world physical
      EVM.State.advance tvWorldConfig (EVM.State.run work tvWorldConfig) action) =
    external work (withBudget cfg mir physical.gas) mir 0xfa [target,offset,length,0,32]
      (some (BitVec.ofNat 256 physical.gas)) := by
  have opcode : physical.code[physical.pc]? = some 0xfa := by
    rw [image, ← EVM.State.CodeChunks.get_eq]
    exact oracleOpcode _ site
  have request : (BitVec.ofNat 256 physical.gas).toNat = physical.gas :=
    Nat.mod_eq_of_lt (by omega)
  rw [EVM.MessageBoundary.staticCall tvWorldConfig mir.world physical {static_base} _
    target offset length 0 32 rest stack opcode rfl (by decide) space, request]
  exact prepared cfg tvWorldConfig work mir physical rest target offset length input active
    fork rfl bound environment memory (by intro conflict; cases conflict) requestFits inputOk outputOk

#print axioms pricedCall
#print axioms pricedOracle
"""


def certify(directory, sites, config, catalog, timeout):
    entry_proof.success(build_model(timeout, MODULE), MODULE)
    output = entry_proof.success(
        entry_proof.lean(
            directory, NAME, source(sites, config, catalog), timeout, module=True
        ),
        NAME,
    )
    entry_proof.check_axioms(output.splitlines(), 2)
    return {
        "proof": f"kernel/{NAME}.lean",
        "sites": sites,
        "scope": "exact debits and child forwarding at reached, prepared Morpho CALL sites",
    }
