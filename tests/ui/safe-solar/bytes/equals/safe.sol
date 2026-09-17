//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: same 0x05121f2c394653606d7a8794a1aebbc8d5e2effc091623303d4a5764717e8b98a5b2bfccd9e6f300, 0x05121f2c394653606d7a8794a1aebbc8d5e2effc091623303d4a5764717e8b98a5b2bfccd9e6f300 => true
//@ run-call: same 0x05121f2c394653606d7a8794a1aebbc8d5e2effc091623303d4a5764717e8b98a5b2bfccd9e6f300, 0x05121f2c394653606d7a8794a1aebbc8d5e2effc091623303d4a5764717e8b98a5b2bfccd9e6f380 => false
//@ run-call: same 0x05121f2c394653606d7a8794a1aebbc8d5e2effc091623303d4a5764717e8b98a5b2bfccd9e6f300, 0x05121f2c394653606d7a8794a1aebbc8d5e2effc091623303d4a5764717e8b98a5b2bfccd9e6f3 => false
//@ run-call: same 0x, 0x => true

// Comparing the bytes themselves, a word at a time.
// CHECK-LABEL: fn @same
// CHECK: mload
// CHECK-NOT: keccak256
import {Bytes} from "solar:core/v1/Bytes.sol";

contract Safe {
    function same(bytes memory a, bytes memory b) public pure returns (bool) {
        return Bytes.equals(a, b);
    }
}
