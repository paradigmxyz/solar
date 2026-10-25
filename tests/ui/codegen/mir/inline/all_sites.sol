//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Osize -Zdump=mir
//@[dump] filecheck:
//@ run-call: accumulate 4 => 6
//@ run-call: increment 5 => 5
//@ run-call-fail: accumulate 1001

// `checkedAdd` has three call sites, two of them in the loop of `accumulate`. Each call is too
// small to repay the copy alone, but inlining all of them removes the callee and its call
// protocol, and the copy at `i + 1` folds against the loop bound.
contract AllSites {
    uint256 private value;

    // CHECK-LABEL: fn @accumulate()
    // CHECK-NOT: icall @checkedAdd
    function accumulate(uint256 n) public returns (uint256) {
        ensure(n <= 1000);
        uint256 acc = value;
        for (uint256 i = 0; i < n; i = checkedAdd(i, 1)) acc = checkedAdd(acc, i);
        value = acc;
        return acc;
    }

    // CHECK-LABEL: fn @increment()
    // CHECK-NOT: icall @checkedAdd
    function increment(uint256 n) public returns (uint256) {
        value = checkedAdd(value, n);
        return value;
    }

    function ensure(bool ok) internal pure {
        require(ok);
    }

    // CHECK-NOT: fn @checkedAdd
    function checkedAdd(uint256 a, uint256 b) internal pure returns (uint256 c) {
        unchecked {
            c = a + b;
        }
        if (c < a) panicArithmetic();
    }

    function panicArithmetic() internal pure {
        assembly {
            mstore(0, shl(224, 0x4e487b71))
            mstore(4, 0x11)
            revert(0, 36)
        }
    }
}
