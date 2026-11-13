import SolarTV.MIR.Program
import SolarTV.EVMIR.Program

/-! Composing the stages of the backend into one stage from lowered MIR to EVM IR.

A compilation is the lowered MIR module, the output of each MIR pass after it, the
scheduled EVM IR program and the output of each EVM IR pass. Each MIR pass and each EVM IR
pass is a stage of its layer's semantics, which `-Zvalidate-passes` and
`-Zvalidate-evm-ir-passes` check pass by pass, and scheduling is a stage between the two
layers, which `-Zvalidate-backend` checks. Chaining them gives a stage from the MIR the
pipeline starts with to the EVM IR it ends with (`mir_to_evmir`), or from certificates
(`mir_to_evmir_certified`). The MIR semantics is at the lowered phase, so `lower-abi` and
`lower-dispatch` are outside these stages. `AssemblyStage` is the stage from EVM IR to
bytecode, which solidity-lean's bytecode semantics states, and `mir_to_bytecode` composes
it. -/

namespace SolarTV

open Machine

noncomputable def mirSem (o : Oracle) : Sem := MIR.Lowered.moduleSem o

noncomputable def evmirSem (o : Oracle) : Sem := EVM.Program.sem o

/-- Each program is a stage of the one before it. -/
def Steps (S : Sem) (scope : Call → Prop) : List S.Prog → Prop
  | [] | [_] => True
  | p :: q :: rest => Stage S S p q scope ∧ Steps S scope (q :: rest)

/-- Each program is certified against the one before it. -/
def CertifiedSteps (L : Layer) (o : Oracle) (scope : Call → Prop) : List L.Prog → Prop
  | [] | [_] => True
  | p :: q :: rest => Certified L L o p q scope ∧ CertifiedSteps L o scope (q :: rest)

theorem Steps.stage {S : Sem} {scope : Call → Prop} :
    ∀ (p : S.Prog) (ps : List S.Prog), Steps S scope (p :: ps) →
      Stage S S p ((p :: ps).getLast (List.cons_ne_nil p ps)) scope
  | p, [], _ => Stage.refl S p scope
  | p, q :: rest, ⟨first, steps⟩ => by
    rw [List.getLast_cons_cons]
    exact first.trans (Steps.stage q rest steps)

theorem CertifiedSteps.certified {L : Layer} {o : Oracle} {scope : Call → Prop} :
    ∀ (p : L.Prog) (ps : List L.Prog), CertifiedSteps L o scope (p :: ps) →
      Certified L L o p ((p :: ps).getLast (List.cons_ne_nil p ps)) scope
  | p, [], _ => Certified.refl L o p scope
  | p, q :: rest, ⟨first, steps⟩ => by
    rw [List.getLast_cons_cons]
    exact first.trans (CertifiedSteps.certified q rest steps)

theorem CertifiedSteps.steps {L : Layer} {o : Oracle} {scope : Call → Prop} :
    ∀ (ps : List L.Prog), CertifiedSteps L o scope ps → Steps (L.sem o) scope ps
  | [], _ | [_], _ => trivial
  | _ :: q :: rest, ⟨first, steps⟩ =>
    ⟨stage_of_certified first, CertifiedSteps.steps (q :: rest) steps⟩

structure Compilation where
  mir : MIR.Lowered.Module
  mirPasses : List MIR.Lowered.Module
  scheduled : EVM.Program.Program
  evmirPasses : List EVM.Program.Program

/-- The MIR the backend schedules: the output of the last MIR pass. -/
def Compilation.scheduledFrom (p : Compilation) : MIR.Lowered.Module :=
  (p.mir :: p.mirPasses).getLast (List.cons_ne_nil _ _)

/-- The EVM IR the pipeline ends with: the output of the last EVM IR pass. -/
def Compilation.evmir (p : Compilation) : EVM.Program.Program :=
  (p.scheduled :: p.evmirPasses).getLast (List.cons_ne_nil _ _)

theorem mir_to_evmir (o : Oracle) (scope : Call → Prop) (p : Compilation)
    (mirPasses : Steps (mirSem o) scope (p.mir :: p.mirPasses))
    (scheduling : Stage (mirSem o) (evmirSem o) p.scheduledFrom p.scheduled scope)
    (evmirPasses : Steps (evmirSem o) scope (p.scheduled :: p.evmirPasses)) :
    Stage (mirSem o) (evmirSem o) p.mir p.evmir scope :=
  ((Steps.stage _ _ mirPasses).trans scheduling).trans (Steps.stage _ _ evmirPasses)

theorem mir_to_evmir_certified (o : Oracle) (scope : Call → Prop) (p : Compilation)
    (mirPasses : CertifiedSteps MIR.Lowered.moduleLayer o scope (p.mir :: p.mirPasses))
    (scheduling : Certified MIR.Lowered.moduleLayer EVM.Program.layer o p.scheduledFrom p.scheduled
      scope)
    (evmirPasses : CertifiedSteps EVM.Program.layer o scope (p.scheduled :: p.evmirPasses)) :
    Stage (mirSem o) (evmirSem o) p.mir p.evmir scope :=
  mir_to_evmir o scope p (CertifiedSteps.steps _ mirPasses) (stage_of_certified scheduling)
    (CertifiedSteps.steps _ evmirPasses)

/-- The stage from scheduled EVM IR to bytecode under a bytecode semantics, which this
library takes as a hypothesis. -/
def AssemblyStage (bytecode : Sem) (o : Oracle) (q : EVM.Program.Program)
    (code : bytecode.Prog) (scope : Call → Prop) : Prop :=
  Stage (evmirSem o) bytecode q code scope

theorem mir_to_bytecode (o : Oracle) (scope : Call → Prop) (p : Compilation)
    (bytecode : Sem) (code : bytecode.Prog)
    (mirPasses : CertifiedSteps MIR.Lowered.moduleLayer o scope (p.mir :: p.mirPasses))
    (scheduling : Certified MIR.Lowered.moduleLayer EVM.Program.layer o p.scheduledFrom p.scheduled
      scope)
    (evmirPasses : CertifiedSteps EVM.Program.layer o scope (p.scheduled :: p.evmirPasses))
    (assembly : AssemblyStage bytecode o p.evmir code scope) :
    Stage (mirSem o) bytecode p.mir code scope :=
  (mir_to_evmir_certified o scope p mirPasses scheduling evmirPasses).trans assembly

/-- One MIR pass, the backend and one EVM IR pass, each certified. -/
theorem pipeline {o : Oracle} {scope : Call → Prop} {f g : MIR.Lowered.Module}
    {q r : EVM.Program.Program}
    (mirPass : Certified MIR.Lowered.moduleLayer MIR.Lowered.moduleLayer o f g scope)
    (scheduling : Certified MIR.Lowered.moduleLayer EVM.Program.layer o g q scope)
    (evmirPass : Certified EVM.Program.layer EVM.Program.layer o q r scope) :
    Stage (mirSem o) (evmirSem o) f r scope :=
  stage_of_certified ((mirPass.trans scheduling).trans evmirPass)

end SolarTV
