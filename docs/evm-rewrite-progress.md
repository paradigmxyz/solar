# EVM rewrite progress

The rewrite remains incomplete. One original UI assertion, incomplete general
memory-ownership evidence and individual sealed gas/size debts still block final
acceptance.
This page summarizes local state after `ce593973` on 2026-09-08. The
[handoff](evm-rewrite-plan.md) defines acceptance; [PR #1388][pr] tracks review.
The [checkpoint archive](evm-rewrite-checkpoints.md) preserves the complete
history, baseline hashes, rejected trials and measurement limitations.

## Accepted local state

Main `6059f0c0` was integrated by merge `53075d0e`. Recent commits implement
literal caching (`552c9d04`, tests `432df730`), review the cold-call expectations
(`c0782114`), group profitable Size tails (`96c07690`, tests `d9fa3bd8`), and
contract live SSA values before writes (`5f70b2f2`, tests `5b06f7ab`).

MIR semantics, private stack scheduling, physical block IR and primitive assembly
remain separate. Literal caching uses existing capacity and observer facts.
Tail grouping stays in physical block IR and independently checks each member's
transfer capacity, split boundary and destinations. Neither changes memory homes.
[Prior-art research](evm-stack-scheduling-research.md) records pinned solx, Venom
and Sonatina sources and the limits of their applicability.

## Measured changes

Literal caching versus frozen `0e61860e` shrinks eight matched UI objects:
Gas creation/runtime totals each fall 977 bytes, with no growth; Size is exact.
Aave saves 12 gas on 19 hot labels and 24 on two. The entry-order witness saves
940 bytes per object and 15/33 gas, but retains 20 bytes of Gas debt versus sealed.
Its quiet compiler-time comparison is +1.16%, RSS -0.61%; that cost is retained.
See [literal-cache evidence][literal] for exact inputs, producers and samples.

Size tail grouping versus frozen `7d598404` shrinks 56 of 5,036 matched UI objects:
creation/runtime totals each fall 559 bytes, with no growth. All 1,648 IDs and
825 source hashes match. The initial trial's seven growing objects are preserved
as rejected evidence; the final reserve restores each to baseline bytes.
ERC20Mock saves 11 bytes in both objects with unchanged hot call gas.
Alias Size runtime falls 179→166 bytes; three labels cost 11 more gas than the
prior candidate while remaining below sealed. Its Size byte debt remains 13.

The final full workflow preserves all 24 IDs, 175 ordered gas labels, 139
observations and complete Gas outputs, including all nine heavy projects.
The Size supplement retains 15 IDs/175 labels/139 observations. Quiet full-run
compiler-time geometric mean is -1.06%, RSS +0.30%; this is not a causal speedup
claim. Concurrent Size timing is not used. [Tail-group evidence][group] retains
all commands, individual deltas, exact output joins and source/binary pins.

## Verification and reviewed expectations

Final workspace results are 11,734 UI passes, one original alias assertion
failure, 1,395 other passes and two skips. All 36 Foundry projects pass
(772 compiler tests, 765 solc tests), with reported gas and sizes unchanged.
Clippy, formatting and typos pass. The larger tail fixture passes 24 fresh calls
and saves 16 Size bytes with unchanged gas; the alias has bounded solsymdiff
agreement against pinned solc under the recorded Size input/world limits.

Metadata/debug requests preserve measured bytecode across 92 paired captures.
All 75,054 source-map projections match emitted origins. Shared tails can merge
or lose ambiguous checkpoints; optimized-away forwarding entries can lose events.
Those existing policies and changed immutable/reference locations are explicit
in the independent review, not described as complete event preservation.

The cold-call test retains the original Solidity and None checks. Reviewed
optimized checks bind both selectors, predicates, cold helper paths, arguments
and shared returns instead of requiring the former physical fallthrough shape.
All three revisions and 30 exact call/revert checks pass. Current Gas objects
are 194/177 bytes versus sealed 214/197; Size is 193/176 versus 214/197.
All 15 matched labels use less gas except nonpayable rejection, which is equal.
[Cold-call evidence][cold] preserves failed directive setup and normalization
attempts. No original test was removed to close that assertion.

## Remaining acceptance work

`global_stack_calldata_alias.sol` remains the original failing assertion.
The original readback failures now pass in every mode. General source-memory
ownership remains an open contract; bounded shared-input sweeps pass. The accepted heavy ledger
still has 32,427,688 positive bytes of sealed size debt across 1,050 objects;
this is a sum of regressions, not net corpus growth. Compiler-time debts remain.
The backend has 16,931 physical lines in 46 files, 17,707 fewer than the deletion
inventory; this includes embedded tests and is not strict production SLOC.

Eager contraction removes avoidable spills and saves bytecode without corpus
growth; full Gas/Size outputs and Foundry gas remain exact. It adds 188 physical
MIR Rust lines. Quiet repeats retain +1.64% OpenZeppelin and +3.11% Morpho compile
costs. The consumer-tracking simplification (`ce593973`) removes five Rust
lines, halves its consumer entry to four bytes, and preserves all measured
outputs and gas. Its full timing (+4.22%, RSS +0.10%) overlaps another task
on the host and has unequal baseline sample counts; it establishes no speed
claim. Next, restore alias-codegen size and complete the remaining runtime/size
gates. Keep correctness and runtime
gas ahead of size, then compiler time and memory. These local milestones do not
establish complete functionality, green remote CI or a successful remote push.

[pr]: https://github.com/paradigmxyz/solar/pull/1388
[literal]: ../target/codegen-bench/evm-rewrite-candidate/literal-cache-workflow-20260908/
[cold]: ../target/codegen-bench/evm-rewrite-candidate/cold-expectation-migration-20260908/
[group]: ../target/codegen-bench/evm-rewrite-candidate/size-long-tail-group-workflow-20260908/
