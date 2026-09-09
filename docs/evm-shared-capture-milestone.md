# Shared mutable capture contraction

Implementation: `90579752`; MIR coverage: `5a2cb64f`; nonzero runtime coverage:
`c52f4718`. The baseline and final frozen compiler pins are `96d3602c` and
`04d5d37a`, with complete hashes in the retained manifests.

Eighteen storage captures used by two reductions previously remained in spill
homes across source writes. A restore overwrote the source value at address
576 before its readback. The sealed executable preserved that address; the
original source and nonzero-storage variants retain their exact return oracles.

The final MIR scheduler now attempts a bounded order for pure descendants of
mutable reads. It preserves all read/write anchors, operands and value IDs.
More than the target's reachable stack depth in shared matching captures must
remain live after one writer before a trial is considered. Acceptance requires
fewer captured values and fewer total live values at that same writer, without
increasing any write's pressure, the live peak, or the temporary-result bound.
CFG live-out includes successor Phi uses; reversible touched-value state avoids
cloning function-sized sets for each region. Regions above 128 instructions
are excluded. Physical stack layout remains private to backend scheduling.

Unrelated address calculations stay fixed. This matters without optimization:
hoisting the return-buffer address chain made an earlier trial grow pressure
at later writes and reject the fix. A broader trial also grew two existing
contracts. A second trial passed the entire existing corpus but grew a reduced
six-capture mixed case by 37 Gas runtime bytes. Both trials are rejected and
retained under the evidence directory. The final rule leaves that reduced
case's complete MIR and runtime bytecode exactly equal to its baseline.

Focused native checks pass all 45 calls across None, Gas and Size; the standard
UI matrix passes all 60 runtime oracles. Twenty-four MIR cases cover repeated
operands, three reductions, live-out/Phi uses, observation/effect boundaries,
the 128-instruction limit, the unoptimized address chain, and mixed-bank refusal.
Existing test bodies, checks and goldens remain unchanged. The new pure-helper
implementation adds 336 physical Rust lines and its parent adds a net 16, for
352 additional MIR lines. Backend physical LOC remains 17,612 in 48 files,
17,026 below the deletion inventory; this is not strict production SLOC.

The final UI screen preserves all 1,686 existing rows and 5,136 objects exactly,
including the 18 existing diagnostic-failure rows. Two new Solidity sources
add four successful rows and 12 objects. Full/Size workflows retain 24/15 test IDs, all 175 gas labels and 139 runtime
observations per mode, and 30 bytecode objects per mode exactly. All nine heavy
project fingerprints also match; all 3,344 selected heavy objects are byte-identical. Solc reference rows are inherited from the
pinned unchanged baseline inputs; this is not a fresh solc run. The final
five-repeat workflow (three large Full cases retain one sample) measures
compiler-median geomean ratios of 0.983656 Full and 0.995002 Size; RSS ratios
are 0.999743 and 1.003032. These small changes do not establish a speedup. All 1,577 workspace tests
pass (two skips), as do nightly formatting/Clippy and the Foundry suite.

The original 18-capture source is rejected by solc with stack-too-deep. A
smaller solc-compatible case genuinely activated the rejected second trial,
but both bounded symbolic runs timed out after 45 seconds. They establish no
agreement. The final rule refuses that case; exact harness-input replay
restores baseline bytecode in both modes. The retained sealed-executable result at address 576 and exact mathematical
return oracles support this bounded
fix, not arbitrary source-memory ownership or complete rewrite acceptance.

All producer hashes, original failed assertions, raw inputs/outputs, traces,
rejected source versions and independent receipts are retained under
`target/codegen-bench/evm-rewrite-candidate/shared-dag-20260909/`.
