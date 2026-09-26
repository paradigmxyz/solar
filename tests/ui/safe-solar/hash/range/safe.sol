//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: range 0x0102030405060708, 0, 8 => 0xfec062278915ba5c3c3af6ebf470b5afc94fedadf39fe78eea427b9aa5df9692
//@ run-call: range 0x0102030405060708, 2, 3 => 0xfe60c754eeb6f4271f086228744a2bb133832435a98f1d79b65583db7d2e406b
//@ run-call: range 0x0102030405060708, 8, 0 => 0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470
//@ run-call-fail: range 0x0102, 1, 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// Hashing a range of a buffer where it lies, with the range checked first.
// CHECK-LABEL: fn @range
// CHECK: keccak256 {{v[0-9]+}}, arg2
// CHECK-NOT: mstore8
import {Hash} from "solar:core/v1/Hash.sol";

contract Safe {
    function range(bytes memory b, uint256 offset, uint256 count) public pure returns (bytes32) {
        return Hash.keccak256Range(b, offset, count);
    }
}
