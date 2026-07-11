// ported-from: test/libsolidity/syntaxTests/using/function_name_without_braces_inside_contract_err.sol

function id256(uint256 x) pure returns (uint256) {
    return x;
}

contract C {
    using id256 for uint256; //~ ERROR: expected library
}
