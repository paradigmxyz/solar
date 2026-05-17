//@compile-flags: -Ztypeck

struct S {
    uint256 x;
}

library L {
    function f(S calldata s) internal pure returns (uint256) {
        return s.x;
    }
}

contract C {
    using L for S;

    function run(S calldata s) external pure returns (uint256) {
        return s.f();
    }
}
