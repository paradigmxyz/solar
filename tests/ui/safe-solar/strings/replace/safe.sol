//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: replace "banana", "an", "xyz" => "bxyzxyza"
//@ run-call: replace "aaaa", "aa", "x" => "xx"
//@ run-call: replace "abc", "", "-" => "-a-b-c-"
//@ run-call: replace "abc", "abcd", "x" => "abc"
//@ run-call: replace "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "a", "xyz" => "xyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyz"
//@ run-call: replace "xxabcdefghijklmnopqrstuvwxyz0123456789yy", "abcdefghijklmnopqrstuvwxyz0123456789", "Q" => "xxQyy"
//@ run-call: replace "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", "b" => "bAAAAAAAA"
//@ run-call: replace "abcdXXXXabcdefghijklmnopqrstuvwxyz0123456789abcd", "abcdefghijklmnopqrstuvwxyz0123456789", "-" => "abcdXXXX-abcd"
//@ run-call: replace "hello world hello", "hello", "hi" => "hi world hi"
//@ run-call: replace "abababab", "ab", "" => ""
//@ run-call: replace "same", "same", "other" => "other"
//@ run-call: replace "xxxxab", "ab", "Z" => "xxxxZ"
//@ run-call: replace "abcdef", "gh", "Z" => "abcdef"
//@ run-call: replace "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefgABCDEFGHIJKLMNOPQRSTUVWXYZabcdefgtail", "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefg", "<>" => "<><>tail"
//@ run-call: replace "a-b", "-", "0123456789012345678901234567890123456789" => "a0123456789012345678901234567890123456789b"
//@ run-call: replace "", "a", "b" => ""
//@ run-call: replace "a", "a", "" => ""

import {Strings} from "solar:core/v1/Strings.sol";

contract Safe {
    function replace(string memory subject, string memory needle, string memory replacement)
        public
        pure
        returns (string memory)
    {
        return Strings.replace(subject, needle, replacement);
    }
}
