//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: duplicateUint [1, 2, 1] => true
//@ run-call: duplicateUint [1, 2, 3] => false
//@ run-call: duplicateInt [-1, 2, -1] => true
//@ run-call: duplicateInt [-1, 2, 3] => false
//@ run-call: duplicateAddress [0x0000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000002, 0x0000000000000000000000000000000000000001] => true
//@ run-call: duplicateAddress [0x0000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000002, 0x0000000000000000000000000000000000000003] => false
//@ run-call: duplicateBytes32 [0x0000000000000000000000000000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000000000000000000000000000002, 0x0000000000000000000000000000000000000000000000000000000000000001] => true
//@ run-call: duplicateBytes32 [] => false

import {WordArrays} from "solar:core/v1/WordArrays.sol";

contract Safe {
    function duplicateUint(uint256[] memory a) public pure returns (bool) {
        return WordArrays.hasDuplicate(a);
    }

    function duplicateInt(int256[] memory a) public pure returns (bool) {
        return WordArrays.hasDuplicate(a);
    }

    function duplicateAddress(address[] memory a) public pure returns (bool) {
        return WordArrays.hasDuplicate(a);
    }

    function duplicateBytes32(bytes32[] memory a) public pure returns (bool) {
        return WordArrays.hasDuplicate(a);
    }
}
