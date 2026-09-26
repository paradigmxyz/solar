//@ codegen-matrix: standard portable
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@[mir] filecheck:
//@ run-call: count 0x => 0
//@ run-call: count 0x68656c6c6f => 5
//@ run-call: count 0xc3a9 => 1
//@ run-call: count 0xe697a5e69cace8aa9e => 3
//@ run-call: count 0xf09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1f09f90b1 => 33
//@ run-call: count 0x80 => 1
//@ run-call: count 0x8080 => 1
//@ run-call: count 0x808080 => 2
//@ run-call: count 0xfc616161616161 => 2
//@ run-call: count 0xf8616161616161 => 3
//@ run-call: count 0x61e6 => 2
//@ run-call: count 0x61616161616161616161616161616161616161616161616161616161616161e697a562 => 33
//@ run-call: count 0x616161616161616161616161616161616161616161616161616161616161f09f90b163646364636463646364636463646364636463646364636463646364636463646364636463646364 => 71
//@ run-call: count 0xc3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9 => 16
//@ run-call: count 0xc3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a978 => 17
//@ run-call: count 0x61c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9c3a9 => 17
//@ run-call: count 0xe697a5e697a5e697a5e697a5e697a5e697a5e697a5e697a5e697a5e697a5e697a5 => 11
//@ run-call: sweep 0; gas=16000000 => true
//@ run-call: sweep 1; gas=16000000 => true
//@ run-call: sweep 2; gas=16000000 => true
//@ run-call: sweep 3; gas=16000000 => true
//@ run-call: sweep 5; gas=16000000 => true
//@ run-call: sweep 8; gas=16000000 => true
//@ run-call: sweep 31; gas=16000000 => true
//@ run-call: sweep 32; gas=16000000 => true
//@ run-call: sweep 33; gas=16000000 => true
//@ run-call: sweep 34; gas=16000000 => true
//@ run-call: sweep 35; gas=16000000 => true
//@ run-call: sweep 40; gas=16000000 => true
//@ run-call: sweep 63; gas=16000000 => true
//@ run-call: sweep 64; gas=16000000 => true
//@ run-call: sweep 65; gas=16000000 => true
//@ run-call: sweep 66; gas=16000000 => true
//@ run-call: sweep 97; gas=16000000 => true
//@ run-call: sweep 128; gas=16000000 => true
//@ run-call: sweep 200; gas=16000000 => true

import {Strings} from "solar:core/v1/Strings.sol";

contract Test {
    // The count is lowered in place: a lead's top six bits select a nibble of
    // one constant table of lengths, so nothing is written to memory.
    // CHECK-LABEL: fn @count{{[( ]}}
    // CHECK-NOT: icall
    // CHECK: shr 250,
    function count(bytes memory s) public pure returns (uint256) {
        return Strings.runeCount(string(s));
    }

    // Mostly well-formed text, with stray continuation bytes, five- and
    // six-byte leads and runes cut short mixed in, compared with a direct
    // stepping of the lengths each lead declares.
    function sweep(uint256 length) public pure returns (bool) {
        for (uint256 seed; seed < 12; ++seed) {
            bytes memory s = new bytes(length);
            uint256 x = uint256(keccak256(abi.encode(seed, length)));
            uint256 i;
            while (i < length) {
                x = uint256(keccak256(abi.encode(x)));
                uint256 kind = x % 16;
                uint256 lead;
                uint256 size;
                if (kind < 6) {
                    (lead, size) = (0x20 + (x >> 8) % 0x5f, 1);
                } else if (kind < 9) {
                    (lead, size) = (0xc2 + (x >> 8) % 30, 2);
                } else if (kind < 12) {
                    (lead, size) = (0xe0 + (x >> 8) % 16, 3);
                } else if (kind < 13) {
                    (lead, size) = (0xf0 + (x >> 8) % 5, 4);
                } else if (kind < 14) {
                    (lead, size) = (0x80 + (x >> 8) % 64, 1);
                } else if (kind < 15) {
                    (lead, size) = (0xf8 + (x >> 8) % 8, 1 + (x >> 16) % 6);
                } else {
                    (lead, size) = (0xc2 + (x >> 8) % 50, 1);
                }
                s[i++] = bytes1(uint8(lead));
                for (uint256 k = 1; k < size && i < length; ++k) {
                    s[i++] = bytes1(uint8(0x80 + (x >> (24 + 8 * k)) % 64));
                }
            }
            if (Strings.runeCount(string(s)) != _reference(s)) return false;
        }
        return true;
    }

    function _reference(bytes memory s) private pure returns (uint256 runes) {
        for (uint256 i; i < s.length; ++runes) {
            uint256 c = uint8(s[i]);
            i += c < 0x80 ? 1 : c < 0xe0 ? 2 : c < 0xf0 ? 3 : c < 0xf8 ? 4 : c < 0xfc ? 5 : 6;
        }
    }
}
