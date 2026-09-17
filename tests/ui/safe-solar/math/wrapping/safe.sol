//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: sum 1, 2 => 3
//@ run-call: sum 115792089237316195423570985008687907853269984665640564039457584007913129639935, 1 => 0
//@ run-call: difference 0, 1 => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: times 57896044618658097711785492504343953926634992332820282019728792003956564819968, 2 => 0

// Arithmetic that wraps, said by name at the one operation that means it.
// CHECK-LABEL: safe.sol:Safe
// CHECK-NOT: icall @wrapping
import {Math} from "solar:core/v1/Math.sol";

contract Safe {
    function sum(uint256 x, uint256 y) public pure returns (uint256) {
        return Math.wrappingAdd(x, y);
    }

    function difference(uint256 x, uint256 y) public pure returns (uint256) {
        return Math.wrappingSub(x, y);
    }

    function times(uint256 x, uint256 y) public pure returns (uint256) {
        return Math.wrappingMul(x, y);
    }
}
