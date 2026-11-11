//@ codegen-matrix: standard
//@ run-call-fail: checkedCallee => Panic(17)
//@ run-call: uncheckedCallee => 0

// The callee's arithmetic mode comes from the callee, not from the block that calls it.
contract C {
    struct S {
        uint256 x;
    }

    function checkedMake(uint256 a) internal pure returns (S memory) {
        return S(a + 1);
    }

    function uncheckedMake(uint256 a) internal pure returns (S memory) {
        unchecked {
            return S(a + 1);
        }
    }

    function checkedCallee() external pure returns (uint256) {
        unchecked {
            return checkedMake(type(uint256).max).x;
        }
    }

    function uncheckedCallee() external pure returns (uint256) {
        return uncheckedMake(type(uint256).max).x;
    }
}
