//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: product 3, 5 => 0, 15
//@ run-call: product 340282366920938463463374607431768211456, 340282366920938463463374607431768211456 => 1, 0
//@ run-call: product 115792089237316195423570985008687907853269984665640564039457584007913129639935, 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 115792089237316195423570985008687907853269984665640564039457584007913129639934, 1

// Both words of a product. The results come back as values: no staging in
// memory, no call.
// CHECK-LABEL: fn @product
// CHECK: mulmod
// CHECK-NOT: icall @mul512
import {Math} from "solar:core/v1/Math.sol";

contract Safe {
    function product(uint256 x, uint256 y) public pure returns (uint256 high, uint256 low) {
        return Math.mul512(x, y);
    }
}
