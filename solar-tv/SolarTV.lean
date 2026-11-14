import SolarTV.MIR.Transforms.DCE
import SolarTV.MIR.Transforms.Casts
import SolarTV.MIR.Transforms.CheckElim
import SolarTV.MIR.Transforms.AbiValidation
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

/-- info: 'SolarTV.MIR.State.discharge_abi_obligation' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.State.discharge_abi_obligation

/-- info: 'SolarTV.MIR.State.unproved_abi_obligation' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.State.unproved_abi_obligation

/-- info: 'SolarTV.MIR.State.function_normalization_is_not_padding_evidence' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.State.function_normalization_is_not_padding_evidence

/-- info: 'SolarTV.MIR.State.shift_zero_iff_padding' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.State.shift_zero_iff_padding

/-- info: 'SolarTV.MIR.State.canonical_uint_shift' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.State.canonical_uint_shift

/-- info: 'SolarTV.MIR.State.canonical_uint_and' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.State.canonical_uint_and

/-- info: 'SolarTV.MIR.State.wordBits_widenBits' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.State.wordBits_widenBits

/-- info: 'SolarTV.MIR.State.wordBits_ite' does not depend on any axioms -/
#guard_msgs in
#print axioms SolarTV.MIR.State.wordBits_ite

/-- info: 'SolarTV.MIR.State.widenBits_ite' does not depend on any axioms -/
#guard_msgs in
#print axioms SolarTV.MIR.State.widenBits_ite
