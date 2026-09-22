//@ compile-flags: -Ogas -Zdump=mir
//@ normalize-stdout-test: "(?s).+" -> ""
//@ filecheck:
//@ run-call: decode "Zm9vYmFy" => 0x666f6f626172
//@ run-call: decode "Zm9vYg==" => 0x666f6f62
//@ run-call: decode "" => 0x
//@ run-call-fail: decode "Zm9v!mFy" => 0xa164f8fe

// A strict decoder: a character outside the alphabet is an error.
// CHECK-LABEL: fn @decode
// CHECK: 0xa164f8fe
import {Base64} from "solar:core/v1/codecs/Base64.sol";

contract Safe {
    function decode(string memory data) public pure returns (bytes memory) {
        return Base64.decode(data);
    }
}
