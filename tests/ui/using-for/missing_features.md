# Using-For Missing Features

This tracks the remaining Solar/solc divergences found while auditing the
focused solc using-for corpus. The goal is to cover the distinct behavior and
diagnostic paths without keeping one UI file per upstream test.

## Missing Semantics

- [x] Qualified enum access through contract types.
  - Upstream:
    `test/libsolidity/semanticTests/enums/using_contract_enums_with_explicit_contract_name.sol`
    and
    `test/libsolidity/semanticTests/enums/using_inherited_enum_excplicitly.sol`
  - Expected: `C.E.V` and `Base.E.V` resolve when `E` is an enum declared in
    `C` or inherited from `Base`.
  - Fixed in this branch: member lookup on `type(contract C)` now exposes nested
    type members, including inherited enum types.

- [x] Imported free-function aliases in braced using directives.
  - Upstream:
    `test/libsolidity/semanticTests/using/imported_functions.sol` and
    `test/libsolidity/syntaxTests/using/global_and_local.sol`
  - Expected: `using {A.f, importedAlias} for T` attaches both a qualified
    namespace function and an imported alias.
  - Fixed in this branch: braced using entries now preserve the source member
    name, so imported aliases attach under the alias name.

- [x] Ambiguity between global and local attached members.
  - Upstream: `test/libsolidity/syntaxTests/using/global_local_clash.sol`
  - Expected: if an imported global using directive and a local using directive
    attach different functions under the same member name, member lookup reports
    the member as ambiguous.
  - Fixed in this branch: the regression suite covers imported global and local
    using directives contributing distinct functions under the same member name,
    producing the existing ambiguity diagnostic.

- [x] Reject library names as using-for target types.
  - Upstream: `test/libsolidity/syntaxTests/using/using_library_for_library.sol`
  - Expected: `using L for M` is invalid when `M` is a library name.
  - Fixed in this branch: type checking now rejects library contract types as
    using-for target types.

- [x] Reject library modifiers referenced through using-for.
  - Upstream: `test/libsolidity/syntaxTests/modifiers/library_via_using.sol`
  - Expected: `function f() L.m public {}` rejects `L.m` even if `using L for *`
    is in scope.
  - Fixed in this branch: modifier resolution now rejects modifiers whose
    defining contract is not the current contract or one of its bases.

- [x] Allow storage string fields to receive string literals.
  - Upstream: `test/libsolidity/semanticTests/errors/using_structs.sol`
  - Expected: `s.b = "abc"` is valid for a storage struct field `string b`.
  - Fixed in this branch: assignment checking accepts values that can be copied
    through the matching memory reference type when the left hand side is a
    storage reference.

## Warning Parity

These are solc warnings rather than using-for semantic failures. They are lower
priority unless we decide to match solc warning coverage broadly.

- [ ] Mutability warnings for functions that can be `pure`.
  - Upstream:
    `test/libsolidity/syntaxTests/nameAndTypeResolution/253_using_for_function_exists.sol`
    and
    `test/libsolidity/syntaxTests/nameAndTypeResolution/254_using_for_function_on_int.sol`
  - Current behavior: no mutability warning is emitted.

- [x] Warning for using `this` in a constructor.
  - Upstream:
    `test/libsolidity/syntaxTests/nameAndTypeResolution/491_using_this_in_constructor.sol`
  - Fixed in this branch: type checking warns when the builtin `this` value is
    used in constructor context.
