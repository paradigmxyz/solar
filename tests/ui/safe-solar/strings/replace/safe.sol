//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: replace "banana", "an", "xyz" => "bxyzxyza"
//@ run-call: replace "aaaa", "aa", "x" => "xx"
//@ run-call: replace "abc", "", "-" => "-a-b-c-"
//@ run-call: replace "abc", "abcd", "x" => "abc"
//@ run-call: replace "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "a", "xyz" => "xyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyzxyz"

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
