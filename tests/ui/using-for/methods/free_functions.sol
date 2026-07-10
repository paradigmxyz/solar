//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/using/using_free_functions.sol

function id(uint256 x) pure returns (uint256) {
    return x;
}

function zero(uint256) pure returns (uint256) {
    return 0;
}

using {id} for uint256;

contract C {
    using {zero} for uint256;

    function g(uint256 z) external pure {
        z.zero();
        z.id();
    }
}
