import SolarTV.MIR.Transforms.DCE
import SolarTV.MIR.Transforms.Casts
import SolarTV.MIR.Transforms.CheckElim
import SolarTV.MIR.Transforms.TrimSwitchDefaults
import SolarTV.MIR.Transforms.RenamePhiPredecessors
import SolarTV.EVMIR.Transforms.EqIszeroJumpi
import SolarTV.EVMIR.Transforms.StackPeephole
import SolarTV.EVMIR.StackSafety
import SolarTV.EVMIR.Memory
import SolarTV.MIR.Transforms.DeadStore
import SolarTV.MemorySafety
import SolarTV.EVMIR.GasSafety
import SolarTV.EVMIR.Transforms.GasSwapAdd
import SolarTV.Stages.Assembly
import SolarTV.Stages.Link
import SolarTV.Stages.Entries

import SolarTV.Agreement.Encoder
import SolarTV.Composition
import SolarTV.MIR.Validation.AddZero

import SolarTV.State.StorageSafety
import SolarTV.State.Calls
import SolarTV.KeccakSafety
import SolarTV.WordSafety

import SolarTV.EVMIR.State.Normalize
import SolarTV.EVMIR.State.Observe
import SolarTV.EVMIR.State.Safety
import SolarTV.EVMIR.Precompiles.Observe
import SolarTV.EVMIR.Precompiles.Safety
import SolarTV.MIR.State.Observe
import SolarTV.MIR.State.Safety
import SolarTV.MIR.State.Normalize

-- Keep the transitive axiom sets visible on the pinned toolchain. An admitted
-- proof introduces sorryAx and fails the build even if it lives in an import.
/-- info: 'SolarTV.MIR.simplify_switch' depends on axioms: [propext] -/
#guard_msgs in
#print axioms SolarTV.MIR.simplify_switch

/-- info: 'SolarTV.EVM.eq_iszero_jumpi' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.EVM.eq_iszero_jumpi

/-- info: 'SolarTV.MIR.erase_unused' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.erase_unused

/-- info: 'SolarTV.MIR.rename_phi_predecessors' depends on axioms: [propext] -/
#guard_msgs in
#print axioms SolarTV.MIR.rename_phi_predecessors

/-- info: 'SolarTV.MIR.truncate_extended_address' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.truncate_extended_address

/-- info: 'SolarTV.MIR.increment_below_bound' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.increment_below_bound

/-- info: 'SolarTV.MIR.cannot_drop_overflow' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.cannot_drop_overflow

/-- info: 'SolarTV.EVM.swap_add' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.EVM.swap_add

/-- info: 'SolarTV.EVM.dropping_full_push_pop_changes_outcome' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.EVM.dropping_full_push_pop_changes_outcome

/-- info: 'SolarTV.MIR.dead_store' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.dead_store

/-- info: 'SolarTV.EVM.Gas.swap_add_refines' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.EVM.Gas.swap_add_refines

/-- info: 'SolarTV.Assembly.stage' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.Assembly.stage

/-- info: 'SolarTV.Assembly.example_fault' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.Assembly.example_fault

/-- info: 'SolarTV.Entries.to_bytecode' does not depend on any axioms -/
#guard_msgs in
#print axioms SolarTV.Entries.to_bytecode

-- The agreement theorems that the pass validation encoder's terms are the Lean semantics.
-- They are kernel proofs: none bit-blasts, so none depends on a natively evaluated check.
/-- info: 'SolarTV.Agreement.step_sdiv' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.Agreement.step_sdiv

/-- info: 'SolarTV.Agreement.abstract_mulmod' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.Agreement.abstract_mulmod

/--
info: 'SolarTV.Agreement.word_signextend' depends on axioms: [propext, Classical.choice, Quot.sound]
-/
#guard_msgs in
#print axioms SolarTV.Agreement.word_signextend

/--
info: 'SolarTV.Agreement.mir_add_i160' depends on axioms: [propext, Classical.choice, Quot.sound]
-/
#guard_msgs in
#print axioms SolarTV.Agreement.mir_add_i160
/--
info: 'SolarTV.Agreement.word_byte' depends on axioms: [propext, Quot.sound]
-/
#guard_msgs in
#print axioms SolarTV.Agreement.word_byte

/--
info: 'SolarTV.Agreement.mir_shl_i160' depends on axioms: [propext, Classical.choice, Quot.sound]
-/
#guard_msgs in
#print axioms SolarTV.Agreement.mir_shl_i160

/--
info: 'SolarTV.Agreement.mir_lt_i1' depends on axioms: [propext, Classical.choice, Quot.sound]
-/
#guard_msgs in
#print axioms SolarTV.Agreement.mir_lt_i1

/--
info: 'SolarTV.Agreement.mir_sext_i160_i256' depends on axioms: [propext, Classical.choice, Quot.sound]
-/
#guard_msgs in
#print axioms SolarTV.Agreement.mir_sext_i160_i256
-- Contract semantics and the composition of stages.
/-- info: 'SolarTV.mir_to_evmir_certified' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.mir_to_evmir_certified

/-- info: 'SolarTV.mir_to_bytecode' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.mir_to_bytecode

/--
info: 'SolarTV.MIR.Lowered.failureConvention' depends on axioms: [propext, Classical.choice, Quot.sound]
-/
#guard_msgs in
#print axioms SolarTV.MIR.Lowered.failureConvention

/--
info: 'SolarTV.MIR.Lowered.moduleFailureConvention' depends on axioms: [propext, Classical.choice, Quot.sound]
-/
#guard_msgs in
#print axioms SolarTV.MIR.Lowered.moduleFailureConvention

/--
info: 'SolarTV.EVM.Program.failureConvention' depends on axioms: [propext, Classical.choice, Quot.sound]
-/
#guard_msgs in
#print axioms SolarTV.EVM.Program.failureConvention

/--
info: 'SolarTV.EVM.Program.stepOp_project' depends on axioms: [propext, Classical.choice, Quot.sound]
-/
#guard_msgs in
#print axioms SolarTV.EVM.Program.stepOp_project
-- One pass validation query lifted to a stage.
/--
info: 'SolarTV.MIR.Validation.AddZero.stage' depends on axioms: [propext, Classical.choice, Quot.sound]
-/
#guard_msgs in
#print axioms SolarTV.MIR.Validation.AddZero.stage

/-- info: 'SolarTV.Word.power_eq_pow' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.Word.power_eq_pow

/-- info: 'SolarTV.Keccak.empty' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.Keccak.empty

/-- info: 'SolarTV.Storage.overwrite' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.Storage.overwrite

/-- info: 'SolarTV.Storage.sentry_rejects' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.Storage.sentry_rejects

/-- info: 'SolarTV.Calls.reverted_restores_journal' does not depend on any axioms -/
#guard_msgs in
#print axioms SolarTV.Calls.reverted_restores_journal

/-- info: 'SolarTV.MIR.State.simultaneous_phis' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.State.simultaneous_phis

/-- info: 'SolarTV.MIR.State.work_limit_is_not_equivalence' depends on axioms: [propext] -/
#guard_msgs in
#print axioms SolarTV.MIR.State.work_limit_is_not_equivalence

/-- info: 'SolarTV.Execution.slotsWrite_overwrite' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.Execution.slotsWrite_overwrite

/-- info: 'SolarTV.Execution.flatten_wordBytes' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.Execution.flatten_wordBytes

/-- info: 'SolarTV.Execution.readWord_writeWord' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.Execution.readWord_writeWord

/-- info: 'SolarTV.MIR.State.run_next' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.State.run_next

/-- info: 'SolarTV.MIR.State.live_return_data_is_observable' depends on axioms: [propext] -/
#guard_msgs in
#print axioms SolarTV.MIR.State.live_return_data_is_observable

/-- info: 'SolarTV.MIR.State.project_inserted_field' depends on axioms: [propext] -/
#guard_msgs in
#print axioms SolarTV.MIR.State.project_inserted_field

/-- info: 'SolarTV.MIR.State.uninitialized_neighbor' depends on axioms: [propext] -/
#guard_msgs in
#print axioms SolarTV.MIR.State.uninitialized_neighbor
