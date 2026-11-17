# Head-bytes run: preparation for solar f71b7554

The next run certifies the runtime that solar f71b7554 emits for the pinned port, on the same
narrowed UniswapV2ERC20.transfer any-caller chain. Receipt 59d0fa44 (697fc686, 1183 bytes) is
immutable non-head evidence and nothing here reuses it.

## Candidate

| | |
| --- | --- |
| commit | f71b7554d6e044cf974ac4f0e5f0b184cd96ff9b |
| binary | `cargo build --release --locked --bin solar`, CARGO_BUILD_JOBS=4, sha256 15808eeef2f5d5d021372d43a36ef491492cc8b2fc84b2c3b97a666a736381db |
| version | solar 0.3.0-dev+commit.f71b7554 |
| flags | -Ogas --evm-version osaka (Certified/uv2_erc20/options) |
| runtime | 1200 bytes, sha256 31f5c33c…11b420 (emitted twice, identical) |

prep.sh and run.sh refuse any other binary or commit, and a stage export another compiler wrote.

## Generated for f71b7554 (solidity-lean 1f0db760)

The stage export, CtxGas.lean, ProofsT/ (18 modules), Ctx.lean and CtxNeg.lean are generated
by prep.sh from the candidate's own output. They are not committed (the port is GPL-3.0);
prep.sh writes the sha256 of every file the build reads to OUT/inputs.sha256. The stage proofs
come from tools/certify/certify.py and asm_ctx.py of this branch, because f71b7554's certify.py
imports solar-tv/milestone/cutpoints.py, which f71b7554 does not have. The tools commit is in
OUT/producer. All three generators exit 0 on f71b7554's export: the gas checker finds
transfer's selector at block 15, and asm_ctx accepts the bytes (they make calls: STATICCALL at
byte 920).

Certified/erc20_any/Statement.lean is regenerated (a8473b23…c7a9c) with the 1200 bytes, the
compiler string and the 18 stage sha256s. Proof.lean and Final.lean are unchanged byte for byte.

## Which proof parts are byte-specific

Redone from the candidate's output by the run (generated, then checked by the kernel):

* the stage export, and every module that imports it: ProofsT (each stage's refinement, cut
  block by block), CtxGas and Ctx (the bytes against the final EVM IR), CtxNeg;
* `bytes_eq` and `export_eq` (`decide +kernel` on the new bytes);
* the SourceMir modules that run the checker on Stage0 and its mutants.

Handwritten and depending on the bytes' shape, so they may fail and need a change:

* Gas.lean: `G_eq : G = 8199112` is `gasBoundL` over the guarded blocks and holds only if
  transfer's path still crosses 7 blocks of the same cost; `rejects_sstore` names the debit
  SSTORE by offset, now 0x438 (was 0x427; the 24 bytes before it are the same apart from a jump
  target 17 bytes further on). If `G` changes, the gas bound in the Statement changes with it.
* GasGuard.lean: `N = 7`, `guarded_run7` and `guarded_ends7` assume 7 steps of the
  guarded program.
* AnyCaller.lean: its proofs assume the stage composition from stage0 to evm11, and evm11 is
  still the last EVM IR stage (prep checks this).
* Contracts/UniswapV2/ERC20/Certified/Transfer, TransferFrom and TransferExpected: they compose
  the above and should hold unchanged if the parts above do.

Generic, not redone: the source side (the slice specification, SolidCore's semantics of the
source, `SeqPostAt`, `TargetPostAt`, `ScopeAt`, the footprint), the Expected framework,
the any-caller theorem in Verify/AnyCaller, and the satisfiability witness.

## The replay, with a real concurrency cap

The 59d0fa44 replay's cap failed because replay.sh counted jobs with `$(jobs -rp | wc -l)`,
always 0 in a subshell. sched.py replaces it:

* at most PARALLEL leanchecker processes in the whole process tree (counted by the sampler
  from /proc, not by the shell);
* a batch starts only if the larger of the machine's used memory and its starting use plus the
  running batches' estimated peaks, plus the new batch's estimated peak, stays within the
  budget (default 75% of memory, so at least 25% headroom). Estimates are 59d0fa44's per-batch
  peaks times 1.25 (replay-peaks.59d0fa44), heaviest first;
* a 2 s sampler writes used memory, checker count and summed tree RSS to samples.tsv and kills
  every batch if used memory passes the budget plus half the remaining headroom;
* gate.txt records the maximum concurrency and memory, and replay.sh fails if either limit was
  passed.

Tested here with dummy checkers: the cap held at 2 of 5 batches, and the memory reservation
held 4 allowed batches to 2.

Sizing on m4-metal-large (377 GiB): with 75% budget, the heaviest batches (150.7, 106.0 and
90.1 GB measured, 188, 133 and 113 GB estimated) cannot overlap freely. Simulated with
59d0fa44's 42-minute batches, PARALLEL 4 finishes in about 168 minutes (peak reservation
253 GiB). PARALLEL 2 takes about 252 minutes. That is longer than a box's 2 h lifetime, before
the build. Unless the run gets more than one box slot or a longer lifetime, the replay must be
split across slots and resumed. Each slot rebuilds the closure, and olean_diff.py checks that
both slots' oleans are identical before their accounts are merged.

## Olean identity, in the same run

After the build, replay.py's plan records each closure module's olean sha256. olean_diff.py
compares that plan with 59d0fa44's (inventory 528a7926…9f63) and lists every module that is
identical, differs, or appears in only one. The byte-dependent modules and those that import
them are expected to differ. The comparison is data only. It does not rebind 59d0fa44, and it
does not make e368ace5's compiled inputs identical to the replayed ones.

## What every head-bytes receipt keeps

Runtime tier only (no deployment, constructor, lifecycle or reentrancy); `TargetPostAt` only
under `ScopeAt`, the separation of the caller's and recipient's balance preimages; callers below
2^160 other than the contract, callvalue 0; sources bound by manifest pins checked outside the
kernel; the gas figure bounds when the bytes match the source and is not a no-OOG guarantee.
