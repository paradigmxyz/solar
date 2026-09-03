//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@ run-call: StackPhiLoop::loopCarried 4, true => 46
//@ run-call: StackPhiLoop::loopCarried 4, false => 62
//@ run-call: StackPhiLoop::sequential 3, 2 => 14
//@ run-call: StackPhiLoop::nested 2, 3 => 15

contract StackPhiLoop {
    uint256 private stored;

    // CHECK-LABEL: @module StackPhiLoop_runtime
    // CHECK: push 0x50d1f082
    // CHECK: push 0x71b76bb2
    // CHECK: eq
    // CHECK: push [[CARRIED:bb[0-9]+]]
    // CHECK: jumpi
    // CHECK: [[CARRIED]]:
    // CHECK: push 7
    // CHECK: [[CARRIED_MERGE:bb[0-9]+]]:
    // CHECK: jump [[CARRIED_HEADER:bb[0-9]+]]
    // CHECK: [[CARRIED_HEADER]]:
    // CHECK: jumpi
    // CHECK: jump [[CARRIED_HEADER]]
    function loopCarried(uint256 n, bool flag) public pure returns (uint256) {
        uint256 step = flag ? 7 : 11;
        uint256 acc = 0;
        for (uint256 i = 0; i < n; i++) {
            acc += i * 3 + step;
        }
        return acc;
    }

    function sequential(uint256 a, uint256 b) public pure returns (uint256) {
        uint256 acc = 0;
        for (uint256 i = 0; i < a; i++) {
            acc += i + 1;
        }
        for (uint256 j = 0; j < b; j++) {
            acc += j * 2 + 3;
        }
        return acc;
    }

    function nested(uint256 outer, uint256 inner) public pure returns (uint256) {
        uint256 acc = 0;
        for (uint256 i = 0; i < outer; i++) {
            for (uint256 j = 0; j < inner; j++) {
                acc += i + j + 1;
            }
        }
        return acc;
    }

    function storeAfterLoop(uint256 a, uint256 b, uint256 iterations) public {
        uint256 result = a;
        for (uint256 i = 0; i < iterations; ++i) {
            result = (result * b + a) / 2;
            result = result % 1_000_000 + 1;
        }
        stored = result;
    }
}
