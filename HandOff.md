# Signature Help Handoff

## Status

The end-to-end Solidity LSP signature-help implementation is complete and verified. There is no
known remaining implementation work in scope.

The resumed Codex thread is:

```text
019f53f5-1b9d-7310-aadc-3ebea533e525
```

The worktree is intentionally uncommitted.

## Implemented

- Registered `textDocument/signatureHelp` and advertised `(` and `,` trigger characters.
- Negotiated parameter label offsets, preferred documentation format, and per-signature
  `activeParameter` support from client capabilities.
- Added UTF-16 position conversion with line and column clamping and surrogate-pair validation.
- Added a semantic `SignatureHelpIndex` that owns rendered signatures after compiler analysis data
  is released.
- Covered functions and overloads, constructors, modifiers, events, errors, structs,
  function-typed variables and fields, attached library methods, builtins, dynamic arrays, NatSpec,
  indexed event parameters, and named returns.
- Added nesting-aware request-time call parsing for incomplete source, named arguments, comments,
  grouped expressions, function call options, constructor create options, and parenthesized
  creation expressions.
- Added distinct regular, constructor, event, and error call forms so contract casts and builtins do
  not collide with Solidity-specific invocation syntax.
- Fingerprinted complete callee token sequences and ranges. Failed analysis can retain useful old
  callsites without reusing them after callee renames or receiver changes.
- Added URI provenance and per-callsite failed-file merging so successful analysis removes stale
  catalog entries while failing files retain usable signatures.
- Classified qualified event/error calls from the semantic callee type and rendered callable
  non-function members from their member type. This covers contract types, imported modules,
  contract members through modules, and nested import namespaces.
- Kept member-call fallback conservative: when an exact semantic callsite cannot be verified,
  signature help returns `None` instead of returning a signature for the wrong receiver.

## Focused Regressions

The signature-help integration suite includes regressions for:

- Active parameter selection, overload ordering, named arguments, and comment-contained colons.
- Client label-offset, documentation-format, and per-signature active-parameter capabilities.
- Unclosed calls after failed analysis and stale callee/receiver edits.
- Declaration suppression and contract-cast/constructor separation.
- Function and constructor call options, nested commas, grouped arguments, and UTF-16 positions.
- Qualified events/errors through `I.Event`, `B.Event`, `B.Contract.Event`, and `B.A.Event`, with
  matching error paths.
- Exact qualified callsite selection using same-named declarations in two import namespaces with
  different parameter types. Qualified members cannot use the global name fallback, so this test
  proves the semantic callsite path supplies the single correct signature.

## Verification

Final checks completed successfully:

```text
cargo fmt --all
cargo nextest run -p solar-lsp
  131 tests run: 131 passed, 0 skipped
cargo clippy -p solar-lsp --all-targets -- -D warnings
cargo clippy --workspace --all-targets
git diff --check
```

The two new untracked Rust files also pass `git diff --no-index --check /dev/null <file>` with no
whitespace diagnostics. Conflict-marker and final-newline checks are clean.

An independent final correctness review found no actionable issues. The exact qualified
event/error regression was specifically reviewed and confirmed to reject lexical/global fallback.

The full workspace nextest suite was attempted earlier in this work and reached the UI aggregate
tests; 13 aggregates failed only because `FileCheck` is not installed in this environment. The
requested complete `solar-lsp` suite passes as shown above.

## Residual Behavior

During failed analysis, a retained member call whose source position shifts is intentionally not
matched by name alone. Returning no signature help in that case is safer than displaying a stale
signature from a different receiver. Qualified overload behavior otherwise follows the existing
semantic member-resolution results.

## Files

```text
Cargo.lock
crates/lsp/Cargo.toml
crates/lsp/src/config.rs
crates/lsp/src/global_state.rs
crates/lsp/src/global_state/tests/signature_help.rs
crates/lsp/src/global_state/tests/support.rs
crates/lsp/src/handlers/reqs.rs
crates/lsp/src/lib.rs
crates/lsp/src/proto.rs
crates/lsp/src/signature_help.rs
crates/lsp/src/symbols.rs
HandOff.md
```
