//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: indicesOf "banana", "an" => [1, 3]
//@ run-call: indicesOf "aaaaa", "aa" => [0, 2]
//@ run-call: indicesOf "abc", "" => [0, 1, 2, 3]
//@ run-call: indicesOf "abc", "abcd" => []
//@ run-call: indicesOf "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "a" => [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32]

import {Strings} from "solar:core/v1/Strings.sol";

contract Safe {
    function indicesOf(string memory subject, string memory needle)
        public
        pure
        returns (uint256[] memory)
    {
        return Strings.indicesOf(subject, needle);
    }
}
