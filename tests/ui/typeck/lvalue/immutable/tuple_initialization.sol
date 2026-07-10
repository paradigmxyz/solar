//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/immutable/ctor_initialization_tuple.sol

contract C {
    uint256 immutable x;
    uint256 immutable y;

    constructor() {
        (x, y) = f();
    }

    function f() internal pure returns (uint256 x_, uint256 y_) {
        x_ = 3;
        y_ = 4;
    }
}
