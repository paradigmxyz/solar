# `using for` solc parity progress

This tracks the local parity work against the solc syntax tests under:

- `test/libsolidity/syntaxTests/using`
- `test/libsolidity/syntaxTests/operators/userDefined`

## Done

- [x] Basic attached free functions.
- [x] Basic attached library functions.
- [x] Basic invalid `using` directive checks.
- [x] Basic invalid attached-function resolution checks.
- [x] Basic user-defined operator checks.
- [x] User-defined operator conversions live on `UserDefinableOperator`.
- [x] Attached-function lookup avoids allocation when no functions are attached.
- [x] User-operator lookup uses callback scanning and `WantOne`.
- [x] Attached library-function overload resolution.
- [x] Braced overloaded free-function rejection.
- [x] Import and module alias coverage.
- [x] Reference-type and data-location coverage.
- [x] Global directive coverage.
- [x] Imported global operator coverage.
- [x] Invalid operator implementor coverage.
- [x] Duplicate operator detection across global and non-global directives.
- [x] Duplicate same-function operator definitions are deduplicated.
- [x] Imported duplicate operator coverage.
- [x] Transitive imported global operator coverage.
- [x] `this`/`super` path rejection coverage.
- [x] Imported type `global` rejection coverage.
- [x] Source-local file-level `using` coverage across imports.
- [x] Imported functions in non-global operator directives.
- [x] Library member visibility attachment coverage.
- [x] Malformed `using` directive parser smoke coverage.
- [x] Expanded malformed `using` parser matrix for wildcard, `global`, and operator forms.
- [x] Contract-level wildcard rejection for specific attached functions.
- [x] Contract-scope duplicate operator coverage.
- [x] Non-inherited contract-scope operator coverage.
- [x] User-defined operator implicit-conversion fallback coverage.
- [x] Library self-call diagnostics for attached external/public library members.
- [x] Imported-source non-global operator diagnostics.
- [x] Transitive imported non-global operator diagnostics.
- [x] Audited ported multi-source solc cases: local ports use `auxiliary/`, not `aux/`.

## Multi-Source Port Audit

Solar UI tests do not embed solc's `==== Source:` blocks. Ported multi-source cases are split
into the nearest `auxiliary/` directory instead.

- `using/global_working.sol` -> `global/global_directives.sol` +
  `global/auxiliary/global_directives.sol`.
- `using/global_for_type_from_other_file.sol` -> `global/imported_type_global.sol` +
  `global/auxiliary/imported_types.sol`.
- `using/file_level_inactive_after_import.sol` -> `imports/file_level_using_not_imported.sol` +
  `imports/auxiliary/file_level_using.sol`.
- `using/module_2.sol`, `using/module_3.sol`, and `using/library_import_as.sol` import coverage ->
  `imports/imported_members.sol`, `imports/module_alias.sol`, and
  `imports/auxiliary/imported_using.sol`.
- `operators/userDefined/calling_operator_imported.sol` ->
  `imports/imported_global_operator.sol`, `operators/imported_global.sol`, and their
  `auxiliary/` sources.
- `operators/userDefined/calling_operator_imported_non_global.sol` ->
  `imports/imported_non_global_operator_definition.sol` +
  `imports/auxiliary/defined_non_global_operator.sol`.
- `operators/userDefined/calling_operator_imported_transitively.sol` ->
  `imports/transitive_global_operator.sol` + `imports/auxiliary/transitive_*.sol`.
- `operators/userDefined/calling_operator_imported_transitively_non_global.sol` ->
  `imports/transitive_non_global_operator.sol` + `imports/auxiliary/non_global_*.sol`.
- `operators/userDefined/multiple_operator_definitions_different_functions_global_and_non_global_different_files.sol`
  -> `imports/imported_duplicate_operator.sol` + `imports/auxiliary/transitive_base.sol`.

## In progress

## Remaining solc parity risks

- [ ] Mutability side-effect checks for operator bodies.

## Known diagnostic differences

- Solar uses its generic expected-token parser diagnostics for malformed `using` directives rather
  than solc's exact parser wording.
- Solar reports user-defined operator fallback failures with its regular builtin operator and
  mismatched type diagnostics rather than solc's specialized operand mismatch wording.
