//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@ run-call: latchRevert 1000, 5 => 10
//@ run-call-fail: latchRevert 5, 10 => 0xa16662a700000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: latchRevert 2, 3 => 0xa16662a700000000000000000000000000000000000000000000000000000000000000030000000000000000000000000000000000000000000000000000000000000002

contract ColdPhiInputs {
    error W(uint256 a, uint256 b);

    // The revert is cold, so the gas build weighs it at zero: the exit that
    // reaches it carries no phi input of its own, and a cold block pushes the
    // second payload word before it joins the revert.
    // CHECK-LABEL: @module ColdPhiInputs_runtime
    // CHECK: gt
    // CHECK-NEXT: push [[EDGE:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK: [[EDGE]] [cold]:
    // CHECK-NEXT: push 2
    function latchRevert(uint256 a, uint256 n) external pure returns (uint256 s) {
        uint256 i = 0;
        uint256 x;
        uint256 y;
        while (true) {
            unchecked {
                s += i;
                ++i;
            }
            if (i < n) {
                if (s <= a) continue;
                x = s;
                y = 1;
            } else {
                if (s <= a) return s;
                x = i;
                y = 2;
            }
            revert W(x, y);
        }
    }
}
