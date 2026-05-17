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

## In progress

## Remaining solc parity risks

- [ ] Exact diagnostics for fallback from builtin to user-defined operators.
- [ ] Full syntax-only matrix for malformed `using` directives and parser recovery.
- [ ] Mutability side-effect checks for operator bodies.
- [ ] Remaining duplicate operator matrix across inherited contract scopes.
- [ ] Full imported non-global operator diagnostic matrix.
- [ ] Exact library self-call diagnostics for attached external/public library members.
