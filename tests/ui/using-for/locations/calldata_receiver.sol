//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/using/bound_calldata_parameter_accepting_calldata.sol

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
