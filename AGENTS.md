# AGENTS.md

Guidance for AI coding agents working in this repository.

## Project Overview

Solar is a blazingly fast, modular Solidity compiler written in Rust, aiming to be a modern alternative to solc.

For testing and comparing behavior and semantics, the current tracked solc version (usually the latest stable release) is always available as a submodule `./testdata/solidity`.

## Commands

```bash
cargo build                            # Build
cargo nextest run --workspace          # Run tests (faster than cargo test)
cargo llvm-cov nextest --workspace     # Test coverage
cargo uitest                           # Run UI tests
cargo uibless                          # Update UI test expectations
cargo fmt --all                        # Format
cargo clippy --workspace --all-targets # Lint
cargo run -- file.sol                  # Run compiler
cargo run -- -Zhelp                    # Unstable flags help
```

DO NOT USE `cargo test` DIRECTLY IF YOU CAN AVOID IT.

NEVER RUN TESTS WITH `--all-features`. This enables "tracy" which has heavy overhead per-process, which the UI tests spawn lots of, increasing test times to minutes and 100% CPU for no reason.

## Architecture

- **solar-parse**: Lexer and parser
- **solar-ast**: AST definitions and visitors
- **solar-sema**: Semantic analysis (symbol resolution, type checking)
- **solar-codegen**: MIR construction, MIR optimizations, and EVM backend codegen
- **solar-interface**: Diagnostics and source management
- **solar-cli**: Command-line interface

Pipeline: Lexing -> Parsing -> Semantic Analysis -> MIR -> EVM backend -> bytecode

### MIR and EVM IR

- **MIR** is the compiler's higher-level codegen IR. It is typed, function-based,
  and is the right place for Solidity-aware and SSA-style optimizations such as
  mem2reg/frame-slot promotion, inlining, CSE/GVN/PRE, SCCP, LICM, and loop
  analysis.
- **EVM IR** is the lower, Machine-IR-like backend layer. It comes after
  function calls and virtual values have been lowered away. It models asm-like
  basic blocks with opcode-like instructions, explicit physical stack operations
  (`dupN`, `swapN`, `pop`), and explicit terminators such as jumps, returns,
  reverts, and stops. Use it for target-specific CFG simplification, terminal
  block deduplication and tail merging, cold/revert-path handling, backend
  peepholes, computation and constant outlining, block layout, and
  address-sensitive code placement.
- Stack scheduling belongs in the MIR-to-EVM lowering boundary. Keep MIR value
  identities and virtual stack layouts in the scheduler's private representation,
  materialize `dupN`/`swapN`/`pop`, and emit already-scheduled EVM IR directly.
- Keep the assembler primitive. Lower block EVM IR once into a compact stream
  containing only opcodes, label definitions/references, deferred pushes, and
  immutable placeholders. The assembler resolves deferred values, computes the
  least fixed point of label offsets and PUSH widths, and emits bytes. PUSH
  widths cannot generally be selected in one forward pass because widening one
  forward reference can move a later target across another width boundary.
- Do not add CFG cleanup, peepholes, deduplication, outlining, layout, or other
  optimization logic to the compact assembly stream. Add those transforms to
  block EVM IR, where control-flow edges and block identity remain explicit.
- Keep the layers separate: MIR should not grow EVM stack-layout details, and
  EVM IR should not rediscover high-level Solidity typing or call semantics.

### MIR Phases

MIR is a phased IR, like rustc's MIR: a `Module` carries a `MirPhase`, phases
only move forward (the enum order is the lowering order), and the phase
round-trips through the text format as `@module Name` and `@phase ...` (printed
only when not the default). The phases, in order:

- `built`: fresh from HIR lowering — one MIR function per Solidity function,
  typed values, dispatch and ABI handling not yet materialized as MIR.
- `optimized`: the canonical pass pipeline has run
  (`run_default_pipeline_with_options` is the phase transition; ad-hoc
  `mir-opt` pass lists do not advance the phase).
- `abi`: each external function is a self-decoding wrapper — it decodes
  calldata into typed arguments and calls the original body as an internal
  function; the body keeps its fused external termination. Produced by the
  `lower-abi` pass.
- `dispatch`: the selector switch is an ordinary MIR `entry` function routing to
  the ABI wrappers through `tail_call` terminators (control transfers and does
  not return, matching the wrappers' external termination). Produced by the
  `lower-dispatch` pass, which requires the `abi` phase.
- `memory-lowered`: semantic memory-object layouts and accesses have been
  lowered through the selected memory-layout policy to physical pointer and
  word operations. Produced by the `lower-memory-objects` pass.
- `evm-shaped`: every call edge either returns or is an explicit `tail_call`
  (arguments included), the shape the backend expects. Produced by the
  `lower-evm-shaped` pass; argument-carrying tail calls are only formed for
  callees the backend statically frames, so their arguments store at
  compile-time frame addresses with no return address pushed.

The `lower-abi`, `lower-dispatch`, `lower-memory-objects`, and `lower-evm-shaped`
passes are progressive MIR-to-MIR lowering, moving dispatch, ABI handling, and
memory layout out of the backend. They run in the codegen pipeline and the
backend consumes the `evm-shaped` module, with the MIR `entry` as the runtime
prologue and `tail_call` lowered to a jump. A module where `lower-abi` bails —
when any external function has returns (the wrappers do not implement
returndata encoding yet), or there is no external interface — keeps its phase
and is dispatched by the backend. When extending them or adding the next phase,
make the transition a
named pass that advances the phase via `Module::advance_phase`, keep it
conservative (bail rather than miscompile — `lower-abi` skips dynamic types),
and pin it with `.mir` UI tests under `tests/ui/codegen/mir/`.

### Visitor Pattern

Use `type BreakValue = Never` if visitor never breaks. Override `visit_*` methods and always call `walk_*` to continue traversal:

```rust
fn visit_expr(&mut self, expr: &'ast Expr) -> ControlFlow<Self::BreakValue> {
    // Your logic here
    walk_expr(self, expr)  // Always use walk_* for child traversal
}
```

## Testing

- **Unit tests**: In source files
- **UI tests**: In `tests/ui/`, verify compiler output
- Prefer UI tests over unit tests for end-to-end Solidity behavior, especially
  diagnostics, semantic analysis, and compiler-output checks.
- For Rust tests that assert formatted output, use `snapbox` snapshots instead
  of scattered `text.contains(...)` assertions.
- Auxiliary files go in an `auxiliary/` subdirectory next to the UI test that needs
  imports or secondary source files. Do not use `aux/`: Windows rejects it.

### Codegen / MIR Pass Tests

- Prefer UI tests for MIR/codegen behavior. Organize codegen tests by layer:
  - Solidity-to-IR lowering tests go under `tests/ui/codegen/lowering/`.
  - MIR optimization tests go under `tests/ui/codegen/mir/<pass-name>/`, using
    the pass's command-line name for the directory.
  - Progressive MIR lowering pass tests (`lower-abi`, `lower-dispatch`, and
    `lower-evm-shaped`) go together under `tests/ui/codegen/mir/lowering/`.
  - EVM IR optimization tests go under `tests/ui/codegen/evm-ir/<pass-name>/`,
    using the `evm-opt` pass name for the directory.
  - Pass-free round-trip fixtures, pipeline tests, and validation tests belong
    in their existing `none/`, `pipeline/`, or `validation/` directories.
- Keep each fixture's `.stdout` or `.stderr` expectation beside its source
  file when moving or adding tests.
- Do not add Rust unit tests that execute whole optimization passes; they make
  pass APIs harder to refactor. Use unit tests only for small pure helpers.
- In Rust tests that assert generated EVM bytecode, disassemble it and snapshot
  the opcode text; do not compare raw byte arrays or individual byte offsets.
- Validate pass output with MIR snapshots or FileCheck-style UI expectations,
  then add runtime or differential tests when behavior can affect bytecode
  execution.
- Keep pass adapters small and colocated with the transform implementation. The
  central pass manager should only coordinate pass names, pipelines, and
  `dyn ModulePass` execution.

### UI Test Annotations

```solidity
//@ compile-flags: --emit=abi
contract Test {
    uint x; //~ ERROR: message here
    //~^ NOTE: note about previous line
}
```

Annotations: `//~ ERROR:`, `//~ WARN:`, `//~ NOTE:`, `//~ HELP:`, `//~ ICE:`,
and `//~ diagnostic_code`. Use `^` or `v` to point to lines above/below, `|`
to add another annotation for the same line, and `?` for a diagnostic without a
location in the test file.

The UI runner infers the expected exit status from annotations. `ERROR`, `ICE` 
annotations expect status 1; tests without them expect status 0.
Do not add `check-pass` or `check-fail` to ordinary tests.
Use an explicit status directive only when the inferred status is wrong
for the test.

Common file-level UI directives:

- `//@ compile-flags: ...`: Pass extra compiler flags for this test.
- `//@ check-pass`: Mark the test as expected to pass even if no inline
  diagnostic annotation appears in the primary file.
- `//@ check-fail`: Mark the test as expected to fail even if no inline
  diagnostic annotation appears in the primary file.
- `//@ failure-status: N`: Override the inferred exit status, for example when
  testing a nonstandard failure status.
- `//@ ignore-host: windows`: Skip a test on a specific host.
- `//@[name] compile-flags: ...`: Define revision-specific flags for tests with
  multiple revisions.
- `//@ filecheck: ...`: Run LLVM FileCheck against the generated `.stdout` file
  after the UI test. Arguments after `filecheck:` are passed directly to
  FileCheck, for example `--check-prefix=ABI` or
  `--implicit-check-not=UnusedSymbol`.

Use FileCheck when exact full-output snapshots are too brittle or when a test
needs to assert selected output properties such as ordering, presence, or
absence. Put `// CHECK:`, `// CHECK-LABEL:`, `// CHECK-NOT:`, and related
directives in the test source. Keep checks specific enough to fail for the bug
being covered, and prefer `CHECK-LABEL` to anchor checks to the relevant
contract/module section when the output contains multiple sections.

### Porting Tests from Solc

Always look at the corresponding Solc test when porting behavior. Solc is always
available in `./testdata/solidity`. Solc tests may embed multiple source files in
one `.sol` file with `==== Source: ... ====` annotations. When porting those
tests, split the secondary sources into the UI test's `auxiliary/` directory and
update imports accordingly.

Add attribution using:
`// ported-from: test/libsolidity/.../name.sol`. Use one line per upstream file.
Do not add a full stop or other trailing punctuation after the path.
Place these after initial UI metadata directives such as `//@ compile-flags`,
`//@ error-in-other-file`, and `//@ check-fail`; if the file has no UI metadata,
put the attribution at the top.
Only add attribution if you're actually porting the semantics of the test 1-1 from Solc, not just "covering the error message". Renames are OK.

### Updating Solc

When updating the tracked Solc version, inspect both the GitHub release notes
and the source diff before changing code. Use `gh release view vX.Y.Z -R
argotorg/solidity` for the release notes, and compare tags locally with
`git -C testdata/solidity diff vOLD..vNEW --stat` plus targeted diffs for
parser, lexer, analysis, `liblangutil/EVMVersion.*`, and changed tests.

Update `testdata/solidity` to the new tag, bump every local Solc version pin
such as `SOLC_VERSION` in workflows and the fallback in
`crates/config/build.rs`, and add any new EVM versions to
`crates/config/src/lib.rs`. If upstream changes the default EVM version, update
the default here and bless the affected CLI snapshots.

Always run the complete upstream Solidity test mode with `cargo tq
solc-solidity` and `cargo tq solc-yul`, without path filters. Update the Solc test ignore
lists in `tools/tester/src/solc/solidity.rs` and `tools/tester/src/solc/yul.rs`
only for tests that are still outside this compiler's implemented behavior.

## Diagnostics Style

Error messages should follow these conventions:

- **No full stops**: Error messages should not end with periods
- **Use backticks for code**: Use `` `identifier` `` instead of `"identifier"` for code references
- **Main message is concise**: Keep the primary error message short and direct
- **Propagate guarantees**: Code paths that emit diagnostics should return `Result<(), ErrorGuaranteed>` instead of `bool` where practical, and pass the emitted guarantee to `mk_ty_err` when producing an error type
- **Avoid unchecked guarantees**: Do not use `ErrorGuaranteed::new_unchecked()` when a real emitted diagnostic guarantee can be propagated
- **Use subdiagnostics**: Add context via `note`, `help`, and `span_note`:
  - `note`: Additional context about why the error occurred
  - `help`: Actionable suggestion for how to fix the error
  - `span_note`: Point to related code locations (e.g., "overridden function is here")

Example:
```rust
self.dcx()
    .err("cannot override non-virtual function")
    .code(error_code!(4334))
    .span(base.span)
    .span_note(overriding.span, "overriding function is here")
    .help("add `virtual` to the base function to allow overriding")
    .emit();
```

## Commit Messages

Default format (conventional commits): `type: description` (feat, fix, perf, chore, docs, test, refactor)

- Optional scope: `type(scope): description`, e.g. `fix(parser): handle empty input`, `chore(deps): bump alloy`
- Breaking changes: append `!` before colon, e.g. `feat(api)!: change return type`

- Check recent `git log` to match the repo's commit style before committing.
- Imperative mood, <50 chars, no period
- Include body for perf (with measurements), bug fixes, complex changes

## PR Titles

- Follow the same conventional format as commit messages: `type: description`.

## PR Descriptions

- Explain what and why in flowing prose
- Include real measurements only
- Do not include validation/testing boilerplate like "Validated with", "Tested with", or command lists unless explicitly requested.
- Link related issues/PRs
- No templates, no bullet lists, no essays
- NEVER pass escaped newlines (`\n`) in PR bodies; use real newlines via a file or heredoc.

## Code Style

- Comments end with periods (except URLs)
- Files end with LF and trailing newline
- Follow existing patterns
- Never expose secrets

### Rust

- Put doc comments before attributes, always: `/// ...` comes before `#[derive]`, `#[inline]`, `#[cfg]`, and every other attribute.
- Put module documentation at the top of the module file with inner doc comments (`//! ...`), not on the `mod` item in the parent module.
- NEVER put imports inside functions unless required for `#[cfg(...)]` gating. All imports go at the top of the file.
- Group all `use` imports together. Keep `pub use` imports in a separate group. For local module re-exports, write `mod x;` before `pub use x;`; for re-exporting another module or external crate, use `use x;`, then a blank line, then `pub use y;`, then a blank line before local `mod my_mod; pub use my_mod::*;`.
- Move ordinary test-only imports into the `#[cfg(test)] mod tests` module instead of gating them individually. Keep crate-level dependency anchors such as `#[cfg(test)] use cc as _;` at crate scope.
- In `Cargo.toml`, generally group optional dependencies for a feature together. Put a comment immediately above the group containing only the feature name, for example `# jit`.
- Prefer `let Some(x) = x else { return };` / `let Ok(x) = x else { return };` over `match x { Some(x) => x, _ => return }`.
- Use `let ... else` only for a single early-exit guard. When multiple conditions or patterns gate the same block, prefer a combined `if let` / `let` chain instead of several sequential `let ... else` statements.
- Use combined `if let` chains (`if let Some(x) = x && let Some(y) = y { ... }`) instead of nesting (`if let Some(x) = x { if let Some(y) = y { ... } }`).
- In loops, prefer an `if let` chain around the loop body over multiple `let ... else { continue };` statements when the body only runs if all patterns match.
- NEVER use `ref` / `ref mut` in patterns as the first resort. Always prefer borrowing the expression with `&` / `&mut` instead.
- Avoid specifying type hints in variables unless absolutely necessary (e.g. `HashMap<_, Vec<_>>` for `x.entry(y).or_default().push(z)` where type inference won't work). Rely on the compiler.
- When type hints are needed, prefer turbofish (`let x = Type::<X, Y>::new()`) over annotation (`let x: Type<X, Y> = Type::new()`).

## Notes

- **Typed index collections**: Use `IndexVec<I, T>` for every collection indexed by an `I` index
  type, including local variables; if code repeatedly indexes a collection with `x.index()`, it is
  probably using the wrong collection type.
- **Sparse index maps**: Audit every `IndexVec<I, T>` for default or sentinel entries, not only
  `Option<T>` and its `None` sentinel. Empty collections, zero counts, maximum IDs, and other
  distinguished values can also indicate sparse storage. Measure representative occupancy before
  converting; a sentinel alone does not make storage sparse. Use `FxHashMap<I, T>` and omit the
  sentinel only when it dominates enough to justify hashing instead of direct indexing.
- **Index sets**: Never use `Vec<bool>`; a bitset is always the more compact representation. Prefer
  fixed dense or mixed bitsets for compact, stable domains and growable bitsets when new indices
  may be allocated while the set is live. Use hash sets for sparse sets, especially when there are
  few entries or the domain is large or unbounded. Iterate set bits with the bitset's built-in
  iterators; never scan `0..domain_size` and test membership one index at a time.
- **Symbol comparisons**: Use `sym::name` or `kw::Keyword` instead of `.as_str()` for performance. Add new symbols to the `symbols! { ... }` list in `crates/interface/src/symbol.rs`.
- **No inline interning of fixed strings**: Never call `Symbol::intern("...")` with a string literal. Add the name to the pre-interned `symbols!` set and use `sym::name`; `Symbol::intern` is only for strings built at runtime.
- **Arena allocation**: AST nodes use arenas for performance.
- **Benchmarks**: See @benches/README.md to benchmark when working on performance-critical code.
- Do not describe Solar in the third person. This repository is the project:
  say "we", "this codebase", or "the compiler" instead of "Solar does",
  "Solar is", or "Solar supports".
  - Exception: `docs/SOLC_DIVERGENCE.md` may say `solar` when explicitly
    contrasting behavior with `solc`.

## Codegen Benchmarking

Rank codegen results in this order: `-Ogas` runtime gas and correctness,
generated bytecode size in both `-Ogas` and `-Osize`, then compiler time and
memory use. Treat compile time as a tie-breaker after output quality. Reject a
candidate whose only win is faster compilation.

Use two corpora for codegen work. The UI codegen files give a fast generated
size signal. The CI corpus in `walnuthq/solidity-compiler-benchmarks` is the
source of truth for runtime checks and gas.

Build the debug compiler in the current checkout, then use the benchmark
repository's `solar_bench.py` to record both optimization modes before editing.
Do not use release builds for routine local tests:

```bash
cargo build -p solar-compiler --bin solar
uv run ~/github/danipopes/solidity-compiler-benchmarks/solar_bench.py \
  --solar-only --solar target/debug/solar --solar-optimize gas \
  --corpus tests/ui/codegen --allow-failures \
  --output target/codegen-bench/ui-gas-baseline.json
uv run ~/github/danipopes/solidity-compiler-benchmarks/solar_bench.py \
  --solar-only --solar target/debug/solar --solar-optimize size \
  --corpus tests/ui/codegen --allow-failures \
  --output target/codegen-bench/ui-size-baseline.json
```

Record the baseline before editing. Rebuild and benchmark the candidate in the
same checkout with distinct output paths. Do not create extra worktrees or
isolated target directories for routine local comparisons. Compare the
successful test IDs before aggregating sizes: expected diagnostic fixtures fail
compilation, and a new failure must not make the candidate total look smaller.
Compare per-file deltas as well because an aggregate win can hide a large
regression in one contract.

For the CI corpus, use the `BENCHMARK_REF` and `SOLC_VERSION` pinned in
`.github/workflows/bench.yml`. Use the current benchmark checkout at that pin
and initialize its recursive submodules; do not create another worktree. Run
`solar_bench.py --suite all` as a quick size screen, then enable the hot gas
workload with `--gas --gas-profile hot --start-anvil` before accepting an
`-Ogas` candidate. Always write JSON with `--output`, retain the baseline JSON,
and compare the same test IDs and gas-call labels. `--allow-failures` keeps an
exploratory run going; it does not make compiler failures or runtime mismatches
acceptable. Use `--solar-optimize size` for the corresponding `-Osize` screen.

When tuning a pipeline, remove or move one pass group at a time. Record the
candidate name, exact ordering, UI gas/size reports, CI size report, and hot-gas
report under `target/codegen-bench/`. Use `-Ztime-passes` on a representative
large contract to see which repeated pass invocations still change IR, then
confirm every removal against both corpora. Equal byte counts are not enough:
compare serialized bytecode and keep the relevant IR snapshots canonical. A
`changed=false` result on one contract is not evidence that a pass is redundant.
