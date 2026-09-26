//@ codegen-matrix: standard
//@[gas] compile-flags: -Zdump=mir
//@[gas] filecheck:
//@ run-call: scan 5 => true
//@ run-call: scan 12 => true
//@ run-call: scan 7 => false
//@ run-call: halve 16 => 4
//@ run-call: halve 3 => 1
//@ run-call: callWrap 42 => 0x000000000000000000000000000000000000000000000000000000000000002a

// Single-use helpers that read and write storage, or return memory objects,
// are consumed by the general and single-use passes; loop-containing helpers
// stay calls.
// CHECK: @module EffectfulInline
// CHECK-NOT: fn @tryPop(
// CHECK-NOT: icall @tryPop
// CHECK-NOT: fn @wrap(
// CHECK-NOT: icall @wrap
// CHECK: fn @countDown(
// CHECK: icall @countDown
contract EffectfulInline {
    uint256[4] internal queue;
    uint256 internal head;

    constructor() {
        queue[0] = 5;
        queue[1] = 9;
        queue[2] = 12;
    }

    function tryPop() internal returns (uint256) {
        uint256 value = queue[head];
        queue[head] = 0;
        head++;
        return value;
    }

    function scan(uint256 target) external returns (bool) {
        while (true) {
            uint256 value = tryPop();
            if (value == 0) return false;
            if (value == target) return true;
        }
    }

    function countDown(uint256 n) internal pure returns (uint256) {
        uint256 steps;
        while (n > 1) {
            n = n / 2;
            steps++;
        }
        return steps;
    }

    function halve(uint256 n) external pure returns (uint256) {
        return countDown(n);
    }

    function wrap(uint256 x) internal pure returns (bytes memory) {
        return abi.encode(x);
    }

    function callWrap(uint256 x) external pure returns (bytes memory) {
        return wrap(x);
    }
}