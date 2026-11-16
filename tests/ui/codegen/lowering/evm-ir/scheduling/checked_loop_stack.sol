//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@ run-call: CheckedLoopStorage::compute 10
//@ run-call-fail: CheckedLoopStorage::compute 58 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: factorial 0 => 1
//@ run-call: factorial 1 => 1
//@ run-call: factorial 5 => 120
//@ run-call: factorial 10 => 3628800
//@ run-call-fail: factorial 58 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: checkedLatch 1, 4 => 10
//@ run-call: checkedLatch 5, 4 => 0
//@ run-call-fail: checkedLatch 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: diagnostic 2, 5 => 14
//@ run-call-fail: diagnostic 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x760769feffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff

contract CheckedLoopStorage {
    uint256 public result;

    // CHECK-LABEL: @module CheckedLoopStorage_runtime
    // A checked loop keeps its counter and accumulator on the operand stack.
    // The counter starts at two and steps by one, so its increment cannot
    // wrap within any affordable number of iterations: the latch is a plain
    // jump back to the header.
    // CHECK: mul
    // CHECK-NOT: mstore
    // CHECK: div
    // CHECK-NOT: mstore
    // CHECK: jumpi
    // CHECK: add
    // CHECK-NEXT: jump
    function compute(uint256 n) external {
        result = 1;
        for (uint256 i = 2; i <= n; ++i) {
            result *= i;
        }
    }
}

contract CheckedLoopStack {
    function factorial(uint256 n) external pure returns (uint256 acc) {
        acc = 1;
        for (uint256 i = 2; i <= n; ++i) {
            acc *= i;
        }
    }

    function checkedLatch(uint256 start, uint256 end) external pure returns (uint256 acc) {
        for (uint256 i = start; i <= end; ++i) {
            unchecked { acc += i; }
        }
    }

    error Wrapped(uint256 previous, uint256 sum);

    function diagnostic(uint256 start, uint256 end) external pure returns (uint256 acc) {
        uint256 i = start;
        while (i <= end) {
            uint256 next;
            unchecked {
                acc += i;
                next = i + 1;
            }
            if (next == 0) revert Wrapped(i, acc);
            i = next;
        }
    }
}
