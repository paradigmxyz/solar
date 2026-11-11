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

import SolarTV.State.StorageSafety
import SolarTV.State.Calls
import SolarTV.KeccakSafety
import SolarTV.WordSafety

import SolarTV.EVMIR.State.Observe
import SolarTV.EVMIR.State.Safety
import SolarTV.EVMIR.Precompiles.Observe
import SolarTV.EVMIR.Precompiles.Safety

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
