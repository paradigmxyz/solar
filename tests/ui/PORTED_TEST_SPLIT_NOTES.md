# Ported Test Split Notes

These ported UI tests were reviewed and intentionally left grouped.

- `tests/ui/typeck/function_ptr_mutability_conversions.sol`: one function pointer mutability conversion matrix. Splitting each pair would duplicate the same assignability setup.
- `tests/ui/typeck/function_ptr_comparisons.sol`: one function pointer comparison matrix with shared external/internal function setup.
- `tests/ui/typeck/function_calls/type_members.sol`: mixed library/interface type-member behavior. The snippets share setup and the upstream mapping is not clean enough to split confidently.
- `tests/ui/typeck/lvalue/immutable.sol`: immutable write contexts interact through one constructor/state-initializer scenario, so splitting may change the behavior being exercised.
- `tests/ui/using-for/operators/operator_definition_matrix.sol`: user-defined operator signature validation matrix. The invalid declarations and fallback operator-use diagnostics are coupled.
- `tests/ui/overrides/stricter_mutability.sol`: override mutability lattice matrix.
- `tests/ui/overrides/diamond_public_vars.sol`: public-variable diamond override matrix.
- `tests/ui/overrides/interface_exception.sol`: interface override exception matrix.
- `tests/ui/overrides/multi_layered.sol`: multi-layer override matrix with shared inheritance graph.
- `tests/ui/overrides/shared_base.sol`: shared-base override matrix with shared inheritance graph.
- `tests/ui/overrides/calldata_memory.sol`: calldata/memory override compatibility matrix.
- `tests/ui/typeck/function_calls/declaration_function_calls.sol`: all diagnostics are the same contract-type function call rule.
- `tests/ui/typeck/function_calls/low_level_call_options.sol`: low-level call option cases exercise the same `value` restriction.

The storage-layout base-slot tests were split by behavior category, but each category still keeps multiple upstream attributions where the cases exercise the same literal/evaluation rule family.
