//@ codegen-matrix: standard portable
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@[gas] run-call: searches 512; gas=8000000 => true
//@[size] run-call: searches 512; gas=12000000 => true
//@[portable] run-call: searches 512; gas=12000000 => true
//@[mir] run-call: searches 128; gas=16000000 => true
//@[gas] run-call: sorting 512; gas=8000000 => true
//@[size] run-call: sorting 512; gas=12000000 => true
//@[portable] run-call: sorting 512; gas=12000000 => true
//@[mir] run-call: sorting 128; gas=16000000 => true
//@[gas] run-call: collisions 512, false; gas=8000000 => true
//@[gas] run-call: collisions 512, true; gas=8000000 => true
//@[size] run-call: collisions 512, false; gas=12000000 => true
//@[portable] run-call: collisions 512, true; gas=12000000 => true
//@[mir] run-call: collisions 128, false; gas=16000000 => true

import {Strings} from "solar:core/Strings.sol";
import {WordArrays} from "solar:core/WordArrays.sol";

// Gas limits leave room for linear matching and n log n sorting, but not the old
// quadratic paths. Building the inputs and checking the results is included.
contract Test {
    function searches(uint256 n) public pure returns (bool) {
        bytes memory subject = new bytes(n);
        bytes memory needle = new bytes(n / 2);
        for (uint256 i; i < n; ++i) {
            subject[i] = "a";
        }
        for (uint256 i; i < needle.length; ++i) {
            needle[i] = "a";
        }
        needle[needle.length - 1] = "b";
        string memory s = string(subject);
        string memory p = string(needle);
        require(Strings.indexOf(s, p, 0) == type(uint256).max);
        require(Strings.lastIndexOf(s, p, type(uint256).max) == type(uint256).max);
        require(Strings.indicesOf(s, p).length == 0);
        require(keccak256(bytes(Strings.replace(s, p, "x"))) == keccak256(subject));
        string[] memory parts = Strings.split(s, p);
        return parts.length == 1 && keccak256(bytes(parts[0])) == keccak256(subject);
    }

    function sorting(uint256 n) public pure returns (bool) {
        uint256[] memory a = new uint256[](n);
        uint256[] memory values = new uint256[](n);
        for (uint256 i; i < n; ++i) {
            a[i] = n - i;
            values[i] = 1;
        }
        WordArrays.sort(a);
        for (uint256 i; i < n; ++i) {
            require(a[i] == i + 1);
        }
        for (uint256 i; i < n; ++i) {
            a[i] = n - i;
        }
        WordArrays.groupSum(a, values);
        for (uint256 i; i < n; ++i) {
            require(a[i] == i + 1 && values[i] == 1);
        }
        return true;
    }

    function collisions(uint256 n, bool duplicate) public pure returns (bool) {
        uint256 capacity = 1;
        while (capacity < n * 2) capacity *= 2;
        uint256[] memory a = new uint256[](n);
        for (uint256 i; i < n; ++i) {
            a[i] = (n - i) * capacity;
        }
        if (duplicate) a[n - 1] = a[0];
        bytes32 before = keccak256(abi.encode(a));
        require(WordArrays.hasDuplicate(a) == duplicate);
        return before == keccak256(abi.encode(a));
    }
}
