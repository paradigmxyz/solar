//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: product 3, 5 => 0, 15
//@ run-call: product 340282366920938463463374607431768211456, 340282366920938463463374607431768211456 => 1, 0
//@ run-call: product 115792089237316195423570985008687907853269984665640564039457584007913129639935, 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 115792089237316195423570985008687907853269984665640564039457584007913129639934, 1

// The same two words in assembly, as the fixed-point libraries write them.
// CHECK-LABEL: fn @product
// CHECK: mulmod
contract Unsafe {
    function product(uint256 x, uint256 y) public pure returns (uint256 high, uint256 low) {
        assembly ("memory-safe") {
            let folded := mulmod(x, y, not(0))
            low := mul(x, y)
            high := sub(sub(folded, low), lt(folded, low))
        }
    }
}
