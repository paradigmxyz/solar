//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/using/function_from_base_contract_qualified_with_super.sol

contract C {
    function contractFunction(uint256) external view {}
}

contract D is C {
    using {super.contractFunction} for uint256; //~ ERROR: `super` is a builtin
}
