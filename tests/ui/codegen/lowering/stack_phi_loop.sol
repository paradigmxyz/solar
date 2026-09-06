//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@ run-call: StackPhiLoop::loopCarried 4, true => 46
//@ run-call: StackPhiLoop::loopCarried 4, false => 62
//@ run-call: StackPhiLoop::sequential 3, 2 => 14
//@ run-call: StackPhiLoop::nested 2, 3 => 15

contract StackPhiLoop {
    uint256 private stored;

    // Form step = 7 + (iszero(flag) << 2), then enter the same loop header
    // reached by the checked accumulator update's increment/backedge.
    // CHECK-LABEL: @module StackPhiLoop_runtime
    // CHECK: push 0x50d1f082
    // CHECK: push 0x71b76bb2
    // CHECK-NEXT: eq
    // CHECK-NEXT: jumpi [[CARRIED:bb[0-9]+]], {{bb[0-9]+}}
    // CHECK: push 3
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: mul
    // CHECK: jumpi [[PRODUCT:bb[0-9]+]], [[PANIC:bb[0-9]+]]
    // CHECK-NEXT: [[PANIC]]:
    // CHECK: revert
    // CHECK-NEXT: [[PRODUCT]]:
    // CHECK: jumpi [[PANIC]], [[ACCUMULATE:bb[0-9]+]]
    // CHECK-NEXT: [[ACCUMULATE]]:
    // CHECK-NEXT: dup 3
    // CHECK-NEXT: add
    // CHECK-NEXT: dup 1
    // CHECK-NEXT: swap 2
    // CHECK-NEXT: swap 3
    // CHECK-NEXT: gt
    // CHECK-NEXT: jumpi [[PANIC]], [[INCREMENT:bb[0-9]+]]
    // CHECK-NEXT: [[INCREMENT]]:
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: push 1
    // CHECK-NEXT: add
    // CHECK-NEXT: jump [[HEADER:bb[0-9]+]]
    // CHECK-NEXT: [[HEADER]]:
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: lt
    // CHECK-NEXT: jumpi {{bb[0-9]+}}, [[EXIT:bb[0-9]+]]
    // CHECK-NEXT: [[EXIT]]:
    // CHECK-NEXT: pop
    // CHECK: return
    // CHECK-NEXT: [[STEP:bb[0-9]+]]:
    // CHECK-NEXT: push 36
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: iszero
    // CHECK-NEXT: push 2
    // CHECK-NEXT: shl
    // CHECK-NEXT: push 7
    // CHECK-NEXT: add
    // CHECK-NEXT: push 0
    // CHECK-NEXT: push 0
    // CHECK-NEXT: jump [[HEADER]]
    // CHECK: [[CARRIED]]:
    // CHECK-NEXT: pop
    // CHECK-NEXT: calldatasize
    // CHECK-NEXT: push 68
    // CHECK-NEXT: gt
    // CHECK-NEXT: jumpi [[REJECT:bb[0-9]+]], [[BOOL:bb[0-9]+]]
    // CHECK-NEXT: [[BOOL]]:
    // CHECK-NEXT: push 36
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: push 2
    // CHECK-NEXT: gt
    // CHECK-NEXT: jumpi [[STEP]], [[REJECT]]
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
