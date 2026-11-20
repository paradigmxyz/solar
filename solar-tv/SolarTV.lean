import SolarTV.MIR.Transforms.DCE
import SolarTV.MIR.Transforms.DeadSSA
import SolarTV.MIR.Transforms.DeadSlice
import SolarTV.MIR.Transforms.FunctionDCE
import SolarTV.MIR.Transforms.CoalesceAlloc
import SolarTV.Correspondence
import SolarTV.MIR.Import
import SolarTV.MIR.Universal
import SolarTV.MIR.Transforms.Casts
import SolarTV.MIR.Transforms.CheckElim
import SolarTV.MIR.Transforms.AbiValidation
import SolarTV.MIR.Transforms.TrimSwitchDefaults
import SolarTV.MIR.Transforms.RenamePhiPredecessors
import SolarTV.MIR.Transforms.LowerAlloc
import SolarTV.MIR.Transforms.LowerAllocPrefix
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
import SolarTV.EVMIR.Region
import SolarTV.EVMIR.Prefix
import SolarTV.EVMIR.MemoryPrefix
import SolarTV.EVMIR.Suffix
import SolarTV.EVMIR.Transforms.TailMerge
import SolarTV.EVMIR.ContextSafety
import SolarTV.EVMIR.State.Observe
import SolarTV.EVMIR.State.Safety
import SolarTV.EVMIR.Precompiles.Observe
import SolarTV.EVMIR.Precompiles.Safety
import SolarTV.MIR.State.Observe
import SolarTV.MIR.State.Safety
import SolarTV.MIR.State.Normalize
import SolarTV.MIR.Effects
import SolarTV.EVMIR.Effects
import SolarTV.EVMIR.SourceGraph
import SolarTV.EVMIR.StackGraph
import SolarTV.MIR.Transforms.FreshHeap
import SolarTV.MIR.Transforms.TypeDCE
import SolarTV.MIR.Transforms.LowerAddressCall
import SolarTV.MIR.Transforms.LowerReturndata
import SolarTV.MIR.Transforms.LowerStorageHeader
import SolarTV.MIR.Transforms.LowerStorageBytes
import SolarTV.MIR.Transforms.StorageWordLoop
import SolarTV.MIR.Transforms.LowerPacked
import SolarTV.MIR.Transforms.LowerStaticWord

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

/-- info: 'SolarTV.EVM.Region.swap_across_jump' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.EVM.Region.swap_across_jump

/-- info: 'SolarTV.Correspondence.expansion_correct' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.Correspondence.expansion_correct

/-- info: 'SolarTV.EVM.Region.toTree_correct' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.EVM.Region.toTree_correct

/-- info: 'SolarTV.EVM.Prefix.contextual' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.EVM.Prefix.contextual

/-- info: 'SolarTV.EVM.Prefix.sufficient_contextual' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.EVM.Prefix.sufficient_contextual

/-- info: 'SolarTV.EVM.Prefix.resource' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.EVM.Prefix.resource

/-- info: 'SolarTV.MIR.Universal.executions_defined' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.Universal.executions_defined

/-- info: 'SolarTV.MIR.Universal.unsupported_is_not_equivalent' depends on axioms: [propext] -/
#guard_msgs in
#print axioms SolarTV.MIR.Universal.unsupported_is_not_equivalent

/-- info: 'SolarTV.MIR.Universal.returned_words_live' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.Universal.returned_words_live

/-- info: 'SolarTV.MIR.Universal.expand_defined' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.Universal.expand_defined

/-- info: 'SolarTV.EVM.Transforms.TailMerge.panic_tail' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.EVM.Transforms.TailMerge.panic_tail

/-- info: 'SolarTV.MIR.State.bit_or_one' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.State.bit_or_one

/-- info: 'SolarTV.MIR.State.nat_or_bits' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.State.nat_or_bits

/-- info: 'SolarTV.MIR.State.ofNat_toNat_shift_right' depends on axioms: [propext] -/
#guard_msgs in
#print axioms SolarTV.MIR.State.ofNat_toNat_shift_right

/-- info: 'SolarTV.MIR.State.shift_zero' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.State.shift_zero

/-- info: 'SolarTV.EVM.ContextSafety.caller_frame' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.EVM.ContextSafety.caller_frame

/-- info: 'SolarTV.EVM.ContextSafety.calldata_frame' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.EVM.ContextSafety.calldata_frame

/-- info: 'SolarTV.EVM.ContextSafety.calldata_memory' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.EVM.ContextSafety.calldata_memory

/-- info: 'SolarTV.EVM.MemoryPrefix.charged_append' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.EVM.MemoryPrefix.charged_append

/-- info: 'SolarTV.EVM.MemoryPrefix.charged_scalar' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.EVM.MemoryPrefix.charged_scalar

/-- info: 'SolarTV.EVM.MemoryPrefix.shared_exact' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.EVM.MemoryPrefix.shared_exact

/-- info: 'SolarTV.EVM.MemoryPrefix.shared_sufficient' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.EVM.MemoryPrefix.shared_sufficient

/-- info: 'SolarTV.MIR.Effects.contextual' does not depend on any axioms -/
#guard_msgs in
#print axioms SolarTV.MIR.Effects.contextual

/-- info: 'SolarTV.EVM.Effects.resource' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.EVM.Effects.resource

/-- info: 'SolarTV.EVM.Effects.expansion_correct' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.EVM.Effects.expansion_correct

/-- info: 'SolarTV.MIR.LowerAlloc.free_pointer_request' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.LowerAlloc.free_pointer_request

/-- info: 'SolarTV.MIR.LowerAlloc.constructor_request' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.LowerAlloc.constructor_request

/-- info: 'SolarTV.MIR.LowerAllocPrefix.contextual' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.LowerAllocPrefix.contextual

/-- info: 'SolarTV.EVM.Suffix.scalar_contextual' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.EVM.Suffix.scalar_contextual

/-- info: 'SolarTV.EVM.SourceGraph.identity' does not depend on any axioms -/
#guard_msgs in
#print axioms SolarTV.EVM.SourceGraph.identity

/-- info: 'SolarTV.EVM.StackGraph.contextual' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.EVM.StackGraph.contextual

/-- info: 'SolarTV.MIR.FreshHeap.reservation' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.FreshHeap.reservation

/-- info: 'SolarTV.MIR.FreshHeap.query' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.FreshHeap.query

/-- info: 'SolarTV.MIR.FreshHeap.initialized' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.FreshHeap.initialized

/-- info: 'SolarTV.MIR.FreshHeap.inhabited' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.FreshHeap.inhabited

/-- info: 'SolarTV.MIR.TypeDCE.certificate' does not depend on any axioms -/
#guard_msgs in
#print axioms SolarTV.MIR.TypeDCE.certificate

/-- info: 'SolarTV.MIR.DeadSSA.simulation' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.DeadSSA.simulation

/-- info: 'SolarTV.MIR.DeadSlice.make' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.DeadSlice.make

/-- info: 'SolarTV.MIR.DeadSlice.extract' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.DeadSlice.extract

/-- info: 'SolarTV.MIR.LowerAddressCall.memoryObjectArguments' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.LowerAddressCall.memoryObjectArguments

/-- info: 'SolarTV.MIR.LowerAddressCall.forwardSample' depends on axioms: [propext] -/
#guard_msgs in
#print axioms SolarTV.MIR.LowerAddressCall.forwardSample

/-- info: 'SolarTV.MIR.LowerReturndata.materialize' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.LowerReturndata.materialize

/-- info: 'SolarTV.MIR.LowerStorageHeader.decode' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.LowerStorageHeader.decode

/-- info: 'SolarTV.MIR.StorageWordLoop.clear_corresponds' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.StorageWordLoop.clear_corresponds

/-- info: 'SolarTV.MIR.LowerReturndata.memoryObjectCall' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.LowerReturndata.memoryObjectCall

/-- info: 'SolarTV.MIR.StorageWordLoop.fullWordsThenPartial' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms SolarTV.MIR.StorageWordLoop.fullWordsThenPartial
