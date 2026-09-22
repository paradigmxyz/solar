//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: sortUint [3, 1, 2] => [1, 2, 3]
//@ run-call: sortInt [2, -3, 1] => [-3, 1, 2]
//@ run-call: sortAddress [0x0000000000000000000000000000000000000003, 0x0000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000002] => [0x0000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000002, 0x0000000000000000000000000000000000000003]
//@ run-call: sortGenerated 129, 7; gas=15000000 => true
//@ run-call: sortSignedGenerated 129, 11; gas=15000000 => true
//@ run-call: sortAddressGenerated 65, 13; gas=15000000 => true
//@ run-call: sortDuplicateHeavy 257; gas=15000000 => true

import {WordArrays} from "solar:core/v1/WordArrays.sol";

contract Safe {
    function sortUint(uint256[] memory a) public pure returns (uint256[] memory) {
        WordArrays.sort(a);
        return a;
    }

    function sortInt(int256[] memory a) public pure returns (int256[] memory) {
        WordArrays.sort(a);
        return a;
    }

    function sortAddress(address[] memory a) public pure returns (address[] memory) {
        WordArrays.sort(a);
        return a;
    }

    function sortGenerated(uint256 n, uint256 seed) public pure returns (bool) {
        uint256[] memory a = new uint256[](n);
        for (uint256 i; i < n; ++i) a[i] = uint256(keccak256(abi.encode(seed, i)));
        WordArrays.sort(a);
        for (uint256 i = 1; i < n; ++i) if (a[i - 1] > a[i]) return false;
        return true;
    }

    function sortSignedGenerated(uint256 n, uint256 seed) public pure returns (bool) {
        int256[] memory a = new int256[](n);
        for (uint256 i; i < n; ++i) {
            a[i] = int256(uint256(keccak256(abi.encode(seed, i))) % 1_000_000) - 500_000;
        }
        WordArrays.sort(a);
        for (uint256 i = 1; i < n; ++i) if (a[i - 1] > a[i]) return false;
        return true;
    }

    function sortAddressGenerated(uint256 n, uint256 seed) public pure returns (bool) {
        address[] memory a = new address[](n);
        for (uint256 i; i < n; ++i) {
            a[i] = address(uint160(uint256(keccak256(abi.encode(seed, i)))));
        }
        WordArrays.sort(a);
        for (uint256 i = 1; i < n; ++i) if (a[i - 1] > a[i]) return false;
        return true;
    }

    function sortDuplicateHeavy(uint256 n) public pure returns (bool) {
        bytes32[] memory a = new bytes32[](n);
        for (uint256 i; i < n; ++i) a[i] = bytes32(i % 7);
        WordArrays.sort(a);
        for (uint256 i = 1; i < n; ++i) if (a[i - 1] > a[i]) return false;
        return true;
    }
}
