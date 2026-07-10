//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/semanticTests/using/library_functions_inside_contract.sol

library L {
    function externalFunction(uint256 a) external pure returns (uint256) { return a; }
    function publicFunction(uint256 b) public pure returns (uint256) { return b * 2; }
    function internalFunction(uint256 c) internal pure returns (uint256) { return c * 3; }
}

contract C {
    using {L.externalFunction} for uint256;
    using {L.publicFunction} for uint256;
    using {L.internalFunction} for uint256;

    function f() public pure returns (uint256) {
        uint256 x = 1;
        return x.externalFunction();
    }

    function g() public pure returns (uint256) {
        uint256 x = 1;
        return x.publicFunction();
    }

    function h() public pure returns (uint256) {
        uint256 x = 1;
        return x.internalFunction();
    }
}
