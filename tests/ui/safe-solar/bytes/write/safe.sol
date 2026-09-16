//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: patch 0xaaaaaaaaaaaaaaaa, 0x01020304 => 0xaa01020304aaaaaa
//@ run-call: word 0x1111111111111111111111111111111111111111111111111111111111111111, 3735928559 => 0x00000000000000000000000000000000000000000000000000000000deadbeef
//@ run-call-fail: patch 0xaaaaaaaa, 0x01020304 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// A fixed-width write that leaves every byte outside its range alone, and
// refuses a range that does not fit before writing anything. Four bytes lower
// to a masked read-modify-write; a whole word to one store.
// CHECK-LABEL: fn @patch
// CHECK: mload
// CHECK: and {{.*}}, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffff
// CHECK: mstore
// CHECK-NOT: mstore8
import {Bytes} from "solar:core/v1/Bytes.sol";

contract Safe {
    using Bytes for bytes;

    function patch(bytes memory buffer, bytes4 value) public pure returns (bytes memory) {
        buffer.writeBytes4(1, value);
        return buffer;
    }

    function word(bytes memory buffer, uint256 value) public pure returns (bytes memory) {
        buffer.writeUint256BE(0, value);
        return buffer;
    }
}
