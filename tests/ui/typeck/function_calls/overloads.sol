//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/semanticTests/freeFunctions/overloads.sol

function f(uint256) returns (uint256) {
    return 2;
}

function f(string memory) returns (uint256) {
    return 3;
}

contract C {
    function g() public returns (uint256, uint256) {
        return (f(2), f("abc"));
    }
}
