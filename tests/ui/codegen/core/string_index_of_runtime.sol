//@ codegen-matrix: standard portable
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@[mir] filecheck:
//@ run-call: first "hello world", "o", 0 => 4
//@ run-call: first "hello world", "o", 5 => 7
//@ run-call: first "hello world", "o", 8 => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: first "hello world", "world", 0 => 6
//@ run-call: first "hello world", "", 3 => 3
//@ run-call: first "hello world", "", 20 => 11
//@ run-call: first "hello", "hello world", 0 => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: first "hello world", "d", 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: first "", "", 0 => 0
//@ run-call: last "hello world", "o", 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 7
//@ run-call: last "hello world", "o", 6 => 4
//@ run-call: last "hello world", "o", 3 => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: last "hello world", "h", 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 0
//@ run-call: last "hello world", "", 3 => 3
//@ run-call: last "hello world", "", 20 => 11
//@ run-call: last "hello", "hello world", 20 => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: first "abcdefghijklmnopqrstuvwxyz0123456789-abcdefghijklmnopqrstuvwxyz0123456789", "abcdefghijklmnopqrstuvwxyz0123456789", 1 => 37
//@ run-call: last "abcdefghijklmnopqrstuvwxyz0123456789-abcdefghijklmnopqrstuvwxyz0123456789", "abcdefghijklmnopqrstuvwxyz0123456789", 36 => 0
//@ run-call: first "abcdefghijklmnopqrstuvwxyz0123456789-abcdefghijklmnopqrstuvwxyz0123456789", "abcdefghijklmnopqrstuvwxyz012345678X", 0 => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: first "ahovckryfmubipwelszgovcjqyfmtaipwdkszgnucjqxemtahowdkrygnubiqxelsahovckryfmubipwelszgovcjqyfmtaipwdkszgnucjqxemtahowdkrygnubiqxelsahovckryfm", "ipwdkszgnucjqxemtahowdkrygnubiqxelsahovckryfmubipwelszgovcjqyfmta", 0 => 30
//@ run-call: last "ahovckryfmubipwelszgovcjqyfmtaipwdkszgnucjqxemtahowdkrygnubiqxelsahovckryfmubipwelszgovcjqyfmtaipwdkszgnucjqxemtahowdkrygnubiqxelsahovckryfm", "ipwdkszgnucjqxemtahowdkrygnubiqxelsahovckryfmubipwelszgovcjqyfmta", 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 30
//@ run-call: first "ahovckryfmubipwelszgovcjqyfmtaipwdkszgnucjqxemtahowdkrygnubiqxelsahovckryfmubipwelszgovcjqyfmtaipwdkszgnucjqxemtahowdkrygnubiqxelsahovckryfm", "ovcjqyfmtaipwdkszgnucjqxemtahowdkrygnubiqxelsahovckryfmubipwelszgovcjqyfmtaipwdkszgnucjqxemtahow", 0 => 20
//@ run-call: last "ahovckryfmubipwelszgovcjqyfmtaipwdkszgnucjqxemtahowdkrygnubiqxelsahovckryfmubipwelszgovcjqyfmtaipwdkszgnucjqxemtahowdkrygnubiqxelsahovckryfm", "ovcjqyfmtaipwdkszgnucjqxemtahowdkrygnubiqxelsahovckryfmubipwelszgovcjqyfmtaipwdkszgnucjqxemtahow", 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 20
//@ run-call: first "ahovckryfmubipwelszgovcjqyfmtaipwdkszgnucjqxemtahowdkrygnubiqxelsahovckryfmubipwelszgovcjqyfmtaipwdkszgnucjqxemtahowdkrygnubiqxelsahovckryfm", "ipwdkszgnucjqxemtahowdkrygnubiqxelsahovc#ryfmubipwelszgovcjqyfmta", 0 => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: last "ahovckryfmubipwelszgovcjqyfmtaipwdkszgnucjqxemtahowdkrygnubiqxelsahovckryfmubipwelszgovcjqyfmtaipwdkszgnucjqxemtahowdkrygnubiqxelsahovckryfm", "ovcjqyfmtaipwdkszgnucjqxemtahowdkrygnubiqxelsahovc#ryfmubipwelszgovcjqyfmtaipwdkszgnucjqxemtahow", 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: sweep 0; gas=16000000 => true
//@ run-call: sweep 1; gas=16000000 => true
//@ run-call: sweep 2; gas=16000000 => true
//@ run-call: sweep 3; gas=16000000 => true
//@ run-call: sweep 5; gas=16000000 => true
//@ run-call: sweep 8; gas=16000000 => true
//@ run-call: sweep 31; gas=16000000 => true
//@ run-call: sweep 32; gas=16000000 => true
//@ run-call: sweep 33; gas=16000000 => true
//@ run-call: sweep 40; gas=16000000 => true
//@ run-call: sweep 63; gas=16000000 => true
//@ run-call: sweep 64; gas=16000000 => true
//@ run-call: sweep 65; gas=16000000 => true
//@ run-call: sweep 70; gas=16000000 => true

// Both searches against a checked reference at subject lengths around word
// boundaries, needles up to 40 bytes including ones of at least a word, and
// `from` at both ends, in the middle, and past the subject. Needles are cut from the
// subject, so most searches find something, and mutated so some do not.
import {Strings} from "solar:core/v1/Strings.sol";

contract Test {
    // Each direction is one shared helper.
    // CHECK-LABEL: fn @first(
    // CHECK: icall @core_string_index_of,
    // CHECK-LABEL: fn @last(
    // CHECK: icall @core_string_last_index_of,
    function first(string memory s, string memory n, uint256 from) public pure returns (uint256) {
        return Strings.indexOf(s, n, from);
    }

    function last(string memory s, string memory n, uint256 from) public pure returns (uint256) {
        return Strings.lastIndexOf(s, n, from);
    }

    function sweep(uint256 length) public pure returns (bool) {
        {
            bytes memory s = new bytes(length);
            for (uint256 i; i < length; ++i) {
                s[i] = bytes1(uint8(97 + (i * 7 + i / 5) % 3));
            }
            for (uint256 needleLength; needleLength <= 40; needleLength += 5) {
                if (needleLength > length + 1) break;
                for (uint256 cut; cut < 3; ++cut) {
                    bytes memory n = new bytes(needleLength);
                    uint256 offset = length > needleLength ? (length - needleLength) * cut / 2 : 0;
                    for (uint256 i; i < needleLength; ++i) {
                        n[i] = offset + i < length ? s[offset + i] : bytes1("z");
                    }
                    if (cut == 2 && needleLength != 0) n[needleLength - 1] = "b";
                    uint256[7] memory froms =
                        [uint256(0), 1, length / 2, length - (length == 0 ? 0 : 1), length, length + 1, type(uint256).max];
                    for (uint256 f; f < 7; ++f) {
                        string memory subject = string(s);
                        string memory needle = string(n);
                        if (Strings.indexOf(subject, needle, froms[f]) != _first(s, n, froms[f])) return false;
                        if (Strings.lastIndexOf(subject, needle, froms[f]) != _last(s, n, froms[f])) return false;
                    }
                }
            }
        }
        return true;
    }

    function _matches(bytes memory s, bytes memory n, uint256 at) private pure returns (bool) {
        for (uint256 i; i < n.length; ++i) {
            if (s[at + i] != n[i]) return false;
        }
        return true;
    }

    function _first(bytes memory s, bytes memory n, uint256 from) private pure returns (uint256) {
        if (n.length == 0) return from > s.length ? s.length : from;
        if (n.length > s.length) return type(uint256).max;
        for (uint256 i = from; i <= s.length - n.length; ++i) {
            if (_matches(s, n, i)) return i;
        }
        return type(uint256).max;
    }

    function _last(bytes memory s, bytes memory n, uint256 from) private pure returns (uint256) {
        if (n.length > s.length) return type(uint256).max;
        uint256 limit = s.length - n.length;
        if (from > limit) from = limit;
        for (uint256 i = from + 1; i != 0;) {
            --i;
            if (_matches(s, n, i)) return i;
        }
        return type(uint256).max;
    }
}
