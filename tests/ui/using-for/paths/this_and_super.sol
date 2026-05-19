// Solc tests:
// - test/libsolidity/syntaxTests/using/external_function_qualified_with_this.sol.
// - test/libsolidity/syntaxTests/using/function_from_base_contract_qualified_with_super.sol.

//@compile-flags: -Ztypeck

contract C {
    using {this.contractFunction} for uint256; //~ ERROR: `this` is a builtin

    function contractFunction(uint256) external view {}
}

contract D is C {
    using {super.contractFunction} for uint256; //~ ERROR: `super` is a builtin
}
