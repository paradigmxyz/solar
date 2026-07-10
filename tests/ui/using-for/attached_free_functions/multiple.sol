//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/semanticTests/using/free_function_multi.sol

contract C {
    function f(uint256 z) external pure returns (uint256) {
        return z.id();
    }

    using {id, zero, zero, id} for uint256;

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
