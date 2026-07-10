//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/semanticTests/immutable/multiple_initializations.sol

contract A {
    uint256 immutable x = x + 1;
    uint256 immutable y = x += 2;

    constructor(uint256) m(x += 16) m(x += 32) {
        x += 64;
        x += 128;
    }

    modifier m(uint256) {
        _;
    }

    function get() public returns (uint256) {
        return x;
    }
}
contract B is A(A.x += 8) {
    constructor(uint256) {}
}
contract C is B {
    constructor() B(x += 4) {}
}
