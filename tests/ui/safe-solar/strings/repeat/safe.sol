//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: repeat "", 0 => ""
//@ run-call: repeat "", 3 => ""
//@ run-call: repeat "a", 0 => ""
//@ run-call: repeat "a", 1 => "a"
//@ run-call: repeat "a", 2 => "aa"
//@ run-call: repeat "a", 3 => "aaa"
//@ run-call: repeat "ab", 3 => "ababab"
//@ run-call: repeat "abc", 5 => "abcabcabcabcabc"
//@ run-call: repeat "x", 32 => "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
//@ run-call: repeat "x", 33 => "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
//@ run-call: repeat "xy", 32 => "xyxyxyxyxyxyxyxyxyxyxyxyxyxyxyxyxyxyxyxyxyxyxyxyxyxyxyxyxyxyxyxy"
//@ run-call: repeat "abcdefghijklmnopqrstuvwxyz01234", 3 => "abcdefghijklmnopqrstuvwxyz01234abcdefghijklmnopqrstuvwxyz01234abcdefghijklmnopqrstuvwxyz01234"
//@ run-call: repeat "abcdefghijklmnopqrstuvwxyz0123456", 2 => "abcdefghijklmnopqrstuvwxyz0123456abcdefghijklmnopqrstuvwxyz0123456"
//@ run-call: repeat "abcdefghijklmnopqrstuvwxyz0123456", 1 => "abcdefghijklmnopqrstuvwxyz0123456"
//@ run-call: repeat "abcdefghijklmnopqrstuvwxyz01234", 1 => "abcdefghijklmnopqrstuvwxyz01234"
//@ run-call-fail: repeat "ab", 57896044618658097711785492504343953926634992332820282019728792003956564819968 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: repeat "a", 18446744073709551616 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041
//@ run-call: repeatTwice "abc", 2 => "abcabc", "abcabc"

import {Strings} from "solar:core/v1/Strings.sol";

contract Safe {
    function repeat(string memory subject, uint256 times) public pure returns (string memory) {
        return Strings.repeat(subject, times);
    }

    // Two results must be distinct objects: writing through one keeps the other.
    function repeatTwice(string memory subject, uint256 times)
        public
        pure
        returns (string memory, string memory)
    {
        bytes memory first = bytes(Strings.repeat(subject, times));
        bytes memory second = bytes(Strings.repeat(subject, times));
        first[0] = second[0];
        return (string(first), string(second));
    }
}
