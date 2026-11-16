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
cargo bless                            # Update all test expectations
cargo fmt --all                        # Format
cargo cl                               # Lint
cargo run -- file.sol                  # Run compiler
cargo run -- -Zhelp                    # Unstable flags help
```

Filter tests with `cargo tq <mode> <path-substring>` (bless with
`cargo bless <mode> <path-substring>`), and Rust tests with
`cargo nextest run -p <crate> <test-name>`. Run focused tests while iterating
and broader checks once the change settles. Avoid plain `cargo test`. For
documentation-only changes, check prose, examples, and spelling; do not build
or run tests.

NEVER RUN TESTS WITH `--all-features`: it enables `tracy`, whose per-process
overhead turns the UI tests into minutes of 100% CPU.

## Architecture

Pipeline: lex -> parse -> sema (AST -> HIR, typeck) -> MIR -> EVM IR -> bytecode.

- `crates/parse`: Solidity and Yul lexer and parser. `crates/ast`: AST and
  visitors.
- `crates/sema`: name resolution and AST -> HIR (`ast_lowering/`), `hir/`,
  `ty/`, `typeck/`, and `output/` (ABI, NatSpec, storage layout).
- `crates/mir` and `crates/evmir`: the data model, text format, and verifier
  of each IR; `crates/ir`: what both share. `crates/mir-interpreter` and
  `crates/evmir-interpreter` run them.
- `crates/codegen`: HIR -> MIR (`mir/lower/`), MIR passes (`mir/transform/`,
  one file per pass; registry and pipelines in `mir/pass.rs`), stack
  scheduling (`backend/evm/codegen/`), EVM IR passes
  (`backend/evm/ir/passes/`), the assembler, and the cost model (`target.rs`).
- `crates/interface`: sources, spans, diagnostics, symbols.
  `crates/data-structures`: index types, bitsets, arena helpers.
  `crates/config`: options and `-Z` flags (`opts.rs`; bless
  `tests/ui/cli/Zhelp.stdout`). `crates/cli`: driver. `crates/solar`: binary
  and test entry point. Also `lsp`, `lint`, `capi`, `macros`.
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
- Keep the assembler primitive: opcodes, labels, deferred pushes, and immutable
  placeholders, solved to a least fixed point of label offsets and PUSH widths.
  Add optimizations to EVM IR, never to the assembler.
- MIR must not learn EVM stack layout; EVM IR must not rediscover Solidity types
  or call semantics.

### MIR Phases

MIR has a `semantic` phase (typed SSA, aggregates, semantic operations) and a
`lowered` phase (word SSA and backend-supported operations). The conversion
passes between them stay small and keep SSA and type invariants;
`lower-evm-shaped` checks legality and advances the phase. Conversion errors
stop the pipeline, even with `-Zmir-pipeline`; never skip a lowering silently.
Add instruction legality to `Instruction::unlowered_reason` and type and module
rules to the phase verifier, and test them under `tests/ui/codegen/mir/`. See
[docs/MIR.md](docs/MIR.md#phase-model).

### Operation Schema and ISLE Rules

Declare MIR operations only in `crates/mir/src/op_schema.rs` and EVM opcodes
only in `crates/evmir/src/op.rs`; never add a parallel `match` that classifies
operations elsewhere. Write rewrite rules in ISLE under `crates/codegen/isle/`.
Never hand-edit the generated `prelude.isle` and `extractors.isle` files;
regenerate them with
`SNAPSHOTS=overwrite cargo nextest run -p solar-codegen isle_prelude`. Rules
that affect execution need runtime coverage, and word rules need the
`scripts/evm-rules/` checker. Read [docs/CODEGEN.md](docs/CODEGEN.md) before
adding operations, writing rules, or extending the `egraph` pass.

### Target Cost Model

Price every choice between equivalent code shapes with the cost model in
`crates/codegen/src/target.rs`. Never write gas or byte literals in passes, the
stack scheduler, or the backend; add a tier or query to the model and pin it
with a unit test there.

### Debug Info

Debug info must never change codegen: do not change executable MIR or EVM IR,
optimization decisions, stack scheduling, layout, or bytecode to keep metadata,
and do not keep instructions, duplicate code, or disable optimizations to keep a
source location. Keep valid metadata when possible; otherwise mark it dropped or
unknown rather than invent a location or pick an arbitrary origin for shared
code. Explain deliberate limits with a `NOTE:` comment, and cover debug-info
changes with bytecode-neutrality tests.

Pick locations by the first rule that applies:

1. **solc** decides which source span an expression or statement maps to: the
   `LocationSetter` calls in `testdata/solidity/libsolidity/codegen` for
   legacy, `@src` for via-IR. Cite the solc file and line in the code comment
   and the PR body.
2. **LLVM, rustc, and clang** decide how locations survive a rewrite. Follow
   LLVM's [HowToUpdateDebugInfo](https://llvm.org/docs/HowToUpdateDebugInfo.html):
   code that replaces one instruction, and cloned code, keep their location;
   merge locations only when new code replaces several instructions and
   produces each of their outputs; drop a location that would mislead, such as on code
   hoisted into another block. Never move a deleted instruction's location onto
   surviving code; CSE keeps only the survivor's location.
3. **Our own rule**, only where neither applies, with the reason in the code
   comment and the PR body. Never add a rule only to lower an audit count.

## Testing

- Prefer UI tests (`tests/ui/`) over unit tests for end-to-end behavior,
  especially diagnostics, sema, and compiler output. The annotation syntax is
  in [CONTRIBUTING.md](CONTRIBUTING.md#integration-tests).
- Put each test in the directory for the layer it checks, as the
  [tester guide](tools/tester/README.md#test-layout) lists, and keep leaf
  directories under about 80 tests.
- For codegen tests, use `//@ codegen-matrix: standard` (`mir`, `gas`, `size`
  revisions) unless it cannot express the test.
- To test one source under different flags, passes, levels, EVM versions, or
  outputs, use one test with `//@ revisions:` and revision-scoped directives
  (`//@[rev] compile-flags: ...`).
  Use separate files only when the source text itself differs in purpose.
- Use `//@ filecheck:` instead of a full snapshot when the snapshot is huge or
  brittle, or the test checks order, presence, or absence; see the
  [tester guide](tools/tester/README.md#filecheck).
- Use `//@ run-call:` and `//@ run-call-fail:` for one isolated call, and
  `tests/foundry/` (`cargo tq foundry`) for state, several transactions or
  actors, events, or cheatcodes; see the
  [tester guide](tools/tester/README.md#runtime-directives).
- Put imported or secondary sources in `auxiliary/` next to the test, never
  `aux/` (Windows rejects it).
- Assert formatted output in Rust tests with `snapbox` snapshots, not
  `.contains(...)`. Snapshot bytecode as disassembly, never raw bytes or
  offsets.
- Python: run `bash scripts/check-python.sh` and use uv, never pip; see
  [CONTRIBUTING.md](CONTRIBUTING.md#python-tooling).

### Codegen Passes

Before adding a pass, look for one to extend in `mir/transform/` and
`backend/evm/ir/passes/`. Start every pass module with docs that let a reviewer
understand it without the code: what it rewrites, the algorithm, its main
safety and profitability limits, its place in the pipeline, and deliberate
omissions. Keep the pass adapter small and beside the transform; the pass
manager only coordinates names, pipelines, and `dyn ModulePass` execution.

Test passes with UI tests, checked with snapshots or FileCheck, plus runtime or
differential tests when execution can change. Never write Rust unit tests that
run whole passes; unit-test only small pure helpers.

### Solc Parity

For requested comparisons with solc, use
`uv run --project tools/compiler-diff compiler-diff` from the repository root
and read its [guide](tools/compiler-diff/README.md) first. Never hide errors or
unsupported checks with expectations. Report coverage counts, record accepted
differences in [SOLC_DIVERGENCE.md](docs/SOLC_DIVERGENCE.md), and reduce each
new failure from it or from `cargo tq foundry-external` into a regression test.

Do not chase parity on differences ordinary Solidity would not hit, such as
revert data alone, assembly-forged values, dirty bits left by earlier storage
writes, or absurd type sizes. Record them there too, with a UI test that pins
our behavior. Malformed calldata may differ from solc only in revert data: never
accept an input that solc rejects. Reject a parity fix that makes the compiler
more complex or slower, or the generated code bigger or slower, unless ordinary
Solidity hits the difference.

When porting a solc test, split `==== Source: ... ====` sections into
`auxiliary/` and fix imports. If the port keeps the test's semantics one to one
(renames are fine), add `// ported-from: test/libsolidity/.../name.sol`, one
line per upstream file and no trailing punctuation, after the leading `//@`
directives or at the top. To update the tracked solc version, follow
[CONTRIBUTING.md](CONTRIBUTING.md#updating-solc).

## Diagnostics Style

- Keep the main message short. End no message, note, or help with a full
  stop, and quote code with backticks.
- Reuse solc's code for every diagnostic solc also emits, so `--allow` matches.
  Give a diagnostic solc lacks a code only when users need one, such as to
  `--allow` a warning, and pick a number below 1000; solc uses 1000 to 9999.
- Return `Result<(), ErrorGuaranteed>` rather than `bool` from emitting code
  where practical, and pass the guarantee to `mk_ty_err`. Never use
  `ErrorGuaranteed::new_unchecked()` when a real guarantee exists.
- Add context with `note` (why), `help` (how to fix), and `span_note` (related
  code).

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
- Do not call the project "Solar" in the third person; say "we" or "the
  compiler". `docs/SOLC_DIVERGENCE.md` may contrast `solar` with `solc`.

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
- Use `IndexVec<I, T>` for every collection indexed by `I`; repeated
  `x.index()` signals the wrong collection. Audit `IndexVec`s for sentinel
  entries, and switch to `FxHashMap<I, T>` only when measured occupancy shows
  sentinels dominate.
- Never use `Vec<bool>`: use the bitsets in `crates/data-structures`, or a hash
  set for sparse domains. Iterate with their iterators, never by probing
  `0..domain_size`.
- Compare symbols with `sym::name`/`kw::Keyword`, not `.as_str()`. Never call
  `Symbol::intern` on a literal; add it to `symbols!` in
  `crates/interface/src/symbol.rs`.
- Allocate AST nodes in the arena; write in place where possible, else use the
  helpers in `crates/data-structures/src/bump_ext.rs`.
- In visitors, use `type BreakValue = Never` when the visitor never breaks, and
  call `walk_*` from every overridden `visit_*` to continue traversal.

### IR construction and rewrites

[docs/MIR.md](docs/MIR.md#value-types-and-conversions) defines MIR types and
casts. The rules most code touches:

- Every `iN` value has zero bits above N, and every `i1` is 0 or 1, so passes
  need not clean them. Normalize raw words with `ne v, 0`; branches and selects
  take only `i1`.
- Addresses are `i160`. Keep raw Solidity bool and address bits that assembly
  can see as `i256`, and convert to `i1` for logic and branches.
- Cast explicitly with LLVM casts (`trunc`, `zext`, `sext`, `ptrtoint`,
  `inttoptr`); never retag a value or rely on equal storage width. `trunc` to
  `i1` keeps the low bit and does not test for nonzero.
- Zero tests are `eq v, 0` and `ne v, 0`; `ISZERO` belongs to EVM IR. Write a
  test that a word fits in `k` bits as `lt x, 2^k` and one that it does not as
  `gt x, 2^k - 1`, for 1 <= k < 256, never as a shift against zero;
  `spell-width-tests` picks the final spelling. See
  [docs/CODEGEN.md](docs/CODEGEN.md#width-tests).
- `ptr<memory>` is opaque: reach object words with `ptradd` and loads and
  stores, never integer arithmetic. Memory pointers are never null: a default
  memory value is the zero slot or a default object, never 0, and code reads
  pointers without zero guards.
- Put an IR or pseudo-IR comment, in emitted order and one instruction per line
  where that helps, directly above each piece of code that writes, moves, or
  rearranges IR: the match arm, branch, loop, or builder sequence, not the top
  of a large function. No `IR:` label, prose, or full stop.

## Codegen Benchmarking

Rank changes by correctness and `-Ogas` runtime gas, then `-Ogas` and `-Osize`
bytecode size, then compile time and memory; reject changes that only speed up
compilation. Follow the
[local build workflow](benches/runtime/README.md#comparing-local-builds). For
parser benchmarks, see [benches/README.md](benches/README.md).
