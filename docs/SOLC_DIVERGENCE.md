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

### PARSE-002: Versioned Yul builtin names remain reserved

Status: intentional.

Difference: `solar` reserves target-dependent Yul builtin names independently
of the selected EVM version. This includes `basefee`, `prevrandao`, `mcopy`,
`blobhash`, `blobbasefee`, `tload`, `tstore`, and `clz`. `solc` allows these
names to be declared as identifiers on targets where the corresponding builtin
is unavailable.

Rationale: `solar` keeps the Yul grammar independent of the target EVM version.
Builtin calls are parsed uniformly and their availability is validated during
name resolution, where the selected EVM version is available.

Coverage: `tests/ui/parser/yul/cancun_builtin_identifiers.sol` and the
`tests/ui/assembly/yul_builtins_*_evm_version.sol` fixtures.

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

### TYPECK-003: Inline array literals adopt the expected element type

Status: intentional.

Difference: `solc` types an inline array literal from its elements alone and
then requires the result to convert to the destination, which rules out any
element widening: `uint256[2] memory x = [1, 2];` is an error, and so are
`int256[2] memory y = [1, 2];`, `bytes[2] memory z = ["a", "b"];` and the
nested `uint256[2][2] memory w = [[1, 2], [3, 4]];`. `solar` seeds the
literal's element type with the element type of the destination, so it accepts
all of them and stores the widened values. A copy into storage, such as
`s = [[1, 2], [3, 4]];` or `a.push([1, 2])`, is accepted by both, because a
storage copy converts element-wise.

Rationale: we deliberately support this extended form. The seed gives a
literal the element type of its destination, which is how a nested literal
copied into storage picks up the destination's element type, and the same
rule makes `uint256[2] memory x = [1, 2];` mean what it reads. The widened
values are correct; only the acceptance is wider than `solc`'s, and every
program `solc` accepts here has the same meaning in `solar`.

Coverage: `tests/ui/typeck/inline_array_reference_elements.sol`,
`tests/ui/typeck/array_push_element_locations.sol`, and
`tests/ui/codegen/lowering/run-call/nested_array_storage_memory.sol`.

### TYPECK-004: Named arguments in base constructor and modifier invocations

Status: intentional.

Difference: `solc` parses the argument list of an inheritance specifier, of a
base constructor call in a constructor header, and of a modifier invocation as
a plain expression list, so `contract D is Base({b: 1, a: 2})` and
`function f() m({b: 3, a: 4})` are parse errors there (ParserError 6933,
"Expected primary expression"). `solar` accepts the named form in all three
positions and binds the arguments by parameter name. In each of them the list
gets the same checks as a named function call's: argument types, arity,
duplicate names, and names that no parameter has.

Rationale: the restriction is a shortcoming of `solc`'s grammar rather than a
language rule; these lists denote calls to a constructor or a modifier, and the
named form has one unambiguous meaning. We deliberately support this extended
form. Every program `solc` accepts here has the same meaning in `solar`.

Coverage: `tests/ui/typeck/base_arguments.sol`,
`tests/ui/typeck/modifier_arguments.sol`,
`tests/ui/codegen/lowering/base_constructor_args.sol`,
`tests/ui/codegen/lowering/run-call/named_arguments_extended.sol`, and
`tests/ui/codegen/lowering/run-call/modifier_named_arguments_override.sol`.

### TYPECK-005: Parenthesized `try` targets

Status: intentional.

Difference: `solc` requires a `try` statement's target to be a call
syntactically and reports 5347 ("Try can only be used with external function
calls and contract creation calls") for `try (c.f()) { ... }`, because the
parenthesized expression is a tuple rather than a call. `solar` peels the
parentheses and compiles the statement as if they were not written.

Rationale: parentheses do not change the call they wrap, so the statement has
one unambiguous meaning; rejecting it would be a grammar restriction rather
than a language rule. The checker and lowering peel them identically, so an
accepted statement always compiles.

Coverage: `tests/ui/codegen/lowering/run-call/try_parenthesized_target.sol`.

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
- Status: intentional
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

### CODEGEN-003: Integer literal expressions lose arbitrary precision during lowering

- ID: CODEGEN-003
- Status: intentional
- Difference: `solc` keeps a number-literal expression at arbitrary precision
  until conversion to a non-literal type. `solar`'s type checker computes the
  same literal-only expression with `BigInt` and retains an `IntLiteral` type,
  but function lowering ignores that computed value. It recursively emits
  `U256` EVM operations for its leaves and operators. An intermediate that
  exceeds an EVM word can therefore wrap or, when given a checked integer type
  by lowering, revert with `Panic(0x11)` before a later literal operation
  reduces it. `(2**255 + 2**255) % 7` is one reproducer: solc returns `2`;
  solar reverts. The divergence also covers literal-only expressions with
  oversized intermediates followed by division, comparison, subtraction,
  shifts, or another operation that makes the final result representable.
- Rationale: this codegen path intentionally lowers function-body operations
  as EVM-width operations, even when type checking has evaluated an all-literal
  tree. We do not materialize the type checker's literal result here.
- Coverage: `symbolic-audit/literal_addmod_fold.sol`; upstream source
  `testdata/solidity/test/libsolidity/semanticTests/arithmetics/addmod_mulmod.sol`.

### CODEGEN-004: Public array getters return a panic instead of an empty revert

- ID: CODEGEN-004
- Status: intentional
- Difference: on an out-of-bounds index, generated public getters for arrays
  and mappings of arrays use `Panic(0x32)`. `solc`'s generated getters use
  `revert(0, 0)` instead. Ordinary source-level array indexing still uses
  `Panic(0x32)` in both compilers.
- Rationale: getter lowering intentionally reuses ordinary array-index
  lowering. An out-of-bounds getter therefore keeps the normal `Panic(0x32)`
  behavior instead of matching solc's empty revert data.
- Coverage: `symbolic-audit/getter_out_of_bounds.sol`; the symbolic audit
  reproduced the behavior in 13 functions across 11 upstream semantic tests.
  Solc tracks the getter's empty revert data in
  [issue #16660](https://github.com/argotorg/solidity/issues/16660).

### CODEGEN-005: Narrow storage array indexes are cleaned

- ID: CODEGEN-005
- Status: intentional
- Difference: When assembly assigns dirty upper bits to a narrow integer used
  as a storage-array index, `solar` cleans the value before the bounds check.
  `solc` uses the raw word, so `uint8(0x101)` indexes out of bounds instead of
  selecting index `1`. Memory-array indexes are cleaned by both compilers.
- Rationale: typed storage indexing converts its index to the array's word
  index type before checking bounds. This preserves the normal implicit
  conversion rule for narrow values.
- Coverage: `tests/ui/codegen/lowering/run-call/dirty_storage_array_index.sol`.

### CODEGEN-006: Legacy source-map modifier depth

- ID: CODEGEN-006
- Status: implemented
- Behavior: Legacy `sourceMap` output carries the compiler's modifier nesting
  depth in the `m` field, preserving it through MIR and EVM IR lowering and
  optimization. Shared code with different modifier depths has no unique
  modifier frame and uses depth zero. Shared code with multiple source origins
  is unmapped in legacy output; ETHDebug retains bounded source alternatives.
  This policy applies equally to MIR and EVM IR sharing.
- Coverage: `tests/ui/standard-json/source-maps/modifier.jsonc`.

### CODEGEN-007: `revertStrings: debug` message parity is best effort

- ID: CODEGEN-007
- Status: intentional
- Difference: With `--revert-strings debug` (Standard JSON
  `settings.debug.revertStrings: "debug"`), compiler-generated reverts carry
  solc's `Error(string)` messages, and the common checks report the same
  message under the same condition as solc. Exact parity is not a goal:
  the compiler fuses and orders its ABI decoding checks differently from
  solc, so malformed input that fails several checks at once, or that is
  validated lazily on access rather than eagerly, can report a different
  message than solc. One message is never produced: "ABI encoding: array
  data too long", because the encoder has no `2**64` length check when
  re-encoding calldata arrays. `debug` never changes whether an input is
  accepted. `strip` matches `solc`: a dropped reason is still evaluated for
  its effects and failures, and only the payload, including the copy of a
  storage string that would validate its encoding, is dropped.
  `verboseDebug` is rejected as unimplemented by both compilers.
- Rationale: the messages are debugging aids. Matching every solc message
  in every edge case would require restructuring the decoder around solc's
  check order, which is not worth worse source or generated code.
- Coverage: `tests/ui/standard-json/debug/`,
  `tests/ui/codegen/lowering/revert-strings/`,
  `tests/ui/codegen/lowering/library_delegatecall_guard.sol`.
