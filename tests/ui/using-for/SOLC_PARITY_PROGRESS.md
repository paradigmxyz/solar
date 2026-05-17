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

## In progress

## Remaining solc parity risks

- [ ] Exact `this`/`super`/module path resolution in `using` entries.
- [ ] Exact solc library-member visibility semantics.
- [ ] Exact diagnostics for fallback from builtin to user-defined operators.
- [ ] Full syntax-only matrix for malformed `using` directives.
- [ ] Mutability side-effect checks for operator bodies.
- [ ] Full duplicate operator matrix across source/contract/import scopes.
- [ ] Full imported and transitive imported `using` matrix.
