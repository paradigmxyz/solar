//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: decode "00ff10" => 0x00ff10
//@ run-call: decode "DEADbeef" => 0xdeadbeef
//@ run-call-fail: decode "zz" => 0xcbbc48a0
//@ run-call-fail: decode "abc" => 0xcbbc48a0

// A strict decoder: a character that is not a digit, or an odd number of
// them, is an error.
// CHECK-LABEL: fn @decode
// CHECK: 0xcbbc48a0
import {Hex} from "solar:core/v1/codecs/Hex.sol";

contract Safe {
    function decode(string memory data) public pure returns (bytes memory) {
        return Hex.decode(data);
    }
}
