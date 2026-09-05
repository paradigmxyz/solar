# EVM rewrite baseline — 2026-09-05

Revision: `9cb036c034649f9bb0c510f05e02cef477e19ab4`; initially clean. No compiler implementation was changed. Debug build; solc 0.8.36, submodule `8a079791d9cca7a6c03fd6a8429b93aa3bddefed`. `target/codegen-bench/evm-rewrite-baseline-9cb036c/manifest.json` records host, Rust/tool versions, executable hashes, input hashes and all 40 deletion paths. `solar-debug` is comparison-only evidence, never an implementation fallback.

| Lane (compiler under rewrite) | Cases | Creation bytes | Runtime bytes | Hot-call gas | Sum compile seconds | Max peak RSS bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Runtime gas mode | 15 | 121,544 | 116,656 | 5,116,867 | 4.300 | 57,102,336 |
| Runtime size mode | 15 | 117,720 | 113,051 | 5,189,683 | 4.625 | 70,021,120 |
| Archived whole projects | 9 | n/a | n/a | n/a | 103.940 | 886,018,048 |
| UI gas screen | 694 successful / 702 | 1,393,537 | 1,069,788 | n/a | 21.805 | 29,216,768 |
| UI size screen | 694 successful / 702 | 649,080 | 543,106 | n/a | 23.273 | 44,363,776 |

All 24 main benchmark cases compiled on both compilers; all 15 runtime cases matched recorded runtime observations. Both optimization modes captured 175 gas labels and passed deployment/gas/runtime checks. Runtime gas mode uses archived optimizer settings (runs 200 or 1000); the size adapter sets enabled/runs=1 for both compilers before fingerprinting. Three compile samples were requested; the standard harness may stop repeating compiles taking at least ten seconds. The JSON retains actual samples. UI timing is one sequential sample per source/mode and a screen, not a statistically robust speed comparison. Heavy project settings remain unchanged and participate only in compile time/RSS.

`cargo nextest run --workspace --no-fail-fast --success-output immediate --failure-output immediate`: 1,527 passed, two declared ignored. Its integration harness reports 10,761 passing cases and 806 filtered cases. Separate `cargo tq ui`, `cargo tq standard-json`, and `cargo tq foundry` runs passed. The ignored Rust tests are `solar-lsp::flycheck_tests::fake_json_emitter` (test helper) and `solar-tester::foundry::tests::external` (optional external suite). Foundry discovered all 35 in-repository projects. Logs preserve individual case/revision IDs.

`solsymdiff` recorded bounded agreement for `InternalCallStackReturn.stackAcross(uint256)` in gas and size modes. This is bounded evidence for one selected function, not an unrestricted equivalence proof. Both generated projects and result files are preserved under `symbolic-gas/` and `symbolic-size/`.

The corpus excludes `uniswap-v2-pair` because its pragma is incompatible with pinned solc 0.8.36. Each UI mode records the same eight failed screen compilations: `lowering/abi_head_size_overflow.sol`, `lowering/calldata_array_subslice_dynamic.sol`, `lowering/contract_creation_self_cycle.sol`, `lowering/event_topic_limit.sol`, `lowering/member_call_unresolved.sol`, `lowering/yul_call_errors.sol`, `lowering/yul_ext_calls.sol`, `lowering/yul_slotnum.sol`. These remain rows with exact diagnostics; the annotation-aware correctness suite passed. One auxiliary Solidity source is fingerprinted but not a primary screen case. Never remove these rows to improve totals.

Two initial focused commands rejected `--format`; `cargo tq ui codegen` then selected zero nextest wrapper tests. These invocation errors are retained and were superseded by successful `cargo tq ui` and `cargo tq standard-json`. Use the verified commands, not those failed invocations. This is a runner-filter limitation, not a compiler test failure.

`commands.txt` records exact commands; `lane-status.json` and `symbolic-status.json` record exits; `case-index.json` inventories benchmark IDs/contracts/labels and UI IDs/modes/statuses; `known-failures.json` retains non-successes and exclusions. `corpus-baseline.json`, `hot-baseline.json`, `size-hot-baseline.json`, and `ui-baseline.json` are immutable comparison inputs. `artifacts/` and `size-artifacts/` retain compiler inputs/outputs, MIR, EVM IR, disassembly and bytecode; heavy inputs remain in the pinned, hashed project archives. The saved UI screen and size adapter are benchmark-only tools. Their CLI commands supply explicit repository-root paths; run them from the repository root.

The sibling archive `evm-rewrite-baseline-9cb036c.tar.gz` and its SHA-256 file package this directory. Preserve/extract that archive before cleaning `target/` or handing work to another checkout. `SHA256SUMS` verifies the extracted evidence. Reuse existing-success identities and input/settings fingerprints before aggregating any candidate result. Missing cases, dropped labels, failures, skips and changed outcomes must remain visible. Benchmark outputs include both compilers; the table above describes only the compiler being rewritten.
