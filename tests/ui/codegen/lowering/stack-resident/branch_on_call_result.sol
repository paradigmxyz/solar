//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck: --implicit-check-not=mload
//@[ir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: run 3, 5 => 12
//@ run-call: run 4, 6 => 12
//@ run-call: run 4, 5 => 60
//@ run-call: run 0, 0 => 0
//@ run-call: run 7, 9 => 19

// A branch on the first result of a call whose successors join on its second
// result. That result exists only once the call returns, so the join's inputs
// are copied after the call rather than before it. Both results stay on the
// stack; none is read back from memory.
// CHECK-LABEL: @module BranchOnCallResult_runtime
// CHECK: sstore
contract BranchOnCallResult {
    uint256 private state;

    function run(uint256 a, uint256 b) external returns (uint256) {
        (bool found, uint256 value) = search(a);
        if (!found) {
            (bool again, uint256 other) = search(b);
            if (again) return other;
            value = other + 1;
        }
        return value * 3;
    }

    function search(uint256 x) internal returns (bool, uint256) {
        state += x;
        if (x % 3 == 0) return (true, x / 3 + state);
        return (false, x * 2 + state);
    }
}
