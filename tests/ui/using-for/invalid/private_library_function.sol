//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/using/private_library_function_outside_scope.sol

library L {
    function privateFunction(uint256) private pure {}
}

contract C {
    using {L.privateFunction} for uint256; //~ ERROR: is private and therefore cannot be attached
}

using {L.privateFunction} for uint256; //~ ERROR: is private and therefore cannot be attached
