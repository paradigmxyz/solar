//@ revisions: intrinsic size portable
//@[intrinsic] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: copyWords [] => []
//@ run-call: copyWords [7] => [7]
//@ run-call: copyWords [3, 1, 2, 115792089237316195423570985008687907853269984665640564039457584007913129639935] => [3, 1, 2, 115792089237316195423570985008687907853269984665640564039457584007913129639935]
//@ run-call: copySigned [] => []
//@ run-call: copySigned [-1, 0, 5] => [-1, 0, 5]
//@ run-call: copyAddresses [] => []
//@ run-call: copyAddresses [0xe5bf710f3bca8b868f98c20256568ba477e7425f, 0x0000000000000000000000000000000000000001] => [0xe5bf710f3bca8b868f98c20256568ba477e7425f, 0x0000000000000000000000000000000000000001]
//@ run-call: copyBytes32 [0xff00000000000000000000000000000000000000000000000000000000000001] => [0xff00000000000000000000000000000000000000000000000000000000000001]
//@ run-call: copyIsIndependent [1, 2, 3] => [9, 2, 3], [1, 2, 3]
//@ run-call: copyIsIndependent [] => [], []
//@ run-call-fail: copyOverlong() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041

import {WordArrays} from "solar:core/v1/WordArrays.sol";

contract Safe {
    function copyWords(uint256[] memory a) public pure returns (uint256[] memory) {
        return WordArrays.copy(a);
    }

    function copySigned(int256[] memory a) public pure returns (int256[] memory) {
        return WordArrays.copy(a);
    }

    function copyAddresses(address[] memory a) public pure returns (address[] memory) {
        return WordArrays.copy(a);
    }

    function copyBytes32(bytes32[] memory a) public pure returns (bytes32[] memory) {
        return WordArrays.copy(a);
    }

    // Writing through the copy leaves the original unchanged.
    function copyIsIndependent(uint256[] memory a)
        public
        pure
        returns (uint256[] memory, uint256[] memory)
    {
        uint256[] memory c = WordArrays.copy(a);
        if (c.length != 0) c[0] = 9;
        return (c, a);
    }

    // A length the body's `new` rejects fails the same way, with Panic(0x41).
    function copyOverlong() public pure returns (uint256[] memory) {
        uint256[] memory a;
        assembly ("memory-safe") {
            a := mload(0x40)
            mstore(a, shl(64, 1))
            mstore(0x40, add(a, 0x20))
        }
        return WordArrays.copy(a);
    }
}
