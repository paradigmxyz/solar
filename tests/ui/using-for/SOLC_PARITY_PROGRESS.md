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
- [x] Global library directive coverage for enum, struct, and UDVT.
- [x] Global using lookup when the type is not directly nameable in the current source.
- [x] Imported global operator coverage.
- [x] Invalid operator implementor coverage.
- [x] Duplicate operator detection across global and non-global directives.
- [x] Duplicate same-function operator definitions are deduplicated.
- [x] Imported duplicate operator coverage.
- [x] Transitive imported global operator coverage.
- [x] Wrong-source transitive imported global operator diagnostics.
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
- `semanticTests/using/using_global_all_the_types.sol` ->
  `global/global_library.sol` + `global/auxiliary/global_library.sol`.
- `semanticTests/using/using_global_invisible.sol` ->
  `global/global_library_invisible.sol` + `global/auxiliary/global_invisible_*.sol`.
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
  `imports/transitive_global_operator_wrong_source.sol` +
  `imports/auxiliary/global_wrong_*.sol`.
- Valid transitive global operator lookup ->
  `imports/transitive_global_operator.sol` + `imports/auxiliary/transitive_*.sol`.
- `operators/userDefined/calling_operator_imported_transitively_non_global.sol` ->
  `imports/transitive_non_global_operator.sol` + `imports/auxiliary/non_global_*.sol`.
- `operators/userDefined/multiple_operator_definitions_different_functions_global_and_non_global_different_files.sol`
  -> `imports/imported_duplicate_operator.sol` + `imports/auxiliary/transitive_base.sol`.

## In progress

## Remaining solc parity risks

- [ ] Mutability side-effect checks for operator bodies.

## Feature Coverage Audit

| Feature | Solar coverage | Solc comparison | Status |
| --- | --- | --- | --- |
| Basic attached free functions | `methods.sol`, `attached_functions.sol` | `using_free_functions.sol`, `free_functions_individual.sol`, `free_function_multi.sol` | Covered; Solar also checks bare attached members are call-only. |
| Basic attached library functions | `methods.sol`, `libraries/visibility.sol`, `attached_functions.sol` | `library_functions_inside_contract.sol`, `library_functions_at_file_level.sol`, `library_functions_attached_in_single_directive_*` | Covered. |
| Braced free-function overload rejection | `overloads/braced_free_overload_rejected.sol` | `free_functions_non_unique_err.sol`, `free_overloads.sol` | Covered; Solar wording is shorter. |
| Library member overload resolution | `overloads/library_member_overloads.sol`, `overloads/ambiguous_library_member.sol`, `attached_functions.sol` | solc overload resolution in `TypeChecker::tryApplyMemberFunction` | Covered by positive and ambiguous calls; Solar wording is shorter. |
| Receiver binding at call site | `attached_functions.sol`, `methods.sol` | solc removes the receiver when applying using-for functions | Covered, including wrong explicit argument counts. |
| Global free-function directives | `global/global_directives.sol`, `global/auxiliary/global_directives.sol` | `using/global_working.sol` | Covered. |
| Global library directives | `global/global_library.sol`, `global/global_library_invisible.sol`, `global/auxiliary/global_library.sol` | `semanticTests/using/using_global_all_the_types.sol`, `semanticTests/using/using_global_invisible.sol` | Covered, including enum, struct, UDVT, external library member, free function, and non-nameable returned type. |
| Invalid global target type | `global/invalid_global_targets.sol`, `invalid_directives.sol` | `global_for_non_user_defined.sol`, `global_library_for_builtin.sol`, `global_library_for_interface.sol`, operator builtin/contract/library/interface tests | Covered. |
| Global target must be same source file level | `global/imported_type_global.sol`, `invalid_directives.sol`, `imports/transitive_global_operator_wrong_source.sol` | `global_for_type_from_other_file.sol`, `global_for_type_defined_elsewhere.sol`, `calling_operator_imported_transitively.sol` | Covered. |
| File-level using is source-local | `imports/file_level_using_not_imported.sol`, `imports/auxiliary/file_level_using.sol` | `file_level_inactive_after_import.sol` | Covered; Solar wording is shorter. |
| Import and module alias paths | `imports/imported_members.sol`, `imports/module_alias.sol`, `imports/auxiliary/imported_using.sol` | `module_2.sol`, `module_3.sol`, `library_import_as.sol` | Covered. |
| Reference type data locations | `locations/reference_types.sol`, `locations/calldata_receiver.sol`, `locations/calldata_rejects_memory.sol` | `free_reference_type.sol`, `bound_calldata_parameter_accepting_calldata.sol`, `bound_calldata_parameter_not_accepting_memory.sol` | Covered; Solar member-not-found wording is shorter. |
| User-defined operators | `operators.sol` | `operators/userDefined/calling_operator.sol`, operator equality tests | Covered. |
| Imported global operators | `operators/imported_global.sol`, `operators/auxiliary/operator_imported.sol`, `imports/imported_global_operator.sol`, `imports/transitive_global_operator.sol`, `imports/auxiliary/transitive_*.sol` | `calling_operator_imported.sol`; valid transitive propagation through the type source | Covered. |
| Imported non-global operators | `imports/imported_non_global_operator.sol`, `imports/imported_non_global_operator_definition.sol`, `imports/transitive_non_global_operator.sol`, `imports/auxiliary/non_global_*.sol` | `calling_operator_imported_non_global.sol`, `calling_operator_imported_transitively_non_global.sol` | Covered; Solar suppresses some follow-on builtin operator diagnostics after invalid directives. |
| Duplicate operator definitions | `invalid_operators.sol`, `operators/duplicate_distinct_functions.sol`, `operators/duplicate_same_function.sol`, `imports/imported_duplicate_operator.sol` | `multiple_operator_definitions_*` | Covered for valid duplicate definitions and same-function dedupe; Solar suppresses the solc duplicate follow-on for an already-invalid non-global directive. |
| Contract-scope operator directives | `operators/contract_scope_duplicates.sol`, `operators/contract_scope_not_inherited.sol` | `using_for_with_operator_at_contract_level_in_base_contract.sol`, `multiple_operator_definitions_on_file_and_contract_level.sol` | Covered; Solar reports the syntax-only non-global-operator error and suppresses some follow-on operator lookup errors. |
| Invalid operator implementors | `operators/invalid_implementors.sol`, `operators/invalid_event_implementor.sol`, `invalid_operators.sol` | `implementing_operator_with_contract_function_at_file_level.sol`, `implementing_operator_with_library_function_at_file_level.sol`, `implementing_operator_with_event.sol`, `implementing_operator_with_non_pure_function.sol` | Covered. |
| Operator fallback on incompatible operands | `operators/implicit_conversion_failures.sol` | `calling_operator_with_implicit_conversion.sol` | Covered; Solar uses regular builtin/mismatch diagnostics instead of solc's specialized user-operator operand wording. |
| Operators are not attached methods | `operators/operator_not_method.sol` | `calling_operator_as_attached_function_via_function_name.sol` | Covered; wording differs by diagnostic style. |
| Invalid `this`/`super` qualified paths | `paths/this_and_super.sol` | `external_function_qualified_with_this.sol`, `function_from_base_contract_qualified_with_super.sol` | Covered; Solar reports a more specific builtin-path error. |
| Invalid using directive targets | `invalid_resolution.sol`, `invalid_directives.sol` | `using_contract_err.sol`, `function_name_without_braces_*`, `using_non_function.sol`, `using_free_no_parameters_*`, `free_functions_implicit_conversion_err.sol`, `private_library_function_outside_scope.sol` | Covered. |
| Library self-call through attached members | `libraries/self_call.sol` | `library_functions_attached_at_file_level_used_inside_library.sol`, `unqualified_library_functions_attached_in_single_directive_inside_library.sol` | Covered. |
| Malformed parser forms | `tests/ui/parser/using_*.sol` | parser and syntax tests under `syntaxTests/using` and `operators/userDefined/operator_parsing_*` | Covered outside this directory; Solar keeps generic parser wording. |
| Lookup performance refactors | using-for UI suite plus `cargo cl` | No solc diagnostic analogue | Covered by regression suite. |

## Per-File Solc Comparison

| Solar file | Solc source or basis | Diagnostic comparison |
| --- | --- | --- |
| `attached_functions.sol` | `free_functions_individual.sol`, `free_function_multi.sol`, library overload behavior | Positive behavior covered; bare attached-member errors are Solar-specific wording for a solc-equivalent invalid member use. |
| `global/global_directives.sol`, `global/auxiliary/global_directives.sol` | `using/global_working.sol` | No diagnostics; behavior matches. |
| `global/global_library.sol`, `global/global_library_invisible.sol`, `global/auxiliary/global_library.sol` | `semanticTests/using/using_global_all_the_types.sol`, `semanticTests/using/using_global_invisible.sol` | No diagnostics; semantic lookup matches at type-checking level. |
| `global/imported_type_global.sol`, `global/auxiliary/imported_types.sol` | `using/global_for_type_from_other_file.sol` | Same error meaning: global target must be defined in same source unit at file level. |
| `global/invalid_global_targets.sol` | `global_for_non_user_defined.sol`, `global_library_for_builtin.sol`, `global_library_for_interface.sol`, operator target tests | Same error meaning: `global` only accepts struct, enum, or UDVT targets. |
| `imports/file_level_using_not_imported.sol`, `imports/auxiliary/file_level_using.sol` | `using/file_level_inactive_after_import.sol` | Same failure; Solar says `member ... not found on type ...`, solc says not found or not visible after argument-dependent lookup. |
| `imports/imported_members.sol`, `imports/module_alias.sol`, `imports/auxiliary/imported_using.sol` | `module_2.sol`, `module_3.sol`, `library_import_as.sol` | No diagnostics; behavior matches. |
| `imports/imported_global_operator.sol`, `operators/imported_global.sol`, `operators/auxiliary/operator_imported.sol` | `operators/userDefined/calling_operator_imported.sol` | No diagnostics; behavior matches. |
| `imports/transitive_global_operator.sol`, `imports/auxiliary/transitive_base.sol`, `imports/auxiliary/transitive_mid.sol` | Valid transitive import case derived from solc global lookup rules | No diagnostics; behavior matches the solc rule that globals travel from the type source. |
| `imports/transitive_global_operator_wrong_source.sol`, `imports/auxiliary/global_wrong_*.sol` | `operators/userDefined/calling_operator_imported_transitively.sol` | Same wrong-source global errors; Solar's follow-on operator fallback wording differs. |
| `imports/imported_non_global_operator.sol`, `imports/auxiliary/non_global_operator.sol` | `calling_operator_imported_non_global.sol` | Same non-global-operator errors; Solar suppresses solc's extra fallback operator errors when the invalid directive is local. |
| `imports/imported_non_global_operator_definition.sol`, `imports/auxiliary/defined_non_global_operator.sol` | `calling_operator_imported_non_global.sol` variant | Same non-global-operator errors from imported source; Solar suppresses solc's extra fallback operator errors. |
| `imports/transitive_non_global_operator.sol`, `imports/auxiliary/non_global_*.sol` | `calling_operator_imported_transitively_non_global.sol` | Same non-global-operator errors; Solar suppresses solc's extra fallback operator errors. |
| `imports/imported_duplicate_operator.sol` | `multiple_operator_definitions_different_functions_global_and_non_global_different_files.sol` | Solar reports the invalid non-global operator directive; solc also emits a duplicate-operator follow-on. |
| `invalid_directives.sol` | `using_contract_err.sol`, `using_free_no_parameters_*`, `free_functions_implicit_conversion_err.sol`, `private_library_function_outside_scope.sol`, `global_for_type_defined_elsewhere.sol` | Same error meanings; wording follows Solar style. |
| `invalid_resolution.sol` | `function_name_without_braces_*`, `using_non_function.sol` | Same error meanings; wording follows Solar style. |
| `methods.sol` | `using_free_functions.sol`, library using tests, `using/global_working.sol` | Positive behavior matches; bare attached-member error is Solar-specific wording. |
| `libraries/visibility.sol` | `library_functions_at_file_level.sol`, `library_functions_inside_contract.sol`, `private_library_function_inside_scope.sol` | No diagnostics; behavior matches at type-checking level. |
| `libraries/self_call.sol` | `library_functions_attached_at_file_level_used_inside_library.sol` | Same error meaning: libraries cannot call their own external/public functions externally. |
| `locations/calldata_receiver.sol` | `bound_calldata_parameter_accepting_calldata.sol` | No diagnostics; behavior matches. |
| `locations/calldata_rejects_memory.sol` | `bound_calldata_parameter_not_accepting_memory.sol` | Same failure; Solar member-not-found wording is shorter. |
| `locations/reference_types.sol` | `free_reference_type.sol` | No diagnostics; behavior matches. |
| `operators.sol` | `calling_operator.sol` and equality operator cases | No diagnostics; behavior matches. |
| `operators/contract_scope_duplicates.sol` | `multiple_operator_definitions_on_file_and_contract_level.sol` | Same non-global operator error; Solar suppresses solc follow-on duplicate/fallback diagnostics. |
| `operators/contract_scope_not_inherited.sol` | `using_for_with_operator_at_contract_level_in_base_contract.sol` | Same non-global operator errors; Solar suppresses solc follow-on builtin fallback diagnostic. |
| `operators/duplicate_distinct_functions.sol` | `multiple_operator_definitions_different_functions_same_directive.sol`, separate-directive variants | Same duplicate-operator error meaning; wording follows Solar style. |
| `operators/duplicate_same_function.sol` | `multiple_operator_definitions_same_function_same_directive.sol`, `multiple_operator_definitions_same_function_separate_directives.sol` | No diagnostics; same-function duplicate dedupe matches. |
| `operators/implicit_conversion_failures.sol` | `calling_operator_with_implicit_conversion.sol` | Same rejection behavior; Solar diagnostic wording differs for right-operand mismatch and fallback. |
| `operators/invalid_event_implementor.sol` | `implementing_operator_with_event.sol` | Same error meaning: expected function name. |
| `operators/invalid_implementors.sol` | `implementing_operator_with_contract_function_at_file_level.sol`, `implementing_operator_with_library_function_at_file_level.sol` | Same error meanings; Solar may emit both attachment-kind and pure-free-function diagnostics for the same bad contract function. |
| `operators/operator_not_method.sol` | `calling_operator_as_attached_function_via_function_name.sol` | Same behavior; wording follows Solar member-not-found style. |
| `overloads/ambiguous_library_member.sol` | solc overloaded library member selection | Same ambiguous overload behavior; wording follows Solar style. |
| `overloads/braced_free_overload_rejected.sol` | `free_functions_non_unique_err.sol`, `free_overloads.sol` | Same rejection behavior; Solar says expected function name. |
| `overloads/library_member_overloads.sol` | solc overloaded library member selection | No diagnostics; behavior matches. |
| `paths/this_and_super.sol` | `external_function_qualified_with_this.sol`, `function_from_base_contract_qualified_with_super.sol` | Same invalid behavior; Solar gives a more specific builtin-path diagnostic. |

## Known diagnostic differences

- Solar uses its generic expected-token parser diagnostics for malformed `using` directives rather
  than solc's exact parser wording.
- Solar reports user-defined operator fallback failures with its regular builtin operator and
  mismatched type diagnostics rather than solc's specialized operand mismatch wording.
- Solar generally suppresses duplicate/fallback follow-on diagnostics for `using for` entries that
  were rejected as syntax-only invalid, while solc sometimes reports those follow-ons too.
- Solar member lookup diagnostics are shorter than solc's `not found or not visible after
  argument-dependent lookup` wording.
