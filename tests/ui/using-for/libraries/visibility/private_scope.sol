//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/using/private_library_function_inside_scope.sol

library L {
    using {L.privateFunction} for uint256;

    function privateFunction(uint256) private pure {}
}
