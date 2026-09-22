//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: split "a,b,,c", "," => ["a", "b", "", "c"]
//@ run-call: split "banana", "an" => ["b", "", "a"]
//@ run-call: split "abc", "" => ["a", "b", "c"]
//@ run-call: split "", "," => [""]
//@ run-call: split "abc", "abcd" => ["abc"]

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
