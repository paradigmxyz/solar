//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/immutable/inheritance_ctor_argument.sol

contract B {
    uint256 immutable x;

    constructor(uint256 x_) {
        x = x_;
    }
}
contract C is B {
    uint256 immutable y;

    constructor() B(y = 3) {}
}
