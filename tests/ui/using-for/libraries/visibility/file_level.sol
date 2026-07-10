//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/using/library_functions_at_file_level.sol

library L {
    function externalFunction(uint256) external pure {}
    function publicFunction(uint256) public pure {}
    function internalFunction(uint256) internal pure {}
}

using {L.externalFunction} for uint256;
using {L.publicFunction} for uint256;
using {L.internalFunction} for uint256;

contract C {
    function f() public pure {
        uint256 x;
        x.externalFunction();
        x.publicFunction();
        x.internalFunction();
    }
}
