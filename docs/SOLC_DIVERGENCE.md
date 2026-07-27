# solc Divergence

This file tracks intentional, user-visible differences from `solc`. The baseline is
the `solc` version checked into `testdata/solidity`, unless an entry names a
different upstream version.

The goal is not to list every missing feature. A divergence belongs here when
`solar` deliberately accepts, rejects, warns, or reports source locations
differently from `solc`. Each entry should state the phase, the behavior
difference, why we keep it, and the tests or issue that cover it.

See [#547](https://github.com/paradigmxyz/solar/issues/547) for the tracking issue
for documenting divergences.

## Entry Format

Use the next ID in the relevant phase.

| Field | Meaning |
| --- | --- |
| ID | Stable identifier, prefixed by frontend phase. |
| Status | `intentional`, `parity debt`, or `under review`. |
| Difference | What users observe differently from `solc`. |
| Rationale | Why the behavior exists or is accepted. |
| Coverage | Tests, fixtures, or issues that keep the behavior visible. |

## Parsing

### PARSE-001: Validation stage differences

Status: intentional.

Difference: `solar` and `solc` do not always perform equivalent validation in
the same compiler stage. As a result, `--stop-after=parsing` may accept input
that `solc` rejects during parsing even though a normal `solar` compilation
rejects it in a later frontend stage. For example, `solar` parses an `unchecked`
block used directly as an `if`, loop, or `else` body and rejects it during AST
validation, while `solc` rejects it during parsing.

Rationale: the frontend structures parsing and validation differently from
`solc`; checks live in the earliest `solar` stage with the context and
responsibility needed to enforce them rather than mirroring solc's internal
phase boundaries.

Coverage: `tests/ui/typeck/unchecked_as_single_statement.sol`; the upstream
`unchecked_while_body` parse-only fixture remains excluded from solc parity
testing.

## AST Validation

No intentional divergences documented yet.

## Name Resolution

No intentional divergences documented yet.

## Type Checking

### TYPECK-001: Called Yul functions in view/pure checking

Status: intentional.

Difference: `solc` checks inline-assembly Yul function bodies at their definition
site during view/pure checking, including bodies that are never called. `solar`
only propagates Yul function effects through Yul call expressions.
Uncalled Yul function bodies do not affect view/pure diagnostics or mutability
restriction suggestions.

Rationale: a used Yul helper should behave like a function call for this lint:
the call expression is the operation that can affect the enclosing Solidity
function's mutability. Uncalled Yul helpers are dead code for this analysis, so
reporting their bodies as if they affect the enclosing function is intentionally
not preserved.

Coverage: `tests/ui/typeck/view_pure_checker/yul_functions.sol` and
`tests/ui/typeck/view_pure_checker/yul_parity.sol`.

## Contract-Level Checks

No intentional divergences documented yet.

## Code Generation

No intentional divergences documented yet.
