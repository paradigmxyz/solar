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

### PARSE-002: Cancun Yul builtin names are reserved across EVM versions

Status: intentional.

Difference: `solar` reserves the Cancun-only Yul builtin names `mcopy`,
`blobhash`, `blobbasefee`, `tload`, and `tstore` independently of the selected
EVM version. `solc` allows these names to be declared as identifiers when
targeting an older EVM.

Rationale: `solar` keeps the Yul grammar independent of the target EVM version.
Builtin calls are parsed uniformly and their availability is validated during
name resolution, where the selected EVM version is available.

Coverage: `tests/ui/parser/yul/cancun_builtin_identifiers.sol`.

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

### TYPECK-002: Standalone call-option function values

Status: intentional.

Difference: `solc` permits call options such as `{gas: ...}` and `{value: ...}`
to form a function value, including when accessing its `.address` or `.selector`
member. `solar` requires call options to be part of a call expression.

Rationale: `solar` models call options on HIR call expressions and intentionally
does not represent an option-bearing function value as a separate HIR node.

Coverage: `tests/ui/typeck/function_calls/call_options_standalone.sol` and
[#1269](https://github.com/paradigmxyz/solar/pull/1269#discussion_r3846737698).

## Contract-Level Checks

No intentional divergences documented yet.

## Code Generation

### CODEGEN-001: Dirty bits do not survive assembly-assigned variables read from Solidity

- ID: CODEGEN-001
- Status: intentional
- Difference: When inline assembly assigns a variable whose type spans fewer
  than 256 bits, `solc` leaves the raw word in the variable and cleans it at
  each use site that needs a canonical value (comparisons, checked arithmetic,
  ABI encoding). `solar` instead canonicalizes once, at every Solidity-level
  read of such a variable; reads inside assembly see the raw word in both
  compilers. Code that deliberately round-trips dirty upper bits through a
  typed variable or an internal-function return back into assembly — solady's
  `Brutalizer` test helpers assert exactly that — observes cleaned values
  under `solar`.
- Rationale: The Solidity documentation makes bits outside a type's width
  unspecified after assembly assignments, so both models are conforming. A
  single cleanup point at the assembly-to-Solidity boundary covers every
  downstream consumer (comparisons, arithmetic, mapping keys, encodes) without
  per-use-site masks, and keeps the in-assembly raw-scratch idiom
  (`value := shl(96, value)` then reading `value` back) working exactly like
  `solc`.
- Coverage: `tests/ui/codegen/run-call/assembly_assign_cleanup.sol`;
  external-suite canary: solady `BrutalizerTest::testBrutalizedAddress` and
  `testBrutalizedBool` fail by asserting dirt survives.

### CODEGEN-002: Large ABI-heavy contracts can exceed EIP-170

- ID: CODEGEN-002
- Status: parity debt
- Difference: ABI-heavy contracts can have substantially larger deployed
  bytecode than their `solc` equivalents. Seaport currently has six helpers
  that fit below EIP-170's 24,576-byte limit with `solc` but exceed it with
  this compiler: `PausableZoneController` (26,974 bytes),
  `SuggestedActionHelper` (46,267 bytes), `ExecutionsHelper` (28,064 bytes),
  `MatchFulfillmentHelper` (39,280 bytes), `SeaportValidator` (52,420 bytes),
  and `SeaportNavigator` (27,642 bytes).
- Rationale: Progressive ABI lowering currently expands structurally similar
  aggregate decoders independently in each external wrapper. The EVM IR
  outliner shares repeated straight-line instruction runs but not equivalent
  decoder control-flow subgraphs, so these large wrappers retain duplicated
  validation and materialization code. The external artifact audit exempts
  only these named contracts while continuing to enforce artifact presence
  and EIP-170 parity for the rest of the corpus.
- Coverage: `cargo tq foundry-external seaport`; the exact exemptions live in
  `SEAPORT_CODE_SIZE_SKIPS` in `tools/tester/src/foundry/external.rs`.
