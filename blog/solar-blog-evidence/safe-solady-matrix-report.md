# Safe Solady comparison

**The full API compatibility and equal-or-better gas target is not met.**

EVM: cancun; optimizer runs: 200; solc legacy and via-IR are both included.

362 function signatures implemented in 10 libraries; the pinned archive has 90 source files. This is a partial port.

28413 deterministic differential cases; 177 mismatching executions. 59 cases are excluded from performance comparisons because at least one implementation disagrees with the oracle.

Gas is the sum of executed opcode gas costs from debug_traceCall, excluding transaction intrinsic gas. Wrappers, inputs, optimizer runs and fork are identical. Calls do not share execution state. These library harness costs include ABI decoding, dispatch and encoding, and are not isolated instruction costs.

The reference is the cheaper upstream solc result for each call (legacy or via-IR). This envelope is stricter than choosing one compiler pipeline for a whole deployment. All six raw legs, revert checks, input calldata and every per-call result are in results.json. Case counts are coverage counts, not a workload-weighted score.

| Library | Comparable / excluded cases | Safe Solar wins / ties / losses | Worst gas delta |
|---|---:|---:|---:|
| Base64 | 177 / 0 | 156 / 6 / 15 | +355 |
| ECDSA | 351 / 0 | 292 / 0 / 59 | +49 |
| EfficientHashLib | 847 / 0 | 607 / 0 / 240 | +305 |
| LibBit | 6563 / 2 | 6559 / 0 / 4 | +28 |
| LibBytes | 4325 / 12 | 4075 / 2 / 248 | +10615 |
| LibSort | 10628 / 0 | 10581 / 0 / 47 | +14001 |
| LibString | 3190 / 45 | 1587 / 0 / 1603 | +20629 |
| MerkleProofLib | 1145 / 0 | 875 / 0 / 270 | +4222 |
| SafeCastLib | 660 / 0 | 606 / 0 / 54 | +178 |
| SignatureCheckerLib | 468 / 0 | 336 / 0 / 132 | +368 |

The same comparison after each selector dispatch enters its wrapper, which separates library code from the selector's place in each compiler's dispatch. Shared prologues count on whichever side each compiler places them: solc sets the free-memory pointer before dispatching, so the first memory expansion counts against its dispatch, while solar checks the call value there. Cases whose dispatch shape is not recognized are left out.

| Library | Measured cases | Safe Solar wins / ties / losses after dispatch | Worst gas delta |
|---|---:|---:|---:|
| Base64 | 177 | 156 / 0 / 21 | +380 |
| ECDSA | 351 | 243 / 0 / 108 | +54 |
| EfficientHashLib | 839 | 515 / 0 / 324 | +303 |
| LibBit | 6563 | 6549 / 0 / 14 | +76 |
| LibBytes | 4325 | 4028 / 0 / 297 | +10818 |
| LibSort | 10628 | 10566 / 0 / 62 | +14072 |
| LibString | 3190 | 2447 / 0 / 743 | +20186 |
| MerkleProofLib | 1145 | 868 / 0 / 277 | +4270 |
| SafeCastLib | 660 | 660 / 0 / 0 | -42 |
| SignatureCheckerLib | 468 | 195 / 6 / 267 | +383 |

Runtime bytes for identical harnesses (partial libraries still omit the missing APIs):

| Harness | solc-upstream-legacy | solc-upstream-ir | solc-safe-legacy | solc-safe-ir | solar-upstream | solar-safe |
|---|---:|---:|---:|---:|---:|---:|
| Base64Harness | 1300 | 1673 | 4642 | 5510 | 935 | 6670 |
| ECDSAHarness | 2856 | 2391 | 4259 | 3258 | 1752 | 2126 |
| EfficientHashLibHarness | 4434 | 3383 | 5937 | 4268 | 2537 | 3087 |
| LibBitHarness | 3334 | 3057 | 5158 | 4741 | 3033 | 3906 |
| LibBytesHarness | 6847 | 7252 | 12767 | 11344 | 5904 | 12609 |
| LibSortHarness | 5791 | 6164 | 57231 (over EIP-170) | 37217 (over EIP-170) | 5080 | 27995 (over EIP-170) |
| LibStringHarness | 8558 | 9243 | 21046 | 21055 | 6572 | 17832 |
| MerkleProofLibHarness | 1805 | 1502 | 3118 | 2209 | 1259 | 2341 |
| SafeCastLibHarness | 7352 | 6597 | 7378 | 5594 | 3380 | 3241 |
| SignatureCheckerLibHarness | 3102 | 2729 | 5270 | 4300 | 2333 | 3009 |

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
