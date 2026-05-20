# Using-For Missing Features

This tracks the remaining Solar/solc divergences found while auditing the
focused solc using-for corpus. The goal is to cover the distinct behavior and
diagnostic paths without keeping one UI file per upstream test.

## Missing Semantics

- [ ] Qualified enum access through contract types.
  - Upstream:
    `test/libsolidity/semanticTests/enums/using_contract_enums_with_explicit_contract_name.sol`
    and
    `test/libsolidity/semanticTests/enums/using_inherited_enum_excplicitly.sol`
  - Expected: `C.E.V` and `Base.E.V` resolve when `E` is an enum declared in
    `C` or inherited from `Base`.
  - Current behavior: member lookup on `type(contract C)` does not expose nested
    enum type members, so `C.E` is rejected.

- [ ] Imported free-function aliases in braced using directives.
  - Upstream:
    `test/libsolidity/semanticTests/using/imported_functions.sol` and
    `test/libsolidity/syntaxTests/using/global_and_local.sol`
  - Expected: `using {A.f, importedAlias} for T` attaches both a qualified
    namespace function and an imported alias.
  - Current behavior: qualified namespace functions attach, but the imported
    alias path is not attached as a member.

- [ ] Ambiguity between global and local attached members.
  - Upstream: `test/libsolidity/syntaxTests/using/global_local_clash.sol`
  - Expected: if an imported global using directive and a local using directive
    attach different functions under the same member name, member lookup reports
    the member as ambiguous.
  - Current behavior: duplicate attached member names with different function
    IDs can be hidden instead of reported as ambiguous.

- [ ] Reject library names as using-for target types.
  - Upstream: `test/libsolidity/syntaxTests/using/using_library_for_library.sol`
  - Expected: `using L for M` is invalid when `M` is a library name.
  - Current behavior: the target type lowers to a contract type and is accepted.

- [ ] Reject library modifiers referenced through using-for.
  - Upstream: `test/libsolidity/syntaxTests/modifiers/library_via_using.sol`
  - Expected: `function f() L.m public {}` rejects `L.m` even if `using L for *`
    is in scope.
  - Current behavior: the modifier path is accepted.

- [ ] Allow storage string fields to receive string literals.
  - Upstream: `test/libsolidity/semanticTests/errors/using_structs.sol`
  - Expected: `s.b = "abc"` is valid for a storage struct field `string b`.
  - Current behavior: type checking rejects assigning the string literal to
    `string storage`.

## Warning Parity

These are solc warnings rather than using-for semantic failures. They are lower
priority unless we decide to match solc warning coverage broadly.

- [ ] Mutability warnings for functions that can be `pure`.
  - Upstream:
    `test/libsolidity/syntaxTests/nameAndTypeResolution/253_using_for_function_exists.sol`
    and
    `test/libsolidity/syntaxTests/nameAndTypeResolution/254_using_for_function_on_int.sol`
  - Current behavior: no mutability warning is emitted.

- [ ] Warning for using `this` in a constructor.
  - Upstream:
    `test/libsolidity/syntaxTests/nameAndTypeResolution/491_using_this_in_constructor.sol`
  - Current behavior: no constructor `this` warning is emitted.
