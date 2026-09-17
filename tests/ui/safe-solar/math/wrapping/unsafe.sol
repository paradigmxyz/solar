//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: sum 1, 2 => 3
//@ run-call: sum 115792089237316195423570985008687907853269984665640564039457584007913129639935, 1 => 0
//@ run-call: difference 0, 1 => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: times 57896044618658097711785492504343953926634992332820282019728792003956564819968, 2 => 0

// The same operations in assembly. Nothing here says the wrapping is meant.
// CHECK-LABEL: unsafe.sol:Unsafe
// CHECK-NOT: icall
contract Unsafe {
    function sum(uint256 x, uint256 y) public pure returns (uint256 z) {
        assembly ("memory-safe") {
            z := add(x, y)
        }
    }

    function difference(uint256 x, uint256 y) public pure returns (uint256 z) {
        assembly ("memory-safe") {
            z := sub(x, y)
        }
    }

    function times(uint256 x, uint256 y) public pure returns (uint256 z) {
        assembly ("memory-safe") {
            z := mul(x, y)
        }
    }
}
