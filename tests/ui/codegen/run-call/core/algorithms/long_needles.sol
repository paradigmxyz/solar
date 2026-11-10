//@ codegen-matrix: standard portable
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: sweep 16; gas=16000000 => true
//@ run-call: sweep 17; gas=16000000 => true
//@ run-call: sweep 32; gas=16000000 => true
//@ run-call: sweep 33; gas=16000000 => true
//@ run-call: sweep 40; gas=16000000 => true

import {Strings} from "solar:core/Strings.sol";

// Compare non-overlapping results, replacements and splits against a simple
// reference, including prefix fallback, overlapping matches and a trailing match.
contract Test {
    function sweep(uint256 width) public pure returns (bool) {
        for (uint256 variant; variant < 4; ++variant) {
            bytes memory s = new bytes(100);
            bytes memory n = new bytes(width);
            for (uint256 i; i < s.length; ++i) {
                s[i] = bytes1(uint8(97 + (variant == 0 ? 0 : i % 3)));
            }
            for (uint256 i; i < width; ++i) {
                n[i] = s[100 - width + i];
            }
            if (variant == 2) n[width - 1] = "z";
            if (variant == 3) {
                n[0] = "c";
                n[width - 1] = "z";
            }
            string memory subject = string(s);
            string memory needle = string(n);
            uint256[] memory matches = Strings.indicesOf(subject, needle);
            string[] memory parts = Strings.split(subject, needle);
            require(parts.length == matches.length + 1);
            bytes memory expected;
            uint256 count;
            uint256 previous;
            for (uint256 at; at + width <= s.length;) {
                bool equal = true;
                for (uint256 j; j < width; ++j) {
                    if (s[at + j] != n[j]) {
                        equal = false;
                        break;
                    }
                }
                if (!equal) {
                    ++at;
                    continue;
                }
                require(matches[count] == at);
                bytes memory part = new bytes(at - previous);
                for (uint256 j; j < part.length; ++j) {
                    part[j] = s[previous + j];
                }
                require(keccak256(part) == keccak256(bytes(parts[count])));
                expected = abi.encodePacked(expected, part, "XYZ");
                ++count;
                at += width;
                previous = at;
            }
            require(count == matches.length);
            bytes memory tail = new bytes(s.length - previous);
            for (uint256 j; j < tail.length; ++j) {
                tail[j] = s[previous + j];
            }
            require(keccak256(tail) == keccak256(bytes(parts[count])));
            expected = abi.encodePacked(expected, tail);
            require(keccak256(expected) == keccak256(bytes(Strings.replace(subject, needle, "XYZ"))));
        }
        return true;
    }
}
