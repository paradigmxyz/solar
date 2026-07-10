//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/using/library_functions_inside_contract.sol

library L {
    function externalFunction(uint256) external pure {}
    function publicFunction(uint256) public pure {}
    function internalFunction(uint256) internal pure {}
}

contract C {
    using {L.externalFunction} for uint256;
    using {L.publicFunction} for uint256;
    using {L.internalFunction} for uint256;

    function f() public pure {
        uint256 x;
        x.externalFunction();
        x.publicFunction();
        x.internalFunction();
    }
}
