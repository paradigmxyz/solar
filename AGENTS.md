# AGENTS.md

Guidance for AI coding agents working in this repository.

## Project Overview

A fast, modular Solidity compiler in Rust, meant as a modern alternative to solc.
The tracked solc release is checked out in the `testdata/solidity` submodule for
comparing behavior and porting tests.

## Commands

```bash
cargo build                            # Build
cargo t                                # Run all tests with nextest (preferred)
cargo nextest run --workspace          # Run tests
cargo llvm-cov nextest --workspace     # Test coverage
cargo uitest                           # Run UI tests
cargo uibless                          # Update UI test expectations
cargo fmt --all                        # Format
cargo cl                               # Lint
cargo run -- file.sol                  # Run compiler
cargo run -- -Zhelp                    # Unstable flags help
cargo run -p solar-mir-interp -- f.mir # Run lowered MIR in the MIR interpreter
```

Filter tests with `cargo uitest <path-substring>` or
`cargo nextest run -p <crate> <test-name>`. Run focused tests while iterating and broader checks once the change settles.
For documentation-only changes, check prose, examples, and spelling; do not
build or run tests. Avoid plain `cargo test`.

NEVER RUN TESTS WITH `--all-features`: it enables `tracy`, whose per-process
overhead turns the UI tests into minutes of 100% CPU.

## Architecture

Pipeline: lex -> parse -> sema (AST -> HIR, typeck) -> MIR -> EVM IR -> bytecode.

- `crates/parse`: Solidity and Yul lexer and parser. `crates/ast`: AST and
  visitors.
- `crates/sema`: `ast_lowering/` (name resolution, AST -> HIR), `hir/`, `ty/`,
  `typeck/`, `output/` (ABI, NatSpec, storage layout).
- `crates/codegen`: `mir/lower/` (HIR -> MIR), `mir/transform/` (one file per
  MIR pass), `mir/pass.rs` (pass registry and pipelines),
  `backend/evm/codegen/` (instruction selection, stack scheduling),
  `backend/evm/ir/passes/` (EVM IR passes), `backend/assembler/`, `target.rs`
  (cost model).
- `crates/interface`: sources, spans, diagnostics, symbols.
  `crates/data-structures`: index types, bitsets, arena helpers.
  `crates/config`: options and `-Z` flags (`opts.rs`; bless
  `tests/ui/cli/Zhelp.stdout`). `crates/cli`: driver.
  `crates/solar`: binary and test entry point. Also `lsp`, `lint`, `capi`,
  `macros`.
- `tools/tester`: UI and integration test runners. `tools/xtask`: `cargo tq`.

### MIR and EVM IR

- **MIR** is the typed, function-based codegen IR. Put Solidity-aware and SSA
  optimizations here: mem2reg, inlining, CSE/GVN/PRE, SCCP, LICM, loop analysis.
- **EVM IR** is the lower, Machine-IR-like layer, after calls and virtual values
  are gone: basic blocks of opcodes, explicit `dupN`/`swapN`/`pop`, and explicit
  terminators. Put target-specific CFG cleanup, block dedup and tail merging,
  cold-path handling, peepholes, outlining, block layout, and address-sensitive
  placement here.
- Stack scheduling sits at the MIR-to-EVM boundary. It keeps value identities
  and virtual stack layouts private and emits scheduled EVM IR directly.
- Keep the assembler primitive: a compact stream of opcodes, labels, deferred
  pushes, and immutable placeholders, solved to a least fixed point of label
  offsets and PUSH widths. Never add optimizations to this stream; add them to
  block EVM IR.
- MIR must not learn EVM stack layout; EVM IR must not rediscover Solidity types
  or call semantics.

### Debug Info

Debug info must never change codegen: no changes to executable MIR or EVM IR,
optimization decisions, stack scheduling, layout, or bytecode to keep metadata,
and no kept instructions, duplicated code, or disabled optimizations to create
a source checkpoint. Keep valid metadata when possible; otherwise mark it
dropped or unknown rather than inventing a location or picking an arbitrary
origin for shared code. Explain deliberate limits with a `NOTE:` comment. Cover
debug-info changes with bytecode-neutrality tests.

### MIR Phases

MIR has two phases, `semantic` (typed SSA, aggregates, semantic operations) and
`lowered` (word SSA and backend-supported operations; calls and phis survive
until stack scheduling). Required conversion passes in between stay small and
named and keep SSA and type invariants. `lower-evm-shaped` checks legality and
calls `Module::advance_phase`; the backend takes the checked `LoweredModule`.
Conversion errors stop the pipeline, even with `-Zmir-pipeline`; never skip a
lowering silently. Add instruction legality to `Instruction::unlowered_reason`
and type/module rules to the phase verifier, and test them under
`tests/ui/codegen/mir/`. See [docs/MIR.md](docs/MIR.md#phase-model).

### Operation Schema and ISLE Rules

Declare MIR operations only in `crates/codegen/src/mir/op_schema.rs` and EVM
opcodes only in `backend/evm/op.rs`; never add a parallel `match` that
classifies operations elsewhere. Write rewrite rules in ISLE under
`crates/codegen/isle/`, and never hand-edit the generated `prelude.isle` or
`extractors.isle`; regenerate them with
`SNAPSHOTS=overwrite cargo nextest run -p solar-codegen isle_prelude`. Rules
that affect execution need runtime coverage, and word rules need the
`scripts/evm-rules/` checker. Read [docs/CODEGEN.md](docs/CODEGEN.md) before adding
operations, writing rules, or extending the `egraph` pass.

### Target Cost Model

Price every choice between equivalent code shapes with the cost model in
`crates/codegen/src/target.rs`. Never write gas or byte literals in passes, the
stack scheduler, or the backend; add a tier or query to the model and pin it
with a unit test there.

### Visitor Pattern

Use `type BreakValue = Never` if the visitor never breaks. Override `visit_*` and
always call `walk_*` to continue traversal:

```rust
fn visit_expr(&mut self, expr: &'ast Expr) -> ControlFlow<Self::BreakValue> {
    // Your logic here.
    walk_expr(self, expr)
}
```

## Testing

- Prefer UI tests (`tests/ui/`) over unit tests for end-to-end behavior,
  especially diagnostics, sema, and compiler output.
- For codegen tests, use `//@ codegen-matrix: standard` (`none`, `gas`, `size`,
  `mir` revisions) unless it cannot express the test.
- To test one source under different flags, passes, levels, EVM versions, or
  outputs, use one test with `//@ revisions:` and revision-scoped directives.
  Use separate files only when the source text itself differs in purpose.
- Put imported or secondary sources in `auxiliary/` next to the test, never
  `aux/` (Windows rejects it).
- Assert formatted output in Rust tests with `snapbox` snapshots, not
  `.contains(...)`.
- Python: run `bash scripts/check-python.sh` and use uv, never pip; see
  [CONTRIBUTING.md](CONTRIBUTING.md#python-tooling).

### Compiler comparisons

For requested comparisons with solc, use
`uv run --project tools/compiler-diff compiler-diff` from the repository root
and read its [guide](tools/compiler-diff/README.md) first. Never hide errors or
unsupported checks with expectations. Report coverage counts, reduce new
failures into regression tests, and record accepted differences in
[SOLC_DIVERGENCE.md](docs/SOLC_DIVERGENCE.md).

### Codegen / MIR Pass Tests

Every MIR and EVM IR pass module starts with module docs that let a reviewer
understand it without the code: what it rewrites, the analysis or algorithm,
the main safety and profitability limits, its place in the pipeline, and any
deliberate omissions. A one-line restatement of the name is not enough.

- Before adding a pass, check `mir/transform/` and `backend/evm/ir/passes/`
  for one to extend.
- Test pass behavior with UI tests, by layer:
  - Solidity-to-IR lowering: `tests/ui/codegen/lowering/`.
  - MIR passes: `tests/ui/codegen/mir/<pass-name>/` (command-line pass name).
  - `lower-abi`, `lower-dispatch`, `lower-evm-shaped`:
    `tests/ui/codegen/mir/lowering/`.
  - EVM IR passes: `tests/ui/codegen/evm-ir/<pass-name>/` (`-Zevm-ir-pipeline`
    name).
  - Round-trip, pipeline, and validation tests: existing `none/`, `pipeline/`,
    `validation/`.
- Keep `.stdout`/`.stderr` expectations beside their source.
- Never write Rust unit tests that run whole passes; unit-test only small pure
  helpers.
- In Rust tests of bytecode, snapshot the disassembly, never raw bytes or
  offsets.
- Check pass output with MIR snapshots or FileCheck, then add runtime or
  differential tests when execution can change.
- Keep pass adapters small and beside the transform. The pass manager only
  coordinates names, pipelines, and `dyn ModulePass` execution.

### UI Test Annotations

```solidity
//@ compile-flags: --emit=abi
contract Test {
    uint x; //~ ERROR: message here
    //~^ NOTE: note about previous line
}
```

Annotations: `//~ ERROR:`, `WARN:`, `NOTE:`, `HELP:`, `ICE:`, and
`//~ diagnostic_code`. `^`/`v` point to lines above/below, `|` adds another
annotation for the same line, and `?` marks a diagnostic outside the test file.

The runner infers the exit status: 1 with `ERROR` or `ICE` annotations, else 0.
Add `//@ check-pass`, `//@ check-fail`, or `//@ failure-status: N` only when the
inferred status is wrong.

Other directives: `//@ compile-flags: ...`, `//@[rev] compile-flags: ...`,
`//@ ignore-host: windows`, and:

- `//@ run-call: add 1, 2 => 3` deploys a fresh contract, calls the function,
  and compares the ABI-encoded result; `//@ run-call-fail: fail()` requires a
  failure. See the [tester guide](tools/tester/README.md#runtime-directives)
  for settings. Use them for one isolated call; use `tests/foundry/`
  (`cargo tq foundry`) for multiple transactions, state, actors, events,
  cheatcodes, or complex setup.
- `//@ filecheck: ARGS` runs LLVM FileCheck on the `.stdout` with `ARGS`.

Use FileCheck when full snapshots are too brittle or the test checks order,
presence, or absence. Put `// CHECK:` lines immediately above the function or
block they cover, specific enough to catch the bug, anchored with `CHECK-LABEL`
when output has several sections. Use the default `CHECK` prefix unless
revisions need more. Keep patterns short, capture changing values with
`[[NAME:regex]]`, and keep one function's checks in one comment block.

`cargo tq foundry-external [name]` runs real-world projects as a local-only
differential suite; see the
[tester guide](tools/tester/README.md#external-foundry-suite). Any fix it finds
lands with a reduced regression test.

### Porting Tests from Solc

Read the solc test in `testdata/solidity` when porting. Split
`==== Source: ... ====` sections into `auxiliary/` and fix imports. When the
port keeps the test's semantics one to one (renames are fine), add
`// ported-from: test/libsolidity/.../name.sol`, one line per upstream file, no
trailing punctuation, after the leading `//@` directives or at the top. To
update the tracked solc version, follow
[CONTRIBUTING.md](CONTRIBUTING.md#updating-solc).

## Diagnostics Style

- No full stops at the end of messages.
- Quote code with backticks, not double quotes.
- Keep the main message short.
- Reuse solc's code for diagnostics solc also emits (so `--allow` matches for
  warnings); never invent codes.
- Return `Result<(), ErrorGuaranteed>` rather than `bool` from emitting code
  where practical, and pass the guarantee to `mk_ty_err`. Never use
  `ErrorGuaranteed::new_unchecked()` when a real guarantee exists.
- Add context with `note` (why), `help` (how to fix), and `span_note` (related
  code).

```rust
self.dcx()
    .err("cannot override non-virtual function")
    .code(error_code!(4334))
    .span(base.span)
    .span_note(overriding.span, "overriding function is here")
    .help("add `virtual` to the base function to allow overriding")
    .emit();
```

## Commits and PRs

- Use conventional commits, `type(scope)!: description` (feat, fix, perf,
  chore, docs, test, refactor; scope and `!` optional); PR titles match. Check
  `git log` for local style.
- Follow the 50/72 rule: imperative subject of at most 50 characters, no
  trailing period, blank line, body wrapped at 72. Add a body for perf (with
  measurements), fixes, and complex changes.
- PR descriptions explain what and why in prose: real measurements only, linked
  issues/PRs, no templates, bullet lists, essays, or "tested with" boilerplate.
  Write bodies with real newlines (file or heredoc), never `\n`.
- Self-review the final diff; repeat a full review only for large changes or
  open risks. Check CI and reviews once after publishing; do not poll.

## Code Style

- Comments end with periods (except URLs); files end with LF.
- Follow existing patterns; fix the existing path before adding infrastructure.
- Never expose secrets.

### Rust

- Add items (functions, `impl`s, modules, imports, dependencies) at the bottom
  of their scope or group, constructors at the top of an `impl`, matching the
  file's existing order and grouping.
- Doc comments go before all attributes. Module docs use `//!` at the top of the
  module file.
- Imports go at the top of the file, never inside functions unless a `#[cfg]`
  requires it. Order: one `use` group, then `pub use`. Write `mod x;` before
  `pub use x;`; for external re-exports, `use x;`, blank line, `pub use y;`,
  blank line, then local `mod m; pub use m::*;`.
- Conditional imports follow unconditional ones after a blank line, grouped by
  identical `#[cfg]` with blank lines between groups (also inside nested
  modules and `pub use`). Merge same-crate imports within a group. Keep the full
  condition on each `use`; no import-only modules or macros.
- Test-only imports go in the `#[cfg(test)]` module, which starts with
  `use super::*`; keep parent-level test imports only when several test modules
  or helpers need them. Keep crate-level anchors like `#[cfg(test)] use cc as _;`.
- In `Cargo.toml`, group a feature's optional dependencies under a comment with
  the feature name, such as `# jit`.
- Use `let ... else` for a single early exit instead of a `match`; use one
  `if let` chain when several patterns gate a block, including loop bodies.
  Never nest `if let`.
- Borrow with `&`/`&mut` instead of `ref`/`ref mut`.
- Use map entry APIs instead of a lookup followed by an insert.
- Omit type hints unless inference fails; then prefer turbofish.
- Leave a blank line between items and item groups (imports count as one),
  except for one struct with one `impl` or a run of similar `impl`s.

### IR construction and rewrites

- Generated MIR scalars are `i1`, `i160`, `i256`, or `memptr`; structs and
  slices keep their types. `memptr` is opaque, like LLVM's `ptr`: accessing
  operations carry the layout. Keep source widths, signedness, and ABI encodings
  in operation or layout metadata.
- `iN` names bit width. MIR syntax accepts any positive 32-bit width, but source
  lowering emits only `i1`, `i160`, and `i256`; other widths need EVM IR
  lowering support first.
- Every `iN` value has zero bits above N, including arguments, loads, call
  results, phis, and constants, so optimizations need not clean it. Raw memory
  and assembly values stay `i256` until an explicit cast.
- Addresses are `i160`: narrow with `trunc i256 v to i160`, widen with
  `zext i160 v to i256`, and keep the width until EVM IR.
- Every `i1` is exactly 0 or 1, including values from inline assembly; normalize
  raw words with `ne v, 0`. Branches and selects take only `i1`.
- Cast explicitly between value types; never retag a value or rely on equal
  storage width. Use LLVM casts (`trunc`, `zext`, `sext`, `ptrtoint`,
  `inttoptr`) with `type v to type` syntax; `trunc` to `i1` keeps the low bit
  and does not test for nonzero.
- Zero tests are `eq v, 0` and `ne v, 0`; `ISZERO` belongs to EVM IR. Rewrites
  keep value and type; bool-to-word needs `zext i1 v to i256`.
- `memptr` is not an integer; pointer casts do not establish validity,
  provenance, ownership, or non-wrapping arithmetic.
- Keep raw Solidity bool and address bits as `i256` where assembly can see them;
  convert to `i1` for logic and branches.
- Put an IR or pseudo-IR comment, in emitted order and one instruction per line
  where that helps, directly above each piece of code that writes, moves, or
  rearranges IR: the match arm, branch, loop, or builder sequence, not the top of
  a large function. No `IR:` label, prose, or full stop.

## Notes

- Use `IndexVec<I, T>` for every collection indexed by `I`, including locals;
  repeated `x.index()` signals the wrong collection.
- Audit `IndexVec`s for sentinel entries (`None`, empty, zero, max ID). Switch to
  `FxHashMap<I, T>` only when measured occupancy shows sentinels dominate.
- Never use `Vec<bool>`. Use fixed dense or mixed bitsets for stable domains,
  growable bitsets when indices grow while the set lives, and hash sets for
  sparse or unbounded domains. Iterate with the bitset's iterators, never by
  probing `0..domain_size`.
- Compare symbols with `sym::name`/`kw::Keyword`, not `.as_str()`. Never call
  `Symbol::intern` on a literal; add it to `symbols!` in
  `crates/interface/src/symbol.rs`.
- Allocate AST nodes in the arena; write in place where possible, else use
  `alloc_vec`, `alloc_smallvec`, `alloc_from_iter`, and friends in
  `crates/data-structures/src/bump_ext.rs`.
- Do not call the project "Solar" in the third person; say "we" or "the
  compiler". `docs/SOLC_DIVERGENCE.md` may contrast `solar` with `solc`.

## Codegen Benchmarking

Rank changes by correctness and `-Ogas` runtime gas, then `-Ogas` and `-Osize`
bytecode size, then compile time and memory; reject changes that only speed up
compilation. Record a baseline before editing, and follow the
[local build workflow](benches/runtime/README.md#comparing-local-builds). For
parser benchmarks, see [benches/README.md](benches/README.md).
