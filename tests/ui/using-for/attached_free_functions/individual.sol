//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/semanticTests/using/free_functions_individual.sol

using {zero} for uint256;

contract C {
    using {id} for uint256;

    function f(uint256 z) external pure returns (uint256) {
        return z.id();
    }

    function g(uint256 z) external pure returns (uint256) {
        return z.zero();
    }
}

function id(uint256 x) pure returns (uint256) {
    return x;
}

function zero(uint256) pure returns (uint256) {
    return 0;
}
