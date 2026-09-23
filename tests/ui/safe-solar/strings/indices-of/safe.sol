//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: indicesOf "banana", "an" => [1, 3]
//@ run-call: indicesOf "aaaaa", "aa" => [0, 2]
//@ run-call: indicesOf "abc", "" => [0, 1, 2, 3]
//@ run-call: indicesOf "abc", "abcd" => []
//@ run-call: indicesOf "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "a" => [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32]
//@ run-call: indicesOf "xxabcdefghijklmnopqrstuvwxyz0123456789yyabcdefghijklmnopqrstuvwxyz0123456789", "abcdefghijklmnopqrstuvwxyz0123456789" => [2, 40]
//@ run-call: indicesOf "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA" => [0, 32]
//@ run-call: indicesOf "abcdXXXXabcdefghijklmnopqrstuvwxyz0123456789abcd", "abcdefghijklmnopqrstuvwxyz0123456789" => [8]
//@ run-call: indicesOf "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefgABCDEFGHIJKLMNOPQRSTUVWXYZabcdefgtail", "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefg" => [0, 33]
//@ run-call: indicesOf "same", "same" => [0]
//@ run-call: indicesOf "xxxxab", "ab" => [4]

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
