//@ revisions: intrinsic size portable
//@[intrinsic] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: uniqueUint [1, 1, 2, 2, 3] => [1, 2, 3]
//@ run-call: uniqueUint [1, 1, 2, 1] => [1, 2, 1]
//@ run-call: uniqueInt [-2, -2, 0, 3, 3] => [-2, 0, 3]
//@ run-call: uniqueAddress [0x0000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000002] => [0x0000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000002]
//@ run-call: uniqueBytes32 [0x0000000000000000000000000000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000000000000000000000000000001] => [0x0000000000000000000000000000000000000000000000000000000000000001]
//@ run-call: uniqueBytes32 [] => []

import {WordArrays} from "solar:core/v1/WordArrays.sol";

contract Safe {
    function uniqueUint(uint256[] memory a) public pure returns (uint256[] memory) {
        WordArrays.uniquifySorted(a);
        return a;
    }

    function uniqueInt(int256[] memory a) public pure returns (int256[] memory) {
        WordArrays.uniquifySorted(a);
        return a;
    }

    function uniqueAddress(address[] memory a) public pure returns (address[] memory) {
        WordArrays.uniquifySorted(a);
        return a;
    }

    function uniqueBytes32(bytes32[] memory a) public pure returns (bytes32[] memory) {
        WordArrays.uniquifySorted(a);
        return a;
    }
}
