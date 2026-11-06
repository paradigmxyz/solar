# solc Divergence

This file tracks intentional, user-visible differences from `solc`. The baseline is
the `solc` version checked into `testdata/solidity`, unless an entry names a
different upstream version. Where `solc`'s legacy and via-IR pipelines behave
differently, we follow via-IR, and such cases are not listed.

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

### JSON-002: Ethdebug output does not depend on `debugInfo` or `experimental`

Status: intentional.

Difference: `solar` always emits full ethdebug output for the
`evm.bytecode.ethdebug`, `evm.deployedBytecode.ethdebug` and `ethdebug.*`
selections. `solc` 0.8.38 emits no semantic debug info for the program outputs
unless `settings.debug.debugInfo` contains `ethdebug`, and rejects ethdebug
outputs and `ethdebug` in `debugInfo` unless `settings.experimental` is set.
`solar` has no `settings.experimental` and ignores it. It rejects `ethdebug`
without `ast-id` in `debugInfo` like `solc`.

Rationale: `solar` builds ethdebug output from its own debug info, which does
not depend on a Yul IR selection, and an experimental flag would only block
tools that request the output.

Coverage: `tests/ui/standard-json/debug/ethdebug-output-without-debug-info`,
`tests/ui/standard-json/debug/ethdebug-with-ast-id` and
`tests/ui/standard-json/debug/ethdebug-without-ast-id`.

### JSON-003: Compiler identity in metadata and CBOR

Status: intentional.

Difference: contract metadata names the compiler as
`"compiler": {"name": "solar", "version": "0.8.38+commit.d9f23fbe.solar.0.3.0"}`,
where solc writes only `"version": "0.8.28+commit.7893614a"`. The version is
the solc-compatible form that `SOLC_WRAPPER=1 solar --version` prints: the
emulated solc version, then the Solar commit and version as build metadata.
The CBOR auxdata carries a `solar` entry instead of `solc`: three version
bytes for releases, and the long Solar version as text
(`0.3.0-dev+commit.d9f23fbe`) for other builds, as solc does for prereleases.
Combined JSON reports the same solc-compatible version.

Rationale: verifiers and other tools gate on solc versions, so the semver core
stays the language version, while `name`, the `.solar.` build tag and the CBOR
key tell them which compiler and build to fetch. A `solc` key would make
solc-only decoders report the wrong compiler.

Coverage: `tests/ui/standard-json/metadata/none/test.jsonc` and
`tests/ui/standard-json/metadata/options/test.jsonc`.

### JSON-004: CBOR auxdata positions

Status: intentional.

Difference: `evm.bytecode.cborAuxdata` and `evm.deployedBytecode.cborAuxdata`
report every CBOR auxdata trailer in the bytecode, including those of embedded
contracts, keyed by the fully qualified name of the contract each trailer
belongs to: `{"A.sol:Child": [{"offset": N, "value": "0x…"}, …], …}`. A
contract embedded more than once has one entry per copy. When contracts have
byte-identical code and trailers, as with `bytecodeHash: none`, they share one
copy, listed under each of them at the same offset. Entries are in offset
order within each array, and keys in the order of their first entry. `offset`
points at the CBOR map after the `INVALID` byte, and `value` holds the map and
its two-byte length. solc has no such output; tools recover the positions from
`evm.legacyAssembly` and a second compilation. The parent selections and `*`
include it, and `evm.legacyAssembly` stays empty.

Rationale: the entries have the shape of the Verifier Alliance `cborAuxdata`
artifact that Sourcify stores, so verifiers can read the positions directly and
flatten them into that artifact's numbered entries. The contract names let them
check each trailer against that contract's metadata hash. Code generation
records where it places each trailer and whose it is, so code or data that
repeats a trailer's bytes is never reported: verifiers replace the reported
bytes with the deployed ones, and a reported literal would let deployed code
differ from its source.

Coverage: `tests/ui/standard-json/cbor-auxdata/test.jsonc`,
`tests/ui/standard-json/cbor-auxdata-shared.jsonc` and
`tests/ui/standard-json/cbor-auxdata-literal.jsonc`.

### JSON-005: `settings.solar` takes unstable flags

Status: intentional.

Difference: `solar` reads `settings.solar.unstable`, a list of unstable flags
that change output, written like `-Z` flags without the prefix, such as
`["tune=interpreter"]`, and records it in each contract's metadata. It ignores
other flags in the list. `solc` rejects the unknown `settings.solar` key. Like
`solc`, `solar` ignores the command line's `--evm-version` in Standard JSON
mode, and it also ignores `--stop-after`, `--revert-strings` and the `-Z`
flags that change output.

Rationale: the Standard JSON input and the compiler version alone must
determine the output, so that verifiers such as Sourcify can rebuild a
contract from its metadata. Flags that only change diagnostics or debugging
output still come from the command line.

Coverage: `tests/ui/standard-json/codegen-error.jsonc`,
`tests/ui/standard-json/debug/cli-flag-ignored/test.jsonc` and
`tests/ui/standard-json/metadata/options/test.jsonc`.

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
next to its target. A `file://` import that `solar` loads from disk is named by
its path, so `file://src/B.sol` and `src/B.sol` are one source unit `src/B.sol`,
where `solc` keeps the prefix in the name. Files read through the Standard JSON
callback keep the prefix, as in `solc`.

Rationale: duplicate copies of one file only produce spurious conflicts. Build
tools such as Foundry preload sources under relative names and pass absolute
remapping targets, which must resolve to the preloaded sources
([#1628](https://github.com/paradigmxyz/solar/pull/1628)).

Coverage: `callback_imports_merge_spellings` and `callback_imports_file_urls` in
`crates/solar/tests/it/standard_json.rs`,
`absolute_remapping_reuses_preloaded_source_unit_name`,
`direct_import_reuses_preloaded_source_unit_name` and
`file_url_imports_load_from_disk` in
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
when several roots lead to the same path, as with a repeated include path.
Different paths to one file, such as an include path and a symbolic link to it,
are still ambiguous, as in `solc`. In Standard JSON mode, which does not access
the file system, it does not check the base path. It does not restrict imports
to allowed paths, so `--allow-paths` has no effect.

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

### PARSE-002: Directional override markers in doc comments

Status: intentional.

Difference: `solar` rejects unbalanced Unicode directional embeddings and
overrides (U+202A..U+202E) in NatSpec doc comments (`///` and `/** */`), as it
does in plain comments and unicode string literals. `solc` checks only plain
comments and unicode string literals.

Rationale: the check defends against "Trojan Source" attacks, where source text
renders in a different order from how it compiles; a doc comment can hide code
just as a plain comment can.

Coverage: `tests/ui/lexer/direction_override/markers.sol`;
[#1844](https://github.com/paradigmxyz/solar/issues/1844).

### PARSE-004: Fixed-point types

Status: intentional.

Difference: `solc` accepts `fixed`, `ufixed`, `fixedMxN` and `ufixedMxN` types
in declarations, conversions, operators, the ABI and storage layouts, and fails
only when code generation reaches a fixed-point operation. It also gives
fractional literals fixed-point mobile types, so it accepts comparisons of them,
such as `0.3 < 0.5`, conditionals with fractional branches and inline arrays of
fractions. `solar` rejects every fixed-point type during parsing, and fractional
literals have no mobile type, so it rejects those expressions. Valid fixed-point
type names stay keywords, as in `solc`.

Rationale: `solc` cannot generate code for fixed-point operations, so we do not
represent fixed-point types.

Coverage: `tests/ui/parser/fixed_point_type.sol`,
`tests/ui/parser/fixed_point_type_default.sol`,
`tests/ui/parser/fixed_mxn_types.sol`,
`tests/ui/parser/fixed_mxn_declaration_names.sol`,
`tests/ui/typeck/literals/rational_literal_operators.sol`, and
`tests/ui/typeck/literals/rational_literal_mobile_type.sol`; the upstream fixtures that use
fixed-point types are excluded from solc parity testing.

### PARSE-005: Yul literal forms

Status: intentional.

Difference: `solc` accepts only plain decimal and hexadecimal integers in Yul
and inline assembly, and rejects `1_000`, `0x1_0`, `1__0` and `1e5` with error
4828. `solar` accepts any number literal whose value is an integer, as in Solidity,
and rejects fractions such as `0.5` with error 4828.

`solc` also accepts only a plain string literal or a single `hex` string
literal in Yul and inline assembly. It rejects `unicode"é"` with error 1856
and adjacent literals such as `"a" "b"` and `hex"ab" hex"cd"` with error 6913.
`solar` accepts all three as in Solidity: `unicode` literals are UTF-8,
adjacent literals of the same kind are concatenated, and the result is the
left-aligned word, such as `0x6162…` for `"a" "b"`.

Rationale: these literals have one obvious value, so rejecting them only makes
the language harder to use.

Coverage: `tests/ui/codegen/run-call/assembly/assembly_number_literals.sol`,
`tests/ui/codegen/run-call/assembly/assembly_string_literals.sol`,
`tests/ui/parser/yul_number_underscores.sol` and
`tests/ui/parser/yul_fractional_literal.sol`.

### PARSE-006: Number literals are limited to 4096 bits

Status: intentional.

Difference: `solc` accepts decimal and hexadecimal number literals with any
number of digits, and limits only the results of literal arithmetic to 4096
bits, so a 2000-digit literal divided by itself is `1`. `solar` also rejects a
literal whose digits, before any exponent, are wider than 4096 bits. Leading
zeros of a fraction count toward the limit, since they widen its denominator;
those of an integer do not.

Rationale: literal arithmetic cannot produce a value wider than 4096 bits, so
a wider literal has no use, and the limit bounds the parser's work on one
token.

Coverage: `tests/ui/parser/number_literal_too_large.sol`.

## AST Validation

### AST-001: Version pragma mismatches are warnings

Status: intentional.

Difference: `solar` checks `pragma solidity` requirements against the `solc`
version it implements like `solc`, with the same error code 5333, but reports a
mismatch as a warning instead of a fatal parser error. `SOLC_WRAPPER_VERSION`
changes only the version that `--version` reports, not the one checked.
`solc` also uses 4294967295 as its wildcard marker, so it accepts
`pragma solidity ^4294967295;`; `solar` reads it as a number and warns.

Rationale: a hard error would reject sources pinned to a different `solc`
release even when `solar` can compile them. The warning still flags sources
that ask for different language semantics.

Coverage: `tests/ui/parser/pragma/pragma_version_mismatch.sol`,
`tests/ui/parser/pragma/pragma_valid.sol` and
`tests/ui/parser/pragma/pragma_version_component_above_signed_int.sol`; the upstream
`wrong_compiler_*`, `broken_version_1` and `version_component_*_greater`
fixtures remain excluded from solc parity testing.

## Name Resolution

### NAME-001: Yul declarations can shadow enclosing Yul names

Status: intentional.

Difference: `solc` rejects a Yul variable or function whose name is already
declared in any enclosing Yul scope, including across function boundaries and
`for` loop parts, with errors 1395 and 6052. `solar` rejects only a name that
is already declared in the same Yul scope, so `{ let x := 1 { let x := 2 } }`
compiles and the inner `x` shadows the outer one.

Rationale: an inner declaration resolves without ambiguity to the innermost
scope, as in Solidity, so the code compiles to the same program the author
wrote. Duplicates within one scope stay errors because Yul has no overloads,
and a use of the name there would be ambiguous.

Coverage: `tests/ui/yul_lowering/name_clashes.sol` and
`tests/ui/yul_lowering/yul_function_scope.sol`.

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
storage copy converts element-wise. The seed also gives a common type to
elements that have none of their own: `solar` accepts
`int256[3] memory a = [100, 0, int256(-100)];`, where `solc` reports error 6378
because `uint8` and `int256` have no common type. Positional base constructor
arguments are seeded too, in an inheritance specifier such as
`contract E is B([100, int256(-100)])` and in a constructor header, and so is
the empty literal: `s = [];` clears a `uint256[]` storage array, where `solc`
reports error 6378. Named base constructor arguments, function call arguments
and `abi.encode` operands are not seeded and keep `solc`'s rule.

Rationale: we deliberately support this extended form. The seed gives a
literal the element type of its destination, which is how a nested literal
copied into storage picks up the destination's element type, and the same
rule makes `uint256[2] memory x = [1, 2];` mean what it reads. The widened
values are correct; only the acceptance is wider than `solc`'s, and every
program `solc` accepts here has the same meaning in `solar`.

Coverage: `tests/ui/typeck/inline_array_reference_elements.sol`,
`tests/ui/typeck/inline_array_common_type.sol`,
`tests/ui/typeck/array_push_element_locations.sol`,
`tests/ui/codegen/run-call/arrays/inline_array_mixed_elements.sol`, and
`tests/ui/codegen/run-call/storage/nested_array_storage_memory.sol`.

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

Coverage: `tests/ui/typeck/inheritance/base_arguments.sol`,
`tests/ui/typeck/inheritance/modifier_arguments.sol`,
`tests/ui/codegen/run-call/constructors/base_constructor_args.sol`,
`tests/ui/codegen/run-call/calls/named_arguments_extended.sol`, and
`tests/ui/codegen/run-call/modifiers/modifier_named_arguments_override.sol`.

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

Coverage: `tests/ui/codegen/run-call/try-catch/try_parenthesized_target.sol`.

### TYPECK-006: Oversized fixed-array copies

Status: intentional.

Difference: Solar rejects copying or ABI-encoding fixed arrays with more than
`2^64 - 1` elements during type checking, including arrays nested in structs.
It still accepts their storage declarations, indexed accesses, and storage
reference bindings. Solc 0.8.38 compiles the storage-to-storage array, tuple,
and struct copies covered by the fixture, but those copies panic with code
`0x41` at runtime. ABI-encoding the same array causes an internal compiler
error in solc 0.8.38. These results hold with optimization enabled or
disabled.

Rationale: report unsupported copies at their source during type checking,
rather than fail during lowering or emit a runtime panic for a known oversized
copy. The restriction applies to copying the values, not to addressing their
storage.

Coverage: `tests/ui/typeck/storage_oversized_copy.sol` and
`tests/ui/codegen/run-call/storage/full_width_storage_layout.sol`.

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

### TYPECK-010: Order of constant dependencies across sources

Status: parity debt.

Difference: when a constant reaches cycles through dependencies declared in
different sources, error 6161 names the first of them. `solc` orders
dependencies by AST ID, which follows parse order: the input files by name,
then each newly imported source. `solar` orders them by declaration after
sorting sources by imports, so an imported source comes first.

Rationale: `solar` does not track which sources were input files, which `solc`
needs to number its AST.

Coverage: `tests/ui/typeck/constant_cycles/cross_source_dependency_order.sol`.

### TYPECK-023: AST JSON

Status: parity debt for the types of a few expressions; intentional for the
rest.

Difference: `solar` writes the compact AST JSON of `solc` (the `ast` output of
Standard JSON and `--emit=ast`). Node IDs do not follow the numbering of
`solc`: items take the IDs that `immutableReferences`, the storage layout and
ETHDebug report, source units and imports follow them, and every other node
takes the next free ID. `internalFunctionIDs` numbers the functions that
function values reach by source name and position, as `solc` numbers its nodes.
`solar` leaves out the `EnumValue` documentation, which its AST does not keep, and reads
`contractDependencies`, `usedEvents` and `usedErrors` from the call graph used for
code generation. The source index in a `src` and the `id` of a source follow the
order of the sources after sorting them by their imports, where `solc` sorts them
by name, and the IDs of the nodes that are no items depend on the order in which
the sources are written. `solar` includes a comment between a variable and its `;`
in the location of the variable.

The attributes of expressions come from the type checker, and differ in these
cases:

- a builtin function that is the component of a tuple, such as `abi.encode` in
  `(abi.encode, 2)`, has the kind `internal` and no call options, and
  `abi.decode` there keeps its parameters, where `solc` gives the kind of the
  builtin and `function () pure`;
- `isPure` of `type(L.E)`, `type(L.E).min` and `type(L.E).max` is `true` for an
  enum `E` that a library or contract `L` declares, where `solc` has `false` for
  the name `L.E`, which lowering reduces to the type;
- `solc` types `require` and the functions attached to a type from the call, which
  `solar` rebuilds from the arguments and the declared receiver;
- before Byzantium, only the call of an external function with one dynamically
  encoded return value gets the inaccessible dynamic type; a tuple of returns
  and a bare `call` keep the types of their components;
- the names before the last one in the callee of `emit` and `revert`, and in the
  argument of `type(...)`, take their resolution from name resolution alone.

`solar` returns no AST for `settings.stopAfter: "parsing"`, and warns that the
AST needs analysis, where `solc` returns the parse-only AST: the same nodes
without `typeDescriptions`, references, scopes and the attributes that type
checking sets. A `stopAfter` of `lowering` or `analysis` has no such counterpart
in `solc`, which accepts `parsing` only; `solar` attaches the AST when analysis
ran.

Rationale: IDs must agree across the outputs of one compilation, not with
`solc`. Consumers that look up declarations by ID do not need `solc`'s
numbering, and the differences of the types do not change what an expression
refers to. The writer walks the AST and reads the IDs, types and resolutions of
each node from the HIR in lockstep, which undoes what lowering rewrites, so that
no second classifier of nodes exists. A parse-only AST has no HIR, and about 50
serializers would need an AST-only variant of every field they read from it, in
effect a second walker. The use that needs the AST, matching `immutableReferences`
and the storage layout to declarations, needs a compiled contract.

Coverage: `tests/ui/standard-json/ast/test.jsonc`,
`tests/ui/standard-json/ast-attached/test.jsonc`,
`tests/ui/standard-json/ast-ids/test.jsonc`,
`tests/ui/standard-json/ast-immutables/test.jsonc`,
`tests/ui/standard-json/ast-import-aliases/test.jsonc`,
`tests/ui/standard-json/ast-inaccessible/test.jsonc`,
`tests/ui/standard-json/ast-internal-function-ids/test.jsonc`,
`tests/ui/standard-json/ast-is-pure/test.jsonc`,
`tests/ui/standard-json/ast-license/test.jsonc`,
`tests/ui/standard-json/ast-tuples/test.jsonc`,
`tests/ui/standard-json/ast-yul/test.jsonc`,
`tests/ui/standard-json/ast-yul-hosts/test.jsonc` and
`tests/ui/cli/emit_ast.sol`.

### TYPECK-011: Module-typed conditionals

Status: intentional.

Difference: `solar` rejects conditionals whose branches are modules, such as
`(c ? M : M).x`, which `solc` accepts.

Rationale: codegen resolves module members statically and would drop the
condition, and real code does not use the construct.

Coverage: `tests/ui/typeck/module_conditional.sol`.

### TYPECK-013: Inline assembly names and constants

Status: intentional.

Difference: `solar` accepts inline assembly that `solc` rejects:

- reads of value-type constants whose value is not a number literal, such as
  `uint(7)`, `true ? 5 : 6`, `three + 1`, `~uint(0)` or `keccak256("abc")`
  (`solc` error 7615), if `solar` computes the value at compile time;
- Yul declarations that shadow a Solidity declaration outside the block, such
  as a parameter, contract, function or `msg` (`solc` error 3859);
- the Yul names `this`, `super` and `_` (`solc` error 4113);
- Yul names reserved for EVM instructions and pure Yul builtins, such as `pc`,
  `dup1` or `datasize` (`solc` error 5017);
- a Yul variable inside a Yul function that shares its name with a Solidity
  local, such as `function g() -> x` in a function that returns `x` (`solc`
  error 6578).

Rationale: these rules reject code that `solar` compiles correctly. Yul scoping
resolves the inner declaration first, and constant folding gives assembly the
same value Solidity sees. An initializer reads the declaration its own `let`
shadows: `uint x = 5; assembly { let x := add(x, 1) }` sets the Yul `x` to 6.
The parser also serves formatters and linters, which should accept the names.

Coverage: `tests/ui/codegen/run-call/assembly/assembly_constant_expressions.sol`,
`tests/ui/codegen/run-call/assembly/assembly_let_initializer_scope.sol`,
`tests/ui/codegen/run-call/assembly/assembly_names.sol` and
`tests/ui/typeck/assembly_unfolded_constant.sol`.

### TYPECK-016: ABI heads are bounded as a whole

Status: intentional.

Difference: `solar` rejects a type when any head of its ABI encoding, including
the heads inside the tails of dynamic parts, takes `2^32 - 1` bytes or more. A
struct's head counts all its members together. The check covers calldata
variables; parameters of constructors, events, errors and external function
types; parameters and returns of public and external functions; arguments of
`abi.encode` and its variants other than `abi.encodePacked`; and `abi.decode`
types. `solc` 0.8.38 checks only constructor and public callable parameters
against calldata, and sizes structs one member at a time, so it accepts, for
example, an external function that takes a calldata struct with two
`uint256[134217727]` members.

Rationale: the backend computes head sizes in 64 bits, and larger heads exhaust
memory or overflow during lowering. No transaction can pass or return such a
value within a block gas limit.

Coverage: `tests/ui/typeck/abi_head_size.sol`,
`tests/ui/typeck/public_param_calldata_size.sol` and
`tests/ui/typeck/struct_member_sizes.sol`.

### TYPECK-018: `msg.gas`

Status: intentional.

Difference: `solar` accepts `msg.gas` as `gasleft()` with warning 1400, where
`solc` rejects it with error 1400.

Rationale: the meaning is unambiguous and we compile it correctly, so a
warning is enough to steer code to `gasleft()`.

Coverage: `tests/ui/typeck/msg_gas.sol`.

### TYPECK-019: Struct nesting depth

Status: intentional.

Difference: `solar` reports "struct is too deeply nested" (5651) when the
longest chain of directly nested structs reaches 256. `solc` measures the depth
of its search tree, which skips a struct already reached through a shorter
path, so it accepts some struct graphs with a longer chain.

Rationale: code that walks struct types recurses along the longest chain, and
the `solc` depth depends on field order. Real code does not nest structs this
deeply.

Coverage: `tests/ui/typeck/recursive_types/recursive_struct_deep_direct.sol`.

### TYPECK-020: Early constant evaluation

Status: intentional.

Difference: `solar` evaluates array lengths, base slots and constant operands
like `solc`'s `ConstantEvaluator`, with these exceptions:

- An array length can read a constant declared after the array type, which
  `solc` rejects with error 5462.
- `solc`'s static analyzer evaluates the operands of every division and modulo
  by a constant, and the modulus of every `addmod` and `mulmod`, and reports
  the arithmetic errors (2643, 3667, 6020), long chains (5210) and `erc7201`
  arguments that are not strings (9796) it finds in them. `solar` reports no
  error in these operands, only a zero divisor or modulus (1211 or 4195), also
  when the dividend fails with an error `solc` reports.
- A literal operation without a value in a constant that an array length
  reads, such as `1 / 0`, gets error 2271, as it does wherever the constant is
  used. `solc` reports it as 6020 when it evaluates the array length first.
- A typed shift by a negative or signed amount gets error 2271 from type
  checking, and an array length that reads it gets 5462 too. `solc`'s
  evaluator reports one error 6020 instead.

Rationale: the order of declarations does not change the value of a constant,
and each error is reported once, where the expression is checked.

Coverage: `tests/ui/codegen/run-call/constants/array_len_const_eval.sol`,
`tests/ui/typeck/array_length/array_len_eval_errors.sol`,
`tests/ui/typeck/array_length/array_len_unsupported.sol`,
`tests/ui/typeck/array_length/array_len_typed_negative_shift.sol`,
`tests/ui/typeck/constants/division_by_zero_solc_values.sol`,
`tests/ui/typeck/constants/literal_division_by_zero.sol` and
`tests/ui/typeck/constants/constant_depth_limit.sol`.

### TYPECK-021: Repeated Yul multi-assignment targets

Status: intentional.

Difference: `solc` rejects a Yul multi-assignment that names the same target
twice, such as `x, x := f()` or `a.length, a.length := f()`, with error 9005.
`solar` accepts plain and suffixed repeats, and the leftmost value wins.

Rationale: targets are written right to left, so the result is well defined,
and rejecting the code would only block programs we compile correctly.

Coverage:
`tests/ui/codegen/run-call/assembly/yul_multi_assignment_duplicate_targets.sol`.

### TYPECK-022: `@custom:solar-` tags are requirements

Status: intentional.

Difference: `solc` reads every `@custom:` NatSpec tag as documentation. `solar`
reserves the `solar-` names: an unknown `@custom:solar-` tag, or one
documenting something it cannot apply to, such as a statement, is an error. `@custom:solar-safe` on a contract or a library is an error
when the code it runs, outside functions tagged `@custom:solar-trusted` or in
contracts tagged so, uses inline assembly. `unchecked` blocks are allowed
unless the tag explicitly requests the `arithmetic` property.
`@custom:solar-optimize` documents a non-abstract contract, a library, or a
function with a body and names exactly one objective, `gas` or `size`, which
its code uses instead of the build's, in an `-O none` build too.
`@custom:solar-inline [always|never]` asks the inliner to favor, always clone,
or never clone a function. `@custom:solar-cold` marks a function with a body as
rarely run. [TAGS.md](TAGS.md) describes each tag.

Rationale: a tag states a requirement this compiler checks or relies on, so a
misspelled or misplaced tag must not silently go unchecked. Other compilers
still read the tags as documentation, and no tag changes what the code
computes.

Coverage: `tests/ui/natspec/solar_tags.sol`, `tests/ui/natspec/solar_inline.sol`,
`tests/ui/natspec/solar_optimize.sol`, `tests/ui/natspec/solar_cold.sol`,
`tests/ui/typeck/solar_safe.sol`,
`tests/ui/codegen/mir/optimize-function/` and
`tests/ui/codegen/mir/inline/solar_inline_forms.sol`.

### TYPECK-024: Hexadecimal literals of 39 and 41 digits

Status: intentional.

Difference: `solc` rejects a hexadecimal literal of 39 or 41 digits with error
9429, because it looks like an address that is not exactly 40 digits long.
`solar` accepts it as an integer. Literals of 40 digits are still addresses
with a checksum check.

Rationale: a literal that is not 40 digits long is a number, and `solar`
has no reason to guess that the author meant an address.

Coverage: `tests/ui/typeck/literals/hex_literal_39_41_digits.sol`.

## Contract-Level Checks

### CONTRACT-001: Base slots that solc does not evaluate

Status: intentional.

Difference: `solar` accepts base slots of storage layout specifiers whose value
its constant evaluator computes after type checking but `solc`'s does not, such
as conditionals (`layout at true ? 42 : 94`), conversions, member accesses and
builtins, and the constants they read. `solc` rejects them with error 1505,
and with error 9796 for `erc7201` of a string its evaluator does not
compute.

Rationale: the base slot has a well-defined value. Errors in a value that
`solc` does not compute get `solc`'s code for it, 1505.

Coverage: `tests/ui/typeck/storage_layout/storage_layout_base_slot_ternary.sol`,
`tests/ui/typeck/storage_layout/storage_layout_base_slot_erc7201.sol` and
`tests/ui/codegen/run-call/storage/storage_layout_base_slot_const_eval.sol`;
upstream `test/libsolidity/syntaxTests/storageLayoutSpecifier/ternary_operator.sol`.

## Code Generation

### CODEGEN-001: Immutables and tuple conditionals handle dirty bits differently

- ID: CODEGEN-001
- Status: intentional
- Difference: When inline assembly leaves bits above a variable's type, `solar`
  cleans the raw bits of an integer or fixed-bytes immutable when it stores
  the immutable, so `uint8 immutable i = f()` reads back clean at run time
  where `solc` via-IR keeps them. A `bool` or `address` immutable that the
  constructor reads back keeps them where `solc` via-IR cleans them. A
  conditional with tuple branches keeps an invalid enum, contract or
  user-defined value type in its false branch raw, where `solc` via-IR
  validates it.
- Rationale: All three cases need a word that inline assembly made invalid for
  its type, which the Solidity documentation leaves unspecified.
- Coverage: the fixtures under `tests/ui/codegen/run-call/dirty-values/`, whose
  expectations come from `solc --via-ir --optimize`.

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
  and EIP-170 parity for the rest of the corpus. A contract that must fit can
  be built with `-O size`, or documented `@custom:solar-optimize size`, which
  compiles its code as a `-O size` build would while the rest of the build
  keeps its objective; other compilers read the tag as documentation.
- Coverage: `cargo tq foundry-external seaport`; the exact exemptions live in
  `SEAPORT_CODE_SIZE_SKIPS` in `tools/tester/src/foundry/external.rs`.

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

### CODEGEN-006: Legacy source-map modifier depth

- ID: CODEGEN-006
- Status: implemented
- Behavior: Legacy `sourceMap` output carries the compiler's modifier nesting
  depth in the `m` field, preserving it through MIR and EVM IR lowering and
  optimization. Shared code with different modifier depths has no unique
  modifier frame and uses depth zero. Shared code with multiple source origins
  is unmapped in legacy output; ETHDebug retains every source alternative.
  This policy applies equally to MIR and EVM IR sharing. Like solc, every entry
  of a constructor's modifier list, including base constructor calls, adds one
  level of depth to the code after it.
- Coverage: `tests/ui/standard-json/source-maps/modifier.jsonc` and
  `tests/ui/standard-json/source-maps/modifier-constructor.jsonc`.

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
  `tests/ui/codegen/run-call/revert-strings/`,
  `tests/ui/codegen/run-call/libraries/library_delegatecall_guard.sol`.

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
  expected source text before applying the correction. The pinned Seaport
  suite's ascending and descending amount tests save `block.timestamp` before
  `vm.warp` and compute the expected amount from it afterwards; the runner
  reads the warped timestamp through an external helper instead, so the five
  affected tests pass under both compilers.

### CODEGEN-009: Static frames sit below the initial free memory pointer

- ID: CODEGEN-009
- Status: intentional
- Difference: `solc` starts the free memory pointer at `0x80`. `solar` keeps
  internal-call frames and spill slots in static memory from `0x80` up to the
  initial free memory pointer, so a contract's heap starts above every frame it
  can reach. Inline assembly that stores data at constant addresses in that
  range, instead of allocating through the free memory pointer, can have it
  overwritten by any internal call, including the helpers the compiler
  generates for ABI encoding and pre-Cancun memory copies. Those copies may
  call one shared helper, unless a pointer is computed from constants or
  calldata alone, so a memory object that assembly places in that range through
  any other computation can be overwritten by the helper's frame before the
  copy reads it.
- Rationale: The Solidity documentation counts only scratch space, memory
  allocated through the free memory pointer, and memory past the free memory
  pointer within one assembly block as memory-safe. Static frames make internal
  calls cheaper than a memory stack, and memory-safe assembly never reaches
  them.
- Coverage: `tests/ui/codegen/lowering/mir/storage-memory/pre_cancun_memory_copies.sol`
  encodes an object that assembly allocates through the shared copy helper.
  `tests/ui/codegen/lowering/mir/storage-memory/mcopy_call_results.sol` copies call
  results through it, and
  `tests/ui/codegen/run-call/memory/mcopy_shared_helper_raw.sol` keeps a
  constant address inline. `tests/ui/codegen/mir/lower-mcopy/helper_eligibility.mir`
  pins which copies may call it.

### CODEGEN-010: Assembly cannot hand compiler-owned memory to the heap

- ID: CODEGEN-010
- Status: intentional
- Difference: When inline assembly leaves a word in the free memory pointer
  slot that is not provably a heap pointer, every later read of the slot as the
  pointer sees at least the initial free memory pointer: an allocation, or an
  `mload` whose word addresses memory. After `mstore(0x40, 0x80)`, such a read
  sees `0x80` under `solc` and the initial pointer here. The word is a
  constant, a word that calldata, memory, a parameter, or a call result
  supplies, or one that a store through a computed address, a copy, or a byte
  store leaves there. A pointer from the heap is proven, also when moved by an
  offset that cannot wrap the address space, or stepped back from by a
  constant outside a loop. A word that is also read as data, such as one hashed
  in scratch memory, or loaded and then compared, hashed, encoded, stored, or
  used as a key, stays in the slot for those reads, and only the pointer reads
  see the raised value. The raise is `max(word, heap floor)`, where the heap
  floor is resolved when frames are placed, so a pass that later folds the
  word, as inlining does for a helper's argument, cannot remove it.

  Each entry places its spill slots and frames clear of every fixed memory
  range that the code it runs names, wherever the range lies, when the area,
  its allocations, and its frames would cover it. An entry that runs code
  writing memory at an address computed from calldata, as Seaport lays out a
  basic order's hashes and event data, also keeps them above `0x2080`, unless it
  places none of them. Other entries keep their memory low unless they share
  those frames or a function that reads the heap floor, which gives them one
  initial pointer.

  A write whose destination the compiler cannot place on the heap, in a frame,
  or below the heap start, such as a copy to a computed destination of any
  length, a store through one, or a call that returns data to one, keeps the
  values that are live across it out of the spill area. A call result counts as
  a heap pointer only when every return of its callee is one, and a word loaded
  from memory never does. The constructor's spill area lies above its staging
  area and not above fixed ranges, so a write to one above that area counts as
  such a write there. This costs memory expansion gas and code size.

  All of this applies only to a contract with inline assembly that does not
  promise memory safety. As under `solc`, an assembly block marked memory-safe,
  with the `"memory-safe"` dialect flag or the `@solidity memory-safe-assembly`
  comment, promises to touch only scratch space, memory it allocates by bumping
  the free memory pointer, and memory the program allocated. A contract whose
  assembly is all marked, or that has none, has no write the compiler cannot
  place, so it keeps the code it had before these rules: it trusts the type of a
  memory reference, a word loaded from a memory object, and a call result as
  heap pointers, and raises only constants and calldata words stored into the
  free memory pointer slot. Such a block cannot store a low address into an
  object's field without breaking its promise. An unmarked block that writes no
  memory (no `mstore`, `mstore8`, copy, or call, and no assignment to a memory
  reference) breaks no promise either, so it does not count.
- Rationale: The spill slots and internal-call frames below the initial free
  memory pointer (CODEGEN-009) hold values that `solc` keeps on the stack, so
  the next allocation after a lowered pointer, or the absolute layout itself,
  would replace them. Raising only the pointer reads keeps the stored value for
  code that uses the slot as scratch, such as an error argument before a revert
  or a hash input before the pointer is restored. A pointer derived from the
  heap already lies above the initial one. The `heap-floor` pass documents how
  it tells pointer reads from data reads. Writes the compiler cannot place are
  assumed to reach the spill area, because no later pass can tell where an
  input or a call will point them. Spill stores after a write at an address that
  calldata computes can still overwrite what it laid out when the layout grows
  past `0x2080`, or is indexed by a loop counter alone; Seaport's basic orders
  with about 40 or more additional recipients do.
  The compiler checks a memory object's element and field offsets against the
  object's bounds and trusts them, so assembly that forges a length can make a
  checked access wrap below the object, which both compilers allow.
- Coverage: `tests/ui/codegen/run-call/assembly/assembly_low_memory_layouts.sol`,
  `tests/ui/codegen/run-call/assembly/assembly_low_memory_routes.sol`,
  `tests/ui/codegen/run-call/assembly/assembly_low_memory_recursive_routes.sol`,
  `tests/ui/codegen/run-call/assembly/assembly_low_memory_places_nothing.sol`,
  `tests/ui/codegen/run-call/assembly/assembly_low_memory_places_something.sol`,
  `tests/ui/codegen/lowering/evm-ir/spills/constructor_constant_write_spills.sol`,
  `tests/ui/codegen/lowering/evm-ir/spills/folded_guard_spills.sol`,
  `tests/ui/codegen/lowering/evm-ir/spills/folded_hazard_spills.sol`,
  `tests/ui/codegen/lowering/evm-ir/spills/folded_operand_spills.sol`,
  `tests/ui/codegen/lowering/evm-ir/spills/folded_tail_call_spills.sol`,
  `tests/ui/codegen/lowering/evm-ir/spills/spill_write_provenance.sol`,
  `tests/ui/codegen/mir/heap-floor/checked_counter_offset.mir`,
  `tests/ui/codegen/mir/heap-floor/heap_floor.mir`,
  `tests/ui/codegen/mir/heap-floor/raised_by_callee.mir`, and Seaport's own suite in
  `cargo tq foundry-external seaport`, whose runner keeps
  `PreAndPostFulfillmentCheckTest::testBasicStateful(uint8,uint8)` at 38
  additional recipients or fewer; 39 already fail.

### CODEGEN-011: ABI coder v1 is not supported

- ID: CODEGEN-011
- Status: intentional
- Difference: `solc` applies ABI coder v1's restrictions to a source unit that
  selects it, so that unit cannot use structs or nested dynamic arrays at ABI
  boundaries. `solar` warns that ABI coder v1 is not supported, generates ABI
  coder v2 code, and does not apply v1's restrictions. The warning reuses
  `solc`'s 9511 code, so `--allow 9511` also hides this change.
- Rationale: ABI coder v1 is deprecated, and both coders encode the types v1
  accepts the same way. A second coder would only change which types compile.
- Coverage: `tests/ui/codegen/run-call/abi-decode/abicoder_v1_uses_v2.sol` and
  `tests/ui/parser/pragma/pragma_valid.sol`.

### CODEGEN-012: External function pointer suffixes keep only their field bits

- ID: CODEGEN-012
- Status: intentional
- Difference: `solc` keeps an external function pointer in assembly as two
  raw stack words, so after `f.address := not(0)` and `f.selector := not(0)`
  both `f.address` and `f.selector` read back as `2^256 - 1`. `solar` stores
  the pointer as one packed word, so an assignment keeps only the low 160 bits
  of the address and the low 32 bits of the selector, and assembly reads the
  cleaned values. Solidity-level reads match in both compilers.
- Rationale: the packed word cannot hold two raw 256-bit values.
- Coverage:
  `tests/ui/codegen/run-call/function-pointers/yul_function_pointer_suffix_dirty_bits.sol`.

### CODEGEN-013: Constants are values

- ID: CODEGEN-013
- Status: intentional
- Difference: `solc` evaluates the initializer of a constant at every use, in
  the arithmetic mode of the use. `solar` evaluates a constant once at compile
  time and uses its value. A constant without a value at compile time, because
  its initializer reverts, as with `uint8 constant C = A + 1` where `A` is
  `255`, or because `solar` does not compute it, such as
  `keccak256(type(C).creationCode)` or a byte array larger than 64 KiB, gets a
  helper function that runs its initializer, one per arithmetic mode. Each use
  calls the one for its mode, so such a constant wraps inside `unchecked`.
- Rationale: a constant is one value, and computing it once is cheaper than
  running its initializer at every use.
- Coverage: `tests/ui/codegen/run-call/events-errors/constant_reverting_initializer.sol`,
  `tests/ui/codegen/run-call/constants/constant_eval_aggregates.sol`,
  `tests/ui/codegen/run-call/constants/constant_runtime_initializer.sol`,
  `tests/ui/codegen/run-call/constants/constant_bytes_too_large.sol` and
  `tests/ui/codegen/lowering/mir/expressions/constant_values_too_large.sol`.
