# Reconciliation addendum to the UniswapV2ERC20.transfer any-caller receipt

This addendum reconciles the receipt of the narrowed acceptance (solar 36f2dac2, replay
receipt sha256 59d0fa445ab192ff9849914372405fb755b4696e05990081154eb7aef259d5e8) with the
pinned solidity-lean commit and with solar head f71b7554. It changes no receipt: everything
below is checked against the evidence the run kept, plus two compiler builds on a small
machine. No Lean was rebuilt or replayed for it.

Scope stays as before: one pinned contract's `transfer(address,uint256)` at the runtime tier.
It is not a generic-driver certification, not the fresh-byte gate, and not general certification.

## 1. Overlay provenance

solidity-lean `alpharush/erc20-gates` e368ace557b37d10c9f6e9572dc1f31d166e0d29 is
9876392965758aafe6aabac435d0b9b40b1eb9c7 plus one commit that adds or changes exactly five
files (tree 10334872, see `git diff --stat 98763929 e368ace5`):

| file | blob | sha256 | replayed copy |
| --- | --- | --- | --- |
| Contracts/UniswapV2/ERC20/Certified/TransferExpected.lean (from 4332b47d) | 19de3cbf | 8573c694…c767c988c | same |
| lakefile.lean | cb5b4c44 | 9677fbcf…a724775 | same blob as the box's patched lakefile |
| Certified/erc20_any/Statement.lean | 916c6414 | 8addd629…d8f8481 | same |
| Certified/erc20_any/Proof.lean | 506cb006 | 73832835…e6ad5ac1 | same |
| Certified/erc20_any/Final.lean | 4d578213 | e9909f08…a2b50a54fb | same |

lean-toolchain (leanprover/lean4:v4.35.0) and lake-manifest.json (sha256 b2777a18…ca42)
are the pin's, unchanged, so the packages (Mathlib and the rest, fetched with
`lake exe cache get`) are those the pin names. Every other source file is the pin's.

Executed inputs that are NOT in any commit:

* `Certified/uv2_erc20/gen/UniswapV2ERC20/*` (the stage export, gitignored at the pin) and
  the uv2_erc20 proof modules that the slice scripts generate. Their identity rests on the
  stage manifest: the kernel proves `Manifest.stages = pinnedStages` (18 file sha256s) and
  `bytecode = runtimeBytes`, and a fresh build of 697fc686 reproduces all 18 stage files with
  exactly those sha256s (section 2). The box's Manifest.lean (dd296c8c…) differs from a fresh
  export's (564d2f49…). The export records its temporary output directory in its arguments,
  which differs on every run and was not kept, so the difference is expected but not shown to
  be only that path: the box's file is gone.
* `.cache/uniswap-v2/src` (the port) and `scripts/verify/uniswap-v2/UniswapV2ERC20.sol`:
  regenerated here from the pinned scripts; the three port sources' sha256s equal the
  Statement's `pinnedSources`.

Replay reuse: the source, import and option identity of the replayed closure with e368ace5 is
shown above. Byte identity of the oleans a fresh build of e368ace5 produces is NOT shown: the
box is gone and comparing would mean rebuilding the closure. The receipt binds each of the
4028 replayed modules to its olean sha256; the sorted `module olean_sha256` inventory hashes
to 528a7926189d1a5279c9441970a271c8ec71e9bdf380175535488dbc82419f63, and the olean set's
digest was 2abd63dc… before and after the replay. Reusing the receipt for e368ace5 therefore
holds by source, import and option identity; a rebuild that rehashes the 4028 oleans against
that inventory would close olean identity, and is owed if that rule is required.

## 2. Cached compiler against f71b7554

Both compilers were built here with `cargo build --release --locked --bin solar`,
CARGO_BUILD_JOBS=4, and run on the same source with the same arguments the export used
(`-Ogas --evm-version osaka`, from Certified/uv2_erc20/options), in the same directories.

| | 697fc686 (rebuilt) | f71b7554 |
| --- | --- | --- |
| build exit, wall, peak RSS | 0, 117 s, 1,560,108 kB | 0, 105 s, 1,442,556 kB |
| binary sha256 | bab52226…abb7b | 15808eee…6381db |
| runtime from the flattened port | 1183 B, ca4c1925…971a0b, identical to runtimeBytes | 1200 B, 31f5c33c…11b420, differs |
| stage export bytecode | identical to runtimeBytes | 1200 B, differs |
| -Zemit-lean-stages changes the runtime | no | no |
| 18 stage file sha256s vs Manifest.stages | all 18 equal | 17 of 18 differ (Sched equal) |
| sources (3 sha256s) vs pinnedSources | equal | equal |
| arguments.take 3, evm version, optimization | -Ogas --evm-version osaka, osaka, Gas | same |
| compiler string | 0.8.38+commit.697fc686.solar.0.3.0-dev | 0.8.38+commit.f71b7554.solar.0.3.0-dev |
| ABI (16 entries) | | equal to 697fc686's except the version field |

Provenance-only differences: the compiler string in the manifest and the ABI's version field.
Semantic differences: every stage from Stage0 (lower-evm-shaped, the first MIR) on. The
first differing stage is Stage0: its value numbering and its blocks differ (146 changed
lines, such as a block that reverted with the return data now ending in a compare and a
branch). The Sched file's sha256 is equal while its MIR input differs. Storage layout cannot
be compared by emit: this compiler has no storage-layout output. `balanceSlot = 1` is a
Statement constant that the proofs check against the 697fc686 artifacts only.

**Result: the receipt does NOT reconcile to f71b7554.** It holds for the bytes 697fc686
emits, which a fresh 697fc686 build reproduces byte for byte at every stage. The open item is
certifying f71b7554's 1200-byte runtime, whose stages differ from Stage0 on. Stage identity,
where it holds, says nothing about the generic fresh-byte gate.

## 3. Claim manifest

Theorem `Certified.erc20_any.Final.final : Certified.erc20_any.Final.Statement`;
`#print axioms`: `[propext, Classical.choice, Quot.sound]`, transitively, with no sorryAx,
native_decide, ofReduceBool or trustCompiler anywhere in the check or replay output.

`Statement` is the conjunction of: `src.unit = importedSourceUnit` (solc's AST of the
flattened port: the declared frontend TCB); `Manifest.sources = pinnedSources`;
`Manifest.compiler = pinnedCompiler`; `Manifest.arguments.take 3 = pinnedOptions`;
`evmVersion = "osaka"`; `optimization = "Gas"`; `Manifest.stages = pinnedStages`;
`bytecode = runtimeBytes`; `∀ a dst, transferFootprint balanceSlot a dst =
[mappingPreimage a balanceSlot, mappingPreimage dst balanceSlot]`;
`∀ a, a < 2^160 → a ≠ selfAddress → ∃ call, specAny.Valid ⟨a⟩ call`;
`Expected src specAny runtimeBytes ∧ Expected src specScoped runtimeBytes`.

Sources are bound by manifest pins checked outside the kernel (gen.py and run.sh hash the
files; the kernel only compares the manifest's strings). There is no kernel hashing. A pin
binds identity; it is not a kernel proof that the parser's AST corresponds to the text.

Expanded:

* `Expected src spec bytes := bytes.all (· < 256) ∧ spec.Covered ∧ (spec.Assumed → ∀ ctx,
  spec.Contexts ctx → ∃ c, src.checked = .ok c ∧ CertifiedAt (src.semAt ctx) c ctx
  (runtimeCode bytes) (spec.Valid ctx) (spec.Allowed ctx) (spec.Post ctx) (spec.gas ctx))`.
* `CertifiedAt` = `TranslationAt` (inhabited; `EquivAt`: at every gas below 2^256, each run
  of the bytes equals the source's run or is `Inert`, i.e. failed with storage and logs
  unchanged; `Terminates`; `ResourcesAt`: from `gas ctx call` up, the bytes give exactly the
  source's observable) and `SpecProof` (every source result is `Allowed` and meets `Post`).
* `Spec.Covered`: a context exists, every context is `Ok`, each entry has a valid call, and
  some valid call violates `Post` and some failing one violates `Allowed` (non-vacuity).
* `Context.Ok ctx := ctx.caller < 2^160 ∧ ctx.caller ≠ selfAddress`.
* `specAny`: Contexts `Ok`; entry `transfer(address,uint256)` by selector 0xa9059cbb;
  `Valid = Slice.Valid deployed`: `∃ σ dst v, Covers deployed σ ∧ dst < 2^160 ∧ v < 2^256 ∧
  call = transferCall σ dst v`, with `transferCall` = ABI calldata, `callvalue = 0`, storage σ,
  and `deployed = [(3, domain separator)]`; `Allowed` = success or SafeMath's sub or add
  revert; `Post = SeqPostAt a ∧ (ScopeAt a → TargetPostAt a)`; gas 8199112;
  `assumptions = []`.
* `SeqPostAt a`: success iff `v ≤ bal a` and the debited state's `bal dst + v < 2^256`; on
  success storage = debit then credit reading the debited state, one Transfer log, output
  `be32 1`; on failure storage and logs unchanged, output the sub or add message.
* `ScopeAt a call := ∀ σ dst v, call = transferCall σ dst v → … →
  ScopedSep (transferFootprint balanceSlot a dst)`, `ScopedSep L := KeccakSeparated (· ∈ L)`:
  keccak is injective on those two preimages and their hashes are at least 2^128.
* `TargetPostAt a`: for `a ≠ dst`, success iff `v ≤ bal a ∧ bal dst + v < 2^256`, and on
  success the two balances are `bal a - v` and `bal dst + v`; for `a = dst`, success iff
  `v ≤ bal a` and storage is unchanged; on success one Transfer log, output `be32 1`, every
  other slot unchanged; on failure nothing changes.
* `specScoped`: Contexts `Ok ∧ ScopedSep (transferFootprint balanceSlot caller caller)`;
  `Valid = Slice.Valid deployed ∧ ScopeAt caller`; `Post = TargetPostAt caller`; same
  entries, Allowed, gas, `assumptions = []`.

Exclusions and assumptions, explicitly: callers at or above 2^160 and the contract's own
address are excluded; callvalue is 0; calldata is the ABI encoding of a clean address and a
word; storage covers the constructor's slot 3; the only hash qualification is the
separation of the caller's and recipient's balance preimages, and `TargetPostAt` holds in
`specAny` only under it. This is runtime partial correctness: the gas bound 8199112 bounds
when the bytes give the source's observable and is not a no-OOG guarantee; deployment, the
constructor, lifecycle and reentrancy are out of scope. The satisfiability witness is a
zero-value self-transfer: it shows some call is valid for each real caller, not coverage of
nonzero amounts or distinct recipients (those are in `Valid`'s quantifiers, not the witness).

## 4. Negatives

Run 2 (`neg.sh`, the receipt's run; results sha256 c802fdea…417a): one positive control and
13 negatives, 14 cases in all. Every Lean case's failure was checked to be the intended error.

| id | expected | actual | category |
| --- | --- | --- | --- |
| control | 0 | 0 | positive control |
| byte_first | 1 | 1 | corrupt bytes: `decide` refutes the changed export |
| byte_last | 1 | 1 | corrupt bytes: `decide` |
| source_pins | 1 | 1 | wrong source pins: `decide` |
| source_contract | 1 | 1 | wrong contract name: type mismatch with the proof |
| prop_credit_plus_one | 1 | 1 | false property: type mismatch |
| prop_self_no_overflow | 1 | 1 | false property: type mismatch |
| prop_unchecked_overflow | 1 | 1 | false property: type mismatch |
| ctx_self_caller | 1 | 1 | excluded context admitted: type mismatch |
| gas_minus_one | 1 | 1 | changed gas bound: type mismatch |
| footprint_dropped | 1 | 1 | hash qualification dropped: type mismatch |
| gen_mutated_emit | 1 | 1 | gen.py refusal: stage export bytecode is not the emitted runtime |
| src_stage_safemath | 1 | 1 | gen.py refusal: SafeMath.sol no longer matches its pin |
| src_stage_erc20 | 1 | 1 | gen.py refusal: UniswapV2ERC20.sol no longer matches its pin |

The type-mismatch cases show the proof does not establish the mutated spec, not that the
mutated spec is false.

Run 1 is an invalid attempt, not evidence: 8 of its cases exited 1 on unknown identifiers
(`byte_first` on `Export.bytecode`; `source_contract`, the three `prop_*`,
`ctx_self_caller`, `gas_minus_one` and `footprint_dropped` on stale theorem names), and its
harness counted them as passes. The earlier report said 7; it was 8. The harness now counts a
name or parse error as a FAIL.

## 5. Resources and attempts

Box: 48 cores, 377 GB RAM. Per-batch peak RSS is GNU time's `Maximum resident set size`
(the largest single process the batch waited for: leanchecker), not a sum. Alongside, a
sampler recorded every 2 s the machine's used memory (`free -k`) and the largest and summed
RSS of lean, lake, leanchecker, solar, cargo and rustc processes; it covered 03:07Z to 04:27Z,
the whole replay included. No limit was set on the replay (no ulimit, no cgroup cap).

All 11 replay batches ran at the same time (03:43Z to 04:26:34Z): the replay script's
parallelism argument (6) did not hold. Peak machine use was 341,495,072 kB at 04:24:24Z
(about 36 GB, 9%, headroom of 377 GB); summed checker RSS 338,699,412 kB. No batch was killed:
every batch and the account step exited 0 by normal exit, and every olean kept its digest.
The kernel log was not checked for OOM messages before the box was deleted.

| batch (first index) | modules | elapsed | peak RSS kB | exit |
| --- | --- | --- | --- | --- |
| 0 | 400 | 39:48 | 11,410,048 | 0 |
| 400 | 400 | 41:14 | 13,084,860 | 0 |
| 800 | 400 | 39:46 | 25,317,912 | 0 |
| 1200 | 400 | 41:44 | 30,806,404 | 0 |
| 1600 | 400 | 40:36 | 51,279,584 | 0 |
| 2000 | 400 | 40:39 | 52,361,756 | 0 |
| 2400 | 400 | 41:46 | 90,123,704 | 0 |
| 2800 | 400 | 43:03 | 106,027,224 | 0 |
| 3200 | 400 | 41:27 | 48,944,304 | 0 |
| 3600 | 400 | 43:22 | 150,703,364 | 0 |
| 4000 | 28 | 3:17 | 53,263,736 | 0 |

Inventory: the required set is the import closure of `erc20_any.Final` that
check_final.lean printed (toolchain and packages included), 4028 modules, every one planned
for replay (0 trusted by directory). The receipt lists the same 4028 names with the same olean
sha256s as the plan (inventory digest above); 0 unaccounted; leanchecker also replayed 39
Mathlib modules outside the plan, listed in the receipt. Checker: the toolchain's
bin/leanchecker, sha256 12c38901…ddb6.

Every nonzero or killed attempt:

* slice exit 1 (5.6 GB), slice2 exit 1 (2.9 GB), slice3 exit 0; a prebuild run beside the
  first slice was killed and not counted.
* acceptance build attempt 1 exit 1 (6.2 GB); attempt 2 exit 143, killed by hand at about
  244 GB RSS (one lean process, 244,579,576 kB at 03:37:22Z) while elaborating kernel SHA-256
  of the sources, which is why sources are bound by pins outside the kernel; attempt 3 exit 1
  (3.0 GB); final build exit 0 (3,034,680 kB); check_final exit 0 (8,334,664 kB).
* kernel-digest probes: digest_stage2 and digest_imported exit 134 at a 40 GB cap,
  digest_stage2 uncapped exit 1.
* negatives run 1: invalid, as above.
* Short leanchecker scaling probes ran beside the replay; they are not counted.
