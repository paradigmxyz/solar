//@ codegen-matrix: standard
//@[gas] compile-flags: -Zdump=evm-ir-runtime
//@[gas] filecheck: --implicit-check-not=mload
//@ run-call: first 3, 4 => 32
//@ run-call: second 3, 4 => 14
//@ run-call: both 3, 4 => 104

// Both result words return on the stack. Callers that read one of them drop the
// other, and none reads the callee's memory return area.
// CHECK-LABEL: @module MultiReturn_runtime
// CHECK: sstore
// CHECK: return
contract MultiReturn {
    uint256 private state;

    function first(uint256 a, uint256 b) external returns (uint256) {
        (uint256 x, ) = split(a, b);
        (uint256 y, ) = split(b, a);
        return x + y;
    }

    function second(uint256 a, uint256 b) external returns (uint256) {
        (, uint256 x) = split(a, b);
        (, uint256 y) = split(b, a);
        return x + y;
    }

    function both(uint256 a, uint256 b) external returns (uint256) {
        (uint256 x, uint256 y) = split(a, b);
        return x * y;
    }

    function split(uint256 a, uint256 b) internal returns (uint256, uint256) {
        unchecked {
            state += a;
            uint256 sum = a * a + b;
            uint256 diff = a + b * 2;
            state ^= sum;
            return (sum, diff - a);
        }
    }
}
