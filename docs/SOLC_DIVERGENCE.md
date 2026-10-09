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

Use the [compiler-diff project](../tools/compiler-diff/README.md) to capture
ABI/JSON or execution differences with inputs, compiler identities and replay
artifacts. A mismatch needs investigation before it becomes an intentional
divergence. Link reviewed expectation rules to the relevant entry or issue;
the comparison tool does not load this document as an allowlist.

## Entry Format

Use the next ID in the relevant phase.

| Field | Meaning |
| --- | --- |
| ID | Stable identifier, prefixed by frontend phase. |
| Status | `intentional`, `parity debt`, or `under review`. |
| Difference | What users observe differently from `solc`. |
| Rationale | Why the behavior exists or is accepted. |
| Coverage | Tests, fixtures, or issues that keep the behavior visible. |

## Standard JSON

### JSON-001: Unknown input fields

Status: intentional.

Difference: `solar` ignores unknown fields in standard-JSON input objects,
including unknown debug and metadata settings that solc rejects. It also
ignores `settings.optimizer.details`; only `enabled` and `runs` configure
the optimizer. Supported fields still require their expected JSON types.

Rationale: Solar accepts inputs containing options it does not implement.
Solc-specific pass controls have no equivalent meaning in Solar's pipeline.

Coverage: `tests/ui/standard-json/metadata/options/test.jsonc` and
`tests/ui/standard-json/debug/unknown-key/test.jsonc`.

## Import Resolution

### IMPORT-001: Files on disk are one source unit each

Status: intentional.

Difference: `solc` treats source unit names as opaque strings, so `src/B.sol`,
`src//B.sol`, `src/./B.sol`, `./src/B.sol` from a remapping target, and an
absolute path to the same file are separate source units. `solar` names a file
that it loads from disk by its normalized path, shown relative to the base
path, so all of these are one source unit named `src/B.sol`. In Standard JSON,
`solar` names a file read through the callback by its normalized source unit
name, keeping the `//` of a URL scheme, and asks the callback for the
normalized path in each root, so `lib/X.sol`, `lib//X.sol` and
`lib/sub/../X.sol` are one source unit `lib/X.sol`, read once when the read
succeeds. `solc` asks for each name as written. A file that
imports both `src/B.sol` and `src//B.sol` fails with `Identifier already
declared` in `solc` and compiles in `solar`. `solar` also looks up absolute
import paths and remapping targets as they are, where `solc` prepends a
non-empty base path to them. Since names come from paths, two different files
can get the same name; `solar` reports this for any source, where `solc` only
checks the files given on the command line.

Normalizing names has other effects. Remapping contexts match the normalized
name, so a file reached through the target `./lib/` has the context `lib/...`
in `solar` and `./lib/...` in `solc`. `solar` applies `..` segments before
following symbolic links, so `link/../x.sol` names `x.sol` next to `link`, not
next to its target.

Rationale: duplicate copies of one file only produce spurious conflicts. Build
tools such as Foundry preload sources under relative names and pass absolute
remapping targets, which must resolve to the preloaded sources
([#1628](https://github.com/paradigmxyz/solar/pull/1628)).

Coverage: `callback_imports_merge_spellings` in
`crates/solar/tests/it/standard_json.rs`,
`absolute_remapping_reuses_preloaded_source_unit_name` and
`direct_import_reuses_preloaded_source_unit_name` in
`crates/interface/src/source_map/file_resolver.rs`.

### IMPORT-002: Names of files in nested include paths

Status: intentional.

Difference: `solar` names every file from disk the way `solc` names files given
on the command line: relative to the base path, else to the first include path
that contains it. `solc` names an imported file by the import's source unit
name instead. The two differ when a file lies in more than one root, such as an
include path inside the base path or inside an earlier include path. With
`--base-path . --include-path node_modules`, `solc` names an import of
`@oz/A.sol` `@oz/A.sol`, and `solar` names it `node_modules/@oz/A.sol`. Since
remapping contexts match the importing file's name, a context such as
`@oz/:x/=y/` applies in `solc` and not in `solar`, and `node_modules/@oz/:x/=y/`
the other way around.

For a file named relative to an include path outside the base path, `solar`
resolves relative imports against the file's absolute path. Remappings for
source unit names then do not apply to them, they do not search the other
roots, and `../` can leave the include path.
Remapping contexts match both the file's name and its absolute path. Like
`solc`, an import resolves to an input file with its source unit name before
searching the disk, but files loaded for other imports don't count, so where
`solc` would reuse such a file, `solar` can report an ambiguous import or a
duplicate name instead.

Rationale: a name that depends only on the file's path is the same for every
import of the file. It also keeps remapping contexts such as `lib/dep/` matching
files in Foundry's `lib` include path, as `solc` does for files that remappings
route through the base path. Foundry names sources outside the project by their
absolute paths and writes remapping contexts for them.

Coverage: `nested_include_paths_name_by_base_path`,
`include_paths_name_source_units`, `relative_imports_leave_include_paths` and
`remapping_contexts_match_absolute_paths_outside_base_path` in
`crates/interface/src/source_map/file_resolver.rs`, and
`crates/solar/tests/it/paths.rs`.

### IMPORT-003: Lenient path options

Status: intentional.

Difference: `solar` accepts `--include-path` without `--base-path`, using the
current directory as the base path, and does not report an ambiguous import
when several roots contain the same file, as with a repeated include path. In
Standard JSON mode, which does not access the file system, it does not check the
base path. It does not restrict imports to allowed paths, so `--allow-paths` has
no effect.

Rationale: these inputs have a single sensible meaning, and Standard JSON reads
imports only through the read callback.

Coverage: `include_paths_without_base_path` in `crates/solar/tests/it/paths.rs`.

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

### TYPECK-006: Oversized fixed-array copies

Status: intentional.

Difference: Solar rejects copying or ABI-encoding fixed arrays with more than
`2^64 - 1` elements during type checking, including arrays nested in structs.
It still accepts their storage declarations, indexed accesses, and storage
reference bindings. Solc 0.8.37 compiles the storage-to-storage array, tuple,
and struct copies covered by the fixture, but those copies panic with code
`0x41` at runtime. ABI-encoding the same array causes an internal compiler
error in solc 0.8.37. These results hold with both code generators and with
optimization enabled or disabled.

Rationale: report unsupported copies at their source during type checking,
rather than fail during lowering or emit a runtime panic for a known oversized
copy. The restriction applies to copying the values, not to addressing their
storage.

Coverage: `tests/ui/typeck/storage_oversized_copy.sol` and
`tests/ui/codegen/lowering/run-call/full_width_storage_layout.sol`.

### TYPECK-007: Uninitialized storage pointers are reported once per location

Status: intentional.

Difference: `solc` analyzes each function once per contract that inherits it
and reports error 3464 for every analysis, so a base function with one
uninitialized access gets one error per derived contract. `solar` runs the same
analyses but reports each location once.

Rationale: the copies point at the same code and give no extra information.

Coverage: `tests/ui/typeck/control_flow/uninitialized_storage_pointer.sol`.

### TYPECK-008: Unreachable code across sources

Status: intentional.

Difference: when unreachable code spans a function and a modifier declared in
another source, `solc` merges the ranges into one with mixed sources and
reports only the part in the function's source. `solar` reports the part in
each source.

Rationale: the merged `solc` range is invalid and hides unreachable code.

Coverage: `tests/ui/typeck/control_flow/unreachable/cross_source.sol`.

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

### CODEGEN-008: Environment snapshots across Foundry cheatcodes

- ID: CODEGEN-008
- Status: intentional
- Difference: Foundry tests that save `block.number` across `vm.roll`, or
  `block.timestamp` across `vm.warp`, can observe different values with solar
  and solc. A source local is not a reliable snapshot: optimization can reuse
  or rematerialize an environment read. Optimized solc via IR exhibits the
  same class of behavior; passing with one pipeline is not a guarantee.
- Rationale: block number and timestamp are invariant within an ordinary EVM
  transaction. Cheatcodes change that environment outside production semantics.
  Keep production optimizations and use `vm.getBlockNumber()` or
  `vm.getBlockTimestamp()` at the intended snapshot or observation point.
- Coverage: [Foundry PR #16727](https://github.com/foundry-rs/foundry/pull/16727)
  implements source lints for both patterns, following local values, internal
  helpers, and modifiers. Its tests cover getter behavior and bytecode
  neutrality. In the pinned OpenZeppelin external suite at
  `f646874fdc9b151631e3c96a68defbdbe736cd53`, the helper for
  `BlockhashTest::testFuzzHistoryBlocks(uint16,uint256,bytes32)`
  saves `block.number - 1` before rolling. Replacing that capture with
  `vm.getBlockNumber() - 1` passes the reproduced case and 256 fixed-seed fuzz
  cases under both compilers. The external runner applies this test-only
  correction to both compiler legs and keeps the test enabled. It checks the
  expected source text before applying the correction.

### CODEGEN-009: Static frames sit below the initial free memory pointer

- ID: CODEGEN-009
- Status: intentional
- Difference: `solc` starts the free memory pointer at `0x80`. `solar` keeps
  internal-call frames and spill slots in static memory from `0x80` up to the
  initial free memory pointer, so a contract's heap starts above every frame it
  can reach. Inline assembly that stores data at constant addresses in that
  range, instead of allocating through the free memory pointer, can have it
  overwritten by any internal call, including the helpers the compiler
  generates for ABI encoding and pre-Cancun memory copies.
- Rationale: The Solidity documentation counts only scratch space, memory
  allocated through the free memory pointer, and memory past the free memory
  pointer within one assembly block as memory-safe. Static frames make internal
  calls cheaper than a memory stack, and memory-safe assembly never reaches
  them.
- Coverage: `tests/ui/codegen/lowering/run-call/pre_cancun_memory_copies.sol`
  encodes an object that assembly allocates through the shared copy helper.

### CODEGEN-010: Assembly cannot hand compiler-owned memory to the heap

- ID: CODEGEN-010
- Status: intentional
- Difference: When inline assembly stores a value computed from constants
  and calldata into the free memory pointer slot, every later read of the
  slot as the pointer sees at least the initial free memory pointer: an
  allocation, or an `mload` whose word addresses memory. After
  `mstore(0x40, 0x80)`, such a read sees `0x80` under `solc` and the initial
  pointer here. A word that is also read as data, such as one hashed in
  scratch memory, or loaded and then compared, hashed, encoded, stored, or
  used as a key, stays in the slot for those reads, and only the pointer
  reads see the raised value. Each external function
  that runs code writing memory at an absolute address computed from calldata,
  as Seaport lays out a basic order's hashes and event data, keeps its spill
  slots and the frames it reaches above `0x2080`, and above the constant
  ranges assembly names there in the functions it runs or that an external
  function sharing those frames runs. This costs memory expansion gas; other
  external functions keep their memory low unless they share those frames, or
  a recursive helper places every frame above all external functions.
- Rationale: The spill slots and internal-call frames below the initial free
  memory pointer (CODEGEN-009) hold values that `solc` keeps on the stack, so
  the next allocation after a lowered pointer, or the absolute layout itself,
  would replace them. Raising only the pointer reads keeps the stored value for
  code that uses the slot as scratch, such as an error argument before a revert
  or a hash input before the pointer is restored. A pointer derived from the
  heap already lies above the initial one, and an absolute pointer that reaches
  the store through a parameter, memory, or a call keeps its value. The
  `heap-floor` pass documents how it tells pointer reads from data reads. A
  layout that grows past `0x2080`, or one indexed by a loop counter alone, can
  still reach the compiler's memory; Seaport's basic orders with about 40 or
  more additional recipients do.
- Coverage: `tests/ui/codegen/lowering/run-call/assembly_low_memory_layouts.sol`,
  `tests/ui/codegen/lowering/run-call/assembly_low_memory_routes.sol`,
  `tests/ui/codegen/lowering/run-call/assembly_low_memory_recursive_routes.sol`,
  `tests/ui/codegen/mir/heap-floor/heap_floor.mir`, and Seaport's own suite in
  `cargo tq foundry-external seaport`.
