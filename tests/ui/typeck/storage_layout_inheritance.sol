// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/abstract_contract_inheriting_from_non_abstract.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_already_specified_in_ancestor_contract.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_specified_by_ancestor_contract_multiple_inheritance.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_specified_by_first_ancestor_contract.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_specified_by_last_ancestor_contract.sol

contract AbstractA layout at 0x1234 {}
abstract contract AbstractC is AbstractA { } //~ ERROR: cannot inherit from a contract with a custom storage layout

contract AncestorA layout at 0x1234 {}

contract AncestorB is AncestorA {} //~ ERROR: cannot inherit from a contract with a custom storage layout

contract AncestorC is AncestorB layout at 0xABCD {}

contract MultipleA layout at 1 {}
contract MultipleB is MultipleA layout at 2 {} //~ ERROR: cannot inherit from a contract with a custom storage layout

contract MultipleC1 is MultipleB {} //~ ERROR: cannot inherit from a contract with a custom storage layout
contract MultipleC2 is MultipleA, MultipleB {} //~ ERROR: cannot inherit from a contract with a custom storage layout
//~^ ERROR: cannot inherit from a contract with a custom storage layout
contract MultipleC3 is MultipleB {} //~ ERROR: cannot inherit from a contract with a custom storage layout

contract MultipleD1 is MultipleC1 {}
contract MultipleD2 is MultipleC2 {}
contract MultipleD3 is MultipleC3 {}

contract FirstA layout at 42 {}
contract FirstB is FirstA {} //~ ERROR: cannot inherit from a contract with a custom storage layout
contract FirstC is FirstB {}
contract FirstD is FirstC {}

contract LastA {}
contract LastB is LastA {}
contract LastC is LastB layout at 42 {}
contract LastD is LastC {} //~ ERROR: cannot inherit from a contract with a custom storage layout
