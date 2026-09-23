//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: decimal 0 => "0"
//@ run-call: decimal 12345 => "12345"
//@ run-call: decimal 115792089237316195423570985008687907853269984665640564039457584007913129639935 => "115792089237316195423570985008687907853269984665640564039457584007913129639935"

// A number in decimal: the digits written back to front from the end of one
// fixed region, whose returned header starts right before the first digit.
// CHECK-LABEL: fn @decimal
// CHECK: mstore8
import {Strings} from "solar:core/v1/Strings.sol";

contract Safe {
    function decimal(uint256 value) public pure returns (string memory) {
        return Strings.toString(value);
    }
}
