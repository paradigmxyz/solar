import SolarTV.MIR.Transforms.DCE
import SolarTV.MIR.Transforms.TrimSwitchDefaults
import SolarTV.MIR.Transforms.RenamePhiPredecessors
import SolarTV.EVMIR.Transforms.EqIszeroJumpi

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
