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
import SolarTV.Agreement.Encoder

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
-- The agreement theorems that the pass validation encoder's terms are the Lean semantics.
-- A bit-blasted proof also depends on its own axiom for bv_decide's natively evaluated
-- LRAT check.
/-- info: 'SolarTV.Agreement.step_sdiv' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.Agreement.step_sdiv

/-- info: 'SolarTV.Agreement.abstract_mulmod' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.Agreement.abstract_mulmod

/--
info: 'SolarTV.Agreement.word_signextend' depends on axioms: [propext,
 Classical.choice,
 Quot.sound,
 SolarTV.Agreement.word_signextend._native.bv_decide.ax_1_15]
-/
#guard_msgs in
#print axioms SolarTV.Agreement.word_signextend

/--
info: 'SolarTV.Agreement.mir_add_i160' depends on axioms: [propext,
 Classical.choice,
 Quot.sound,
 SolarTV.Agreement.mask_160._native.bv_decide.ax_1_5]
-/
#guard_msgs in
#print axioms SolarTV.Agreement.mir_add_i160
