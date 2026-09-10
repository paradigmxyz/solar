# Same-word reload across a literal

The accepted local candidate extends the existing late-DCE memory rewrites:

```text
push H; mstore; push K; push H; mload; binary
  -> dup1; push H; mstore; push K; swapped_binary
```

The original store and memory expansion remain. Only canonical literal operands,
equal full-word addresses and the existing reversible binary whitelist match.
The additional copy uses the existing absolute-entry or original-block-peak
proof against the 1,024-word stack limit. Both split boundaries and glue rules
apply. The ordinary rewrite helper merges bounded source origins and retains
only the original boundary events; metadata does not determine admission.
Scheduling queries and earlier cleanup disable the new form. No memory
observation is crossed, and no new pass, analysis, home policy or assembly
transform is introduced.

The change adds 31 physical backend Rust lines across two existing files.
The backend now has 17,661 physical lines in 48 Rust files, 16,977 fewer than
the 34,638-line deletion inventory. These counts include comments, blanks and
local tests; they are not strict production SLOC.

## Matched measurements

A fresh same-checkout debug baseline precedes production edits. The frozen
baseline is `solar-gas-debt-baseline`, SHA-256
`62a61b666d548112087b6a623ea039f5f30165aa9476636cf52ccd3643d019ae`;
the candidate is `solar-gas-debt-candidate1`, SHA-256
`0fb79d8fa66b54cfda85bd86af2466686f6e6704100e8487e74cb88e25976c3d`.
All 150 source keys match; only `local.rs` and `local/peephole.rs` differ.
The workflow uses pinned solc 0.8.36 and retains the exact inherited reference
rows without relabeling them as fresh solc executions. Native work stopped
around both timed Full/Size workflows.

| Corpus | Matched objects | Smaller | Larger | Creation + runtime delta |
| --- | ---: | ---: | ---: | ---: |
| UI Gas | 2,576 | 12 | 0 | -700 bytes |
| UI Size | 2,576 | 10 | 0 | -682 bytes |
| Heavy projects | 3,344 | 302 | 0 | -149,352 bytes |
| Runtime Gas | 30 | 2 | 0 | -132 bytes |
| Runtime Size | 30 | 2 | 0 | -142 bytes |

The corpora overlap and must not be summed. No equal-size object changes its
bytes. UI comparison retains 846 primary sources, 1,692 mode rows, the same
18 diagnostic-failure rows and 40 unresolved-link objects. Placeholder strings
retain their identities rather than being represented as linked executable
bytes. The heavy audit joins all nine project inputs and complete-output
fingerprints, 1,672 contracts and 3,344 objects. Seven project outputs are
freshly captured; two reuse exact complete fingerprints with their original
producers retained. Seaport accounts for 144,992 of the 149,352 saved bytes.

Full and Size retain all 24/15 test IDs, 175 identical gas labels and 139
identical execution observations in each mode. Nitro is the only shrinking
runtime case: Gas creation/runtime fall 20,410/20,168 to 20,344/20,102 bytes;
Size falls 25,732/25,490 to 25,661/25,419. Its Size warning changes only the
independently verified runtime length. One UI row reorders the same nine
complete warning blocks; the failed strict comparison and exact rendered-block
reconciliation remain archived, with no diagnostic fields removed.

Full compiler-median geometric mean changes -0.718%, and Size changes +1.741%;
RSS changes +0.493% and -0.203%. Both legs retain the same samples: 21 Full
cases have five samples and three have one under the existing ten-second
policy; all 15 Size cases have five. These observations establish neither a
causal speedup nor universal compiler-time improvement. The Size increase and
all per-case increases remain explicit. A quiet four-leg Size ABBA recheck
uses the same frozen compilers and in-repository wrapper without gas execution.
All 15 IDs and 75 compile samples per leg are retained, and every complete
input/output fingerprint matches the original corresponding producer. The
geometric mean of per-case medians across the ten samples per producer changes
+0.020%. This repeat does not reproduce the earlier +1.741% aggregate increase;
it does not establish a compiler speedup. Raw legs and per-case samples remain
in `candidate1/size-abba/`.

## Source activation and verification

The unchanged `ResidentArgDepth` fixture first differs at `late-dce` in both
modes. Every preceding captured pass body matches. The complete resulting
module differs by one `bb10` replacement with H=288, K=17 and ADD, retaining
the source store. Gas runtime shrinks 294 to 291 bytes; Size shrinks 289 to 286.
Eight native plain/staged captures match their complete frozen UI creation and
runtime objects.

Both `first(uint256)` comparisons through `fuzz/bin/solsymdiff` report bounded
agreement with pinned solc, using Osaka, viaIR and optimizer runs 200/1 for
Gas/Size. Bounds are 64 paths, 256 solver queries, depth 5,000, 68 calldata
bytes, 64 returndata bytes, 60 seconds per subprocess, 10 seconds per solver
query and 10,000,000 gas per call. The scalar input is symbolic; this is not
an unbounded equivalence proof. Four additional native staged compilations
consume the exact saved Standard JSON inputs and match the entire generated
symbolic runtimes. Standard JSON appends a 14-byte CBOR trailer absent from
CLI output; that difference is documented and preserved, not stripped away.
Both native stages show the same activation. All owned descendants are reaped.

Five new EVM IR fixtures pass eleven installed revisions, covering phase
admission, comparisons, zero/unaligned addresses, observation and store
refusals, metadata, glue and the known 1,022/1,023-word entry boundary. Focused
execution passes 48 calls across eight inputs, three configurations and two
frozen producers, checking the result, nonzero prefix, stored word and MSIZE.
Twenty paired paths save three gas and four zero-address Osaka paths save two;
the measured whole-call peak remains four words. Metadata/plain images match
in all paired captures. Unknown-entry provenance, symbolic literals, custom
stack effects and huge-address failure are not claimed as tested controls.

The first workspace run exposed two existing snapshot differences containing
five store/reload identities. Reviewed golden updates retain the existing
sources and runtime oracles. The final workspace run passes all 1,577 tests
with two existing skips, including the UI and Foundry suites. Nightly Clippy
passes. Initial fixture/adapter failures, corrected directive metadata and
separate installed-test producer receipts are retained. This is a local
checkpoint. Main `afdd3a1c` is merged in `642e6ad1`, changing only Windows
CLI UI-test normalization; the implementation and tests are in `287eea34`.
Remote CI success for the new commits is not yet claimed.

## Remaining acceptance work

Positive sealed heavy size debt falls from 30,280,048 to 30,130,696 bytes
across 1,007 objects and 524 contracts. This sums individual creation/runtime
increases, including embedded children, rather than net corpus growth or
intrinsic function-body debt. The 15 gas regressions against retained main
`25b0c078` remain unchanged: two OZ mint labels at +3, two Nitro prover labels
at +1, two Aave POOL labels at +6, two Flash manager labels at +10, and seven
Fractional getter labels at +1. General source-memory ownership and complete
rewrite acceptance remain open. Rejected carry/padding and ownership trials
are not included in this change.

Evidence is retained under
`target/codegen-bench/evm-rewrite-candidate/gas-debt-20260910/`:
`candidate1/audit/` holds the independent corpus joins,
`store-reload/source-proof/` holds exact native/symbolic bridges,
`store-reload/tests-draft/` holds focused execution and installation receipts,
and `candidate1/workspace-final.log` records the final local test run.
