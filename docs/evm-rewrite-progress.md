# EVM rewrite progress

The rewrite remains incomplete. Known raw-memory/spill corruption, one original
UI assertion and individual sealed gas/size debts still block final acceptance.
This page summarizes the local state at `c0782114` on 2026-09-08; the
[complete checkpoint archive](evm-rewrite-checkpoints.md) preserves every earlier
progress paragraph, baseline hash, rejected trial and measurement limitation.
The [handoff](evm-rewrite-plan.md) defines acceptance; [PR #1388][pr] tracks review.

## Accepted local state

Main `6059f0c0` was integrated by merge `53075d0e`. Commit-parent metadata
confirms that integration; the old `8d553ca1` status is historical.
Latest local milestones are literal caching `552c9d04`, its tests `432df730`,
its measurement record `72b6de3e`, and cold-call test migration `c0782114`.

The backend keeps MIR semantics, private stack scheduling, physical block IR and
primitive assembly separate. The Gas literal cache uses existing stack-capacity,
observer and complete-body cost guards; it does not change memory ownership.
[Stack scheduling research](evm-stack-scheduling-research.md) records the pinned
solx, Venom and Sonatina sources and the limits of their applicability.

## Latest measured output change

Literal caching versus frozen `0e61860e` shrinks eight of 5,032 matched UI objects:
Gas creation/runtime totals each fall 977 bytes, with no growth; Size is exact.
The entry-order witness saves 940 bytes per object and 15/33 execution gas,
but retains a 20-byte Gas debt against sealed. The narrow control saves seven
bytes and 14 gas, with concrete calls and bounded symbolic agreement.

The official full workflow retains 24 IDs, 175 ordered gas labels and 139
observations. Aave saves 12 gas on 19 labels and 24 on two; none increase.
All nine heavy projects retain 1,672 contracts and 3,344 objects: eight objects
shrink two bytes, none grow. The Size supplement is byte/gas exact.
Compiler-time geometric mean is +1.16%, RSS -0.61% against the prior checkpoint;
several sample ranges are disjoint. This is a measured compilation cost.
See the [literal-cache evidence][literal] for exact inputs, producers and samples.

## Verification and reviewed expectations

The literal-cache acceptance run has 11,714 UI passes, two original failures,
1,395 other passes and two skips. All 36 Foundry projects pass (772 compiler
and 765 solc tests). Clippy, formatting and typos pass at that checkpoint.
Focused calls, bounded solver results and metadata byte-neutrality are retained
in the evidence; timeouts and reused executions remain explicitly distinguished.

The later cold-call test commit closes one original assertion through reviewed
expectation migration. Both selectors, predicates, cold helper paths, arguments
and exact return/revert oracles remain checked; the old mandatory fallthrough
shape is replaced by measured shared-terminal behavior. None checks are unchanged.
Actual filtered UI passes all three revisions and 30 calls. Current Gas objects
are 194/177 bytes versus sealed 214/197; Size is 193/176 versus 214/197.
All 15 matched labels use less gas except nonpayable rejection, which is equal.
The [cold-call evidence][cold] retains failed directive preparations, the final
runner log and exact bytecode/oracle joins. This focused pass is not a new
full-workspace or remote-CI certification.

## Unaccepted trial and remaining work

Size tail grouping remains an uncommitted, unaccepted trial. Its draft2 UI screen
has 28 shrinking contract/mode entries, no growth, and 559 fewer creation and
runtime bytes each; Gas objects are exact. These are provisional corpus results,
not acceptance or closure of the remaining alias assertion. See [trial evidence][group].

`global_stack_calldata_alias.sol` is the one remaining original failing UI
assertion. Known raw-memory/spill alias corruption is unresolved. The accepted
heavy ledger still has 32,427,688 positive bytes of sealed size debt; this is a
sum of regressions, not net corpus growth. Compile-time regressions also remain.

Next, finish the current trial's individual gas/size and metadata gates, address
memory ownership and alias-codegen debt, and rerun final workspace/corpus checks.
Keep correctness and runtime gas ahead of size, then compiler time and memory.
No complete-functionality, all-CI-green or remote-push conclusion follows from
these local milestones. The [archive](evm-rewrite-checkpoints.md) retains the
sealed baseline identity and every prior evidence ledger without rewriting history.

[pr]: https://github.com/paradigmxyz/solar/pull/1388
[literal]: ../target/codegen-bench/evm-rewrite-candidate/literal-cache-workflow-20260908/
[cold]: ../target/codegen-bench/evm-rewrite-candidate/cold-expectation-migration-20260908/
[group]: ../target/codegen-bench/evm-rewrite-candidate/size-long-tail-group-workflow-20260908/
