//@ revisions: intrinsic size portable
//@[intrinsic] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: split "a,b,,c", "," => ["a", "b", "", "c"]
//@ run-call: split "banana", "an" => ["b", "", "a"]
//@ run-call: split "abc", "" => ["a", "b", "c"]
//@ run-call: split "", "," => [""]
//@ run-call: split "abc", "abcd" => ["abc"]
//@ run-call: split "xxabcdefghijklmnopqrstuvwxyz0123456789yyabcdefghijklmnopqrstuvwxyz0123456789", "abcdefghijklmnopqrstuvwxyz0123456789" => ["xx", "yy", ""]
//@ run-call: split "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA" => ["", "", "AAAAAA"]
//@ run-call: split "abcdXXXXabcdefghijklmnopqrstuvwxyz0123456789abcd", "abcdefghijklmnopqrstuvwxyz0123456789" => ["abcdXXXX", "abcd"]
//@ run-call: split "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefgABCDEFGHIJKLMNOPQRSTUVWXYZabcdefgtail", "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefg" => ["", "", "tail"]
//@ run-call: split "same", "same" => ["", ""]
//@ run-call: split "", "" => []
//@ run-call: split "a", "" => ["a"]

import {Strings} from "solar:core/v1/Strings.sol";

contract Safe {
    function split(string memory subject, string memory delimiter)
        public
        pure
        returns (string[] memory)
    {
        return Strings.split(subject, delimiter);
    }
}
