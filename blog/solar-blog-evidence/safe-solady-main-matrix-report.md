# Safe Solady comparison

**The full API compatibility and equal-or-better gas target is not met.**

EVM: cancun; optimizer runs: 200; solc legacy and via-IR are both included.

362 function signatures implemented in 10 libraries; the pinned archive has 90 source files. This is a partial port.

28413 deterministic differential cases; 177 mismatching executions. 59 cases are excluded from performance comparisons because at least one implementation disagrees with the oracle.

Gas is the sum of executed opcode gas costs from debug_traceCall, excluding transaction intrinsic gas. Wrappers, inputs, optimizer runs and fork are identical. Calls do not share execution state. These library harness costs include ABI decoding, dispatch and encoding, and are not isolated instruction costs.

The reference is the cheaper upstream solc result for each call (legacy or via-IR). This envelope is stricter than choosing one compiler pipeline for a whole deployment. All six raw legs, revert checks, input calldata and every per-call result are in results.json. Case counts are coverage counts, not a workload-weighted score.

| Library | Comparable / excluded cases | Safe Solar wins / ties / losses | Worst gas delta |
|---|---:|---:|---:|
| Base64 | 177 / 0 | 7 / 0 / 170 | +81549 |
| ECDSA | 351 / 0 | 192 / 0 / 159 | +8933 |
| EfficientHashLib | 847 / 0 | 724 / 0 / 123 | +32340 |
| LibBit | 6563 / 2 | 6529 / 0 / 34 | +121908 |
| LibBytes | 4325 / 12 | 3095 / 0 / 1230 | +239198 |
| LibSort | 10628 / 0 | 10573 / 1 / 54 | +34877 |
| LibString | 3190 / 45 | 2170 / 0 / 1020 | +239152 |
| MerkleProofLib | 1145 / 0 | 785 / 0 / 360 | +7516 |
| SafeCastLib | 660 / 0 | 636 / 0 / 24 | +68 |
| SignatureCheckerLib | 468 / 0 | 136 / 0 / 332 | +40670 |

The same comparison after each selector dispatch enters its wrapper, which separates library code from the selector's place in each compiler's dispatch. Shared prologues count on whichever side each compiler places them: solc sets the free-memory pointer before dispatching, so the first memory expansion counts against its dispatch, while solar checks the call value there. Cases whose dispatch shape is not recognized are left out.

| Library | Measured cases | Safe Solar wins / ties / losses after dispatch | Worst gas delta |
|---|---:|---:|---:|
| Base64 | 177 | 7 / 0 / 170 | +81597 |
| ECDSA | 351 | 149 / 0 / 202 | +8935 |
| EfficientHashLib | 839 | 521 / 0 / 318 | +32409 |
| LibBit | 6563 | 6528 / 0 / 35 | +121956 |
| LibBytes | 4325 | 2874 / 2 / 1449 | +239293 |
| LibSort | 10628 | 10378 / 0 / 250 | +34905 |
| LibString | 3190 | 1992 / 0 / 1198 | +239246 |
| MerkleProofLib | 1145 | 767 / 0 / 378 | +7564 |
| SafeCastLib | 660 | 660 / 0 / 0 | -36 |
| SignatureCheckerLib | 468 | 86 / 0 / 382 | +40698 |

Runtime bytes for identical harnesses (partial libraries still omit the missing APIs):

| Harness | solc-upstream-legacy | solc-upstream-ir | solc-safe-legacy | solc-safe-ir | solar-upstream | solar-safe |
|---|---:|---:|---:|---:|---:|---:|
| Base64Harness | 1300 | 1672 | 4642 | 5509 | 935 | 3265 |
| ECDSAHarness | 2856 | 2390 | 4259 | 3257 | 1815 | 2190 |
| EfficientHashLibHarness | 4434 | 3382 | 5937 | 4267 | 2541 | 3048 |
| LibBitHarness | 3334 | 3056 | 5158 | 4740 | 3016 | 3809 |
| LibBytesHarness | 6847 | 7251 | 12767 | 11343 | 5649 | 9380 |
| LibSortHarness | 5791 | 6163 | 57231 (over EIP-170) | 37216 (over EIP-170) | 5084 | 26031 (over EIP-170) |
| LibStringHarness | 8558 | 9242 | 21046 | 21054 | 7177 | 16334 |
| MerkleProofLibHarness | 1805 | 1501 | 3118 | 2208 | 1135 | 1677 |
| SafeCastLibHarness | 7352 | 6596 | 7378 | 5593 | 3354 | 3221 |
| SignatureCheckerLibHarness | 3102 | 2728 | 5270 | 4299 | 2343 | 3193 |

Compatibility failures (retained as failures; the runner exits nonzero):

| Library | API | Variant | Mismatching cases |
|---|---|---|---:|
| LibBit | `toNibbles(bytes)` | solar-upstream | 2 |
| LibBit | `toNibbles(bytes)` | solc-upstream-ir | 2 |
| LibBit | `toNibbles(bytes)` | solc-upstream-legacy | 2 |
| LibBytes | `directReturn(bytes[])` | solar-upstream | 5 |
| LibBytes | `directReturn(bytes[])` | solc-upstream-ir | 5 |
| LibBytes | `directReturn(bytes[])` | solc-upstream-legacy | 5 |
| LibBytes | `split(bytes,bytes)` | solar-upstream | 4 |
| LibBytes | `split(bytes,bytes)` | solc-upstream-ir | 4 |
| LibBytes | `split(bytes,bytes)` | solc-upstream-legacy | 4 |
| LibBytes | `uint8At(struct LibBytes.BytesStorage,uint256)` | solar-upstream | 3 |
| LibBytes | `uint8At(struct LibBytes.BytesStorage,uint256)` | solc-upstream-ir | 3 |
| LibBytes | `uint8At(struct LibBytes.BytesStorage,uint256)` | solc-upstream-legacy | 3 |
| LibString | `split(string,string)` | solar-upstream | 4 |
| LibString | `split(string,string)` | solc-upstream-ir | 4 |
| LibString | `split(string,string)` | solc-upstream-legacy | 4 |
| LibString | `toHexString(uint256,uint256)` | solar-upstream | 19 |
| LibString | `toHexString(uint256,uint256)` | solc-upstream-ir | 19 |
| LibString | `toHexString(uint256,uint256)` | solc-upstream-legacy | 19 |
| LibString | `toHexStringNoPrefix(uint256,uint256)` | solar-upstream | 19 |
| LibString | `toHexStringNoPrefix(uint256,uint256)` | solc-upstream-ir | 19 |
| LibString | `toHexStringNoPrefix(uint256,uint256)` | solc-upstream-legacy | 19 |
| LibString | `uint8At(struct LibString.StringStorage,uint256)` | solar-upstream | 3 |
| LibString | `uint8At(struct LibString.StringStorage,uint256)` | solc-upstream-ir | 3 |
| LibString | `uint8At(struct LibString.StringStorage,uint256)` | solc-upstream-legacy | 3 |

No production safety claim or all-input equivalence proof is made by these tests. Missing APIs, malformed-input contracts, memory aliasing and storage layout require separate review.
