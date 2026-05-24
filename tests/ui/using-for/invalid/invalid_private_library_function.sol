//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/using/private_library_function_outside_scope.sol

library L {
    function priv(uint256 x) private pure returns (uint256) {
        return x;
    }
}

contract C {
    using {L.priv} for uint256; //~ ERROR: is private and therefore cannot be attached
}
