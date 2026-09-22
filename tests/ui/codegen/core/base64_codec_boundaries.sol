//@ codegen-matrix: standard portable paris
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@[paris] compile-flags: -Ogas --evm-version=paris
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@[mir] filecheck:
//@ run-call: roundtrip 0; gas=16000000 => true
//@ run-call: roundtrip 1; gas=16000000 => true
//@ run-call: roundtrip 2; gas=16000000 => true
//@ run-call: roundtrip 3; gas=16000000 => true
//@ run-call: roundtrip 4; gas=16000000 => true
//@ run-call: roundtrip 5; gas=16000000 => true
//@ run-call: roundtrip 6; gas=16000000 => true
//@ run-call: roundtrip 7; gas=16000000 => true
//@ run-call: roundtrip 8; gas=16000000 => true
//@ run-call: roundtrip 9; gas=16000000 => true
//@ run-call: roundtrip 10; gas=16000000 => true
//@ run-call: roundtrip 11; gas=16000000 => true
//@ run-call: roundtrip 12; gas=16000000 => true
//@ run-call: roundtrip 13; gas=16000000 => true
//@ run-call: roundtrip 23; gas=16000000 => true
//@ run-call: roundtrip 24; gas=16000000 => true
//@ run-call: roundtrip 25; gas=16000000 => true
//@ run-call: roundtrip 31; gas=16000000 => true
//@ run-call: roundtrip 32; gas=16000000 => true
//@ run-call: roundtrip 33; gas=16000000 => true
//@ run-call: roundtrip 47; gas=16000000 => true
//@ run-call: roundtrip 48; gas=16000000 => true
//@ run-call: roundtrip 49; gas=16000000 => true
//@ run-call: roundtrip 95; gas=16000000 => true
//@ run-call: roundtrip 96; gas=16000000 => true
//@ run-call: roundtrip 97; gas=16000000 => true
//@ run-call: roundtrip 255; gas=16000000 => true
//@ run-call: roundtrip 256; gas=16000000 => true
//@ run-call: roundtrip 257; gas=16000000 => true
//@ run-call: roundtrip 1024; gas=16000000 => true
//@ run-call-fail: invalidShort 2, 0, 0 => 0xa164f8fe
//@ run-call-fail: invalidShort 2, 0, 61 => 0xa164f8fe
//@ run-call-fail: invalidShort 2, 0, 128 => 0xa164f8fe
//@ run-call-fail: invalidShort 2, 0, 255 => 0xa164f8fe
//@ run-call-fail: invalidShort 2, 1, 0 => 0xa164f8fe
//@ run-call-fail: invalidShort 2, 1, 61 => 0xa164f8fe
//@ run-call-fail: invalidShort 2, 1, 128 => 0xa164f8fe
//@ run-call-fail: invalidShort 2, 1, 255 => 0xa164f8fe
//@ run-call-fail: invalidShort 3, 0, 0 => 0xa164f8fe
//@ run-call-fail: invalidShort 3, 0, 61 => 0xa164f8fe
//@ run-call-fail: invalidShort 3, 0, 128 => 0xa164f8fe
//@ run-call-fail: invalidShort 3, 0, 255 => 0xa164f8fe
//@ run-call-fail: invalidShort 3, 1, 0 => 0xa164f8fe
//@ run-call-fail: invalidShort 3, 1, 61 => 0xa164f8fe
//@ run-call-fail: invalidShort 3, 1, 128 => 0xa164f8fe
//@ run-call-fail: invalidShort 3, 1, 255 => 0xa164f8fe
//@ run-call-fail: invalidShort 3, 2, 0 => 0xa164f8fe
//@ run-call-fail: invalidShort 3, 2, 61 => 0xa164f8fe
//@ run-call-fail: invalidShort 3, 2, 128 => 0xa164f8fe
//@ run-call-fail: invalidShort 3, 2, 255 => 0xa164f8fe
//@ run-call-fail: invalidShort 4, 0, 0 => 0xa164f8fe
//@ run-call-fail: invalidShort 4, 0, 61 => 0xa164f8fe
//@ run-call-fail: invalidShort 4, 0, 128 => 0xa164f8fe
//@ run-call-fail: invalidShort 4, 0, 255 => 0xa164f8fe
//@ run-call-fail: invalidShort 4, 2, 0 => 0xa164f8fe
//@ run-call-fail: invalidShort 4, 2, 61 => 0xa164f8fe
//@ run-call-fail: invalidShort 4, 2, 128 => 0xa164f8fe
//@ run-call-fail: invalidShort 4, 2, 255 => 0xa164f8fe
//@ run-call-fail: invalidShort 4, 3, 0 => 0xa164f8fe
//@ run-call-fail: invalidShort 4, 3, 128 => 0xa164f8fe
//@ run-call-fail: invalidShort 4, 3, 255 => 0xa164f8fe
//@ run-call-fail: invalidShort 6, 0, 0 => 0xa164f8fe
//@ run-call-fail: invalidShort 6, 0, 61 => 0xa164f8fe
//@ run-call-fail: invalidShort 6, 0, 128 => 0xa164f8fe
//@ run-call-fail: invalidShort 6, 0, 255 => 0xa164f8fe
//@ run-call-fail: invalidShort 6, 3, 0 => 0xa164f8fe
//@ run-call-fail: invalidShort 6, 3, 61 => 0xa164f8fe
//@ run-call-fail: invalidShort 6, 3, 128 => 0xa164f8fe
//@ run-call-fail: invalidShort 6, 3, 255 => 0xa164f8fe
//@ run-call-fail: invalidShort 6, 5, 0 => 0xa164f8fe
//@ run-call-fail: invalidShort 6, 5, 61 => 0xa164f8fe
//@ run-call-fail: invalidShort 6, 5, 128 => 0xa164f8fe
//@ run-call-fail: invalidShort 6, 5, 255 => 0xa164f8fe
//@ run-call-fail: invalidShort 7, 0, 0 => 0xa164f8fe
//@ run-call-fail: invalidShort 7, 0, 61 => 0xa164f8fe
//@ run-call-fail: invalidShort 7, 0, 128 => 0xa164f8fe
//@ run-call-fail: invalidShort 7, 0, 255 => 0xa164f8fe
//@ run-call-fail: invalidShort 7, 3, 0 => 0xa164f8fe
//@ run-call-fail: invalidShort 7, 3, 61 => 0xa164f8fe
//@ run-call-fail: invalidShort 7, 3, 128 => 0xa164f8fe
//@ run-call-fail: invalidShort 7, 3, 255 => 0xa164f8fe
//@ run-call-fail: invalidShort 7, 6, 0 => 0xa164f8fe
//@ run-call-fail: invalidShort 7, 6, 61 => 0xa164f8fe
//@ run-call-fail: invalidShort 7, 6, 128 => 0xa164f8fe
//@ run-call-fail: invalidShort 7, 6, 255 => 0xa164f8fe
//@ run-call-fail: invalidShort 8, 0, 0 => 0xa164f8fe
//@ run-call-fail: invalidShort 8, 0, 61 => 0xa164f8fe
//@ run-call-fail: invalidShort 8, 0, 128 => 0xa164f8fe
//@ run-call-fail: invalidShort 8, 0, 255 => 0xa164f8fe
//@ run-call-fail: invalidShort 8, 4, 0 => 0xa164f8fe
//@ run-call-fail: invalidShort 8, 4, 61 => 0xa164f8fe
//@ run-call-fail: invalidShort 8, 4, 128 => 0xa164f8fe
//@ run-call-fail: invalidShort 8, 4, 255 => 0xa164f8fe
//@ run-call-fail: invalidShort 8, 7, 0 => 0xa164f8fe
//@ run-call-fail: invalidShort 8, 7, 128 => 0xa164f8fe
//@ run-call-fail: invalidShort 8, 7, 255 => 0xa164f8fe
//@ run-call-fail: invalidShort 12, 0, 0 => 0xa164f8fe
//@ run-call-fail: invalidShort 12, 0, 61 => 0xa164f8fe
//@ run-call-fail: invalidShort 12, 0, 128 => 0xa164f8fe
//@ run-call-fail: invalidShort 12, 0, 255 => 0xa164f8fe
//@ run-call-fail: invalidShort 12, 6, 0 => 0xa164f8fe
//@ run-call-fail: invalidShort 12, 6, 61 => 0xa164f8fe
//@ run-call-fail: invalidShort 12, 6, 128 => 0xa164f8fe
//@ run-call-fail: invalidShort 12, 6, 255 => 0xa164f8fe
//@ run-call-fail: invalidShort 12, 11, 0 => 0xa164f8fe
//@ run-call-fail: invalidShort 12, 11, 128 => 0xa164f8fe
//@ run-call-fail: invalidShort 12, 11, 255 => 0xa164f8fe
//@ run-call-fail: invalidShort 16, 0, 0 => 0xa164f8fe
//@ run-call-fail: invalidShort 16, 0, 61 => 0xa164f8fe
//@ run-call-fail: invalidShort 16, 0, 128 => 0xa164f8fe
//@ run-call-fail: invalidShort 16, 0, 255 => 0xa164f8fe
//@ run-call-fail: invalidShort 16, 8, 0 => 0xa164f8fe
//@ run-call-fail: invalidShort 16, 8, 61 => 0xa164f8fe
//@ run-call-fail: invalidShort 16, 8, 128 => 0xa164f8fe
//@ run-call-fail: invalidShort 16, 8, 255 => 0xa164f8fe
//@ run-call-fail: invalidShort 16, 15, 0 => 0xa164f8fe
//@ run-call-fail: invalidShort 16, 15, 128 => 0xa164f8fe
//@ run-call-fail: invalidShort 16, 15, 255 => 0xa164f8fe
//@ run-call-fail: invalid 0, 0 => 0xa164f8fe
//@ run-call-fail: invalid 0, 32 => 0xa164f8fe
//@ run-call-fail: invalid 0, 61 => 0xa164f8fe
//@ run-call-fail: invalid 0, 127 => 0xa164f8fe
//@ run-call-fail: invalid 0, 128 => 0xa164f8fe
//@ run-call-fail: invalid 0, 255 => 0xa164f8fe
//@ run-call-fail: invalid 15, 0 => 0xa164f8fe
//@ run-call-fail: invalid 15, 32 => 0xa164f8fe
//@ run-call-fail: invalid 15, 61 => 0xa164f8fe
//@ run-call-fail: invalid 15, 127 => 0xa164f8fe
//@ run-call-fail: invalid 15, 128 => 0xa164f8fe
//@ run-call-fail: invalid 15, 255 => 0xa164f8fe
//@ run-call-fail: invalid 31, 0 => 0xa164f8fe
//@ run-call-fail: invalid 31, 32 => 0xa164f8fe
//@ run-call-fail: invalid 31, 61 => 0xa164f8fe
//@ run-call-fail: invalid 31, 127 => 0xa164f8fe
//@ run-call-fail: invalid 31, 128 => 0xa164f8fe
//@ run-call-fail: invalid 31, 255 => 0xa164f8fe
//@ run-call-fail: invalid 32, 0 => 0xa164f8fe
//@ run-call-fail: invalid 32, 32 => 0xa164f8fe
//@ run-call-fail: invalid 32, 61 => 0xa164f8fe
//@ run-call-fail: invalid 32, 127 => 0xa164f8fe
//@ run-call-fail: invalid 32, 128 => 0xa164f8fe
//@ run-call-fail: invalid 32, 255 => 0xa164f8fe
//@ run-call-fail: invalid 63, 0 => 0xa164f8fe
//@ run-call-fail: invalid 63, 32 => 0xa164f8fe
//@ run-call-fail: invalid 63, 61 => 0xa164f8fe
//@ run-call-fail: invalid 63, 127 => 0xa164f8fe
//@ run-call-fail: invalid 63, 128 => 0xa164f8fe
//@ run-call-fail: invalid 63, 255 => 0xa164f8fe
//@ run-call-fail: invalid 64, 0 => 0xa164f8fe
//@ run-call-fail: invalid 64, 32 => 0xa164f8fe
//@ run-call-fail: invalid 64, 61 => 0xa164f8fe
//@ run-call-fail: invalid 64, 127 => 0xa164f8fe
//@ run-call-fail: invalid 64, 128 => 0xa164f8fe
//@ run-call-fail: invalid 64, 255 => 0xa164f8fe
//@ run-call-fail: invalid 65, 0 => 0xa164f8fe
//@ run-call-fail: invalid 65, 32 => 0xa164f8fe
//@ run-call-fail: invalid 65, 61 => 0xa164f8fe
//@ run-call-fail: invalid 65, 127 => 0xa164f8fe
//@ run-call-fail: invalid 65, 128 => 0xa164f8fe
//@ run-call-fail: invalid 65, 255 => 0xa164f8fe

//@ run-call: classify 0; gas=16000000 => true
//@ run-call: classify 16; gas=16000000 => true
//@ run-call: classify 32; gas=16000000 => true
//@ run-call: classify 48; gas=16000000 => true
//@ run-call: classify 64; gas=16000000 => true
//@ run-call: classify 80; gas=16000000 => true
//@ run-call: classify 96; gas=16000000 => true
//@ run-call: classify 112; gas=16000000 => true
//@ run-call: classify 128; gas=16000000 => true
//@ run-call: classify 144; gas=16000000 => true
//@ run-call: classify 160; gas=16000000 => true
//@ run-call: classify 176; gas=16000000 => true
//@ run-call: classify 192; gas=16000000 => true
//@ run-call: classify 208; gas=16000000 => true
//@ run-call: classify 224; gas=16000000 => true
//@ run-call: classify 240; gas=16000000 => true

//@ run-call: discard "AAAA" => true
//@ run-call-fail: discard "AA!A" => 0xa164f8fe

import {Base64} from "solar:core/v1/codecs/Base64.sol";
import {Arrays} from "solar:core/v1/Arrays.sol";

contract Test {
    // Dirty bytes after the logical end must not affect either tail. Repeated
    // allocations also check the output length word and surrounding objects.
    // CHECK-LABEL: fn @roundtrip(
    // CHECK: icall @core_base64_encode
    // CHECK: icall @core_base64_decode_wide
    function roundtrip(uint256 n) public pure returns (bool) {
        bytes memory input = new bytes(n + 32);
        for (uint256 i; i < input.length; ++i) input[i] = bytes1(uint8((i * 37 + 11) % 256));
        Arrays.truncate(input, n);
        bytes32 original = keccak256(input);
        bytes memory sentinel = new bytes(64);
        for (uint256 i; i < 64; ++i) sentinel[i] = bytes1(uint8(i + 1));
        bytes32 guard = keccak256(sentinel);
        for (uint256 mode; mode < 4; ++mode) {
            string memory encoded = Base64.encode(input, mode % 2 != 0, mode >= 2);
            uint256 expected = ((n + 2) / 3) * 4;
            if (mode >= 2 && n % 3 != 0) expected -= 3 - n % 3;
            require(bytes(encoded).length == expected);
            bytes32 encodedHash = keccak256(bytes(encoded));
            bytes memory decoded = Base64.decode(encoded);
            require(decoded.length == n && keccak256(decoded) == original);
            require(keccak256(bytes(encoded)) == encodedHash);
            require(input.length == n && keccak256(input) == original);
            require(sentinel.length == 64 && keccak256(sentinel) == guard);
        }
        return true;
    }

    // Invalid bytes in the first/last wide word and both tail positions.
    function invalid(uint256 index, uint8 value) public pure returns (bytes memory) {
        bytes memory data = new bytes(66);
        for (uint256 i; i < data.length; ++i) data[i] = bytes1(uint8(65));
        data[index] = bytes1(value);
        return Base64.decode(string(data));
    }
    // Invalid bytes in every group of the single-group and short-loop
    // decoders, including characters above 127 and interior padding.
    function invalidShort(uint256 n, uint256 index, uint8 value) public pure returns (bytes memory) {
        bytes memory data = new bytes(n);
        for (uint256 i; i < data.length; ++i) data[i] = bytes1(uint8(65));
        data[index] = bytes1(value);
        return Base64.decode(string(data));
    }

    // Exhaust every byte value in the single-group, short-loop, and wide
    // decoders. Padding is placed in the interior, so '=' is invalid in this
    // classification.
    function classify(uint256 first) public view returns (bool) {
        for (uint256 n = 4; n <= 32; n += n < 12 ? 8 : 20) {
            bytes memory data = new bytes(n);
            for (uint256 i; i < n; ++i) data[i] = bytes1(uint8(65));
            for (uint256 c = first; c < first + 16; ++c) {
                data[1] = bytes1(uint8(c));
                bool valid = (c >= 65 && c <= 90) || (c >= 97 && c <= 122)
                    || (c >= 48 && c <= 57) || c == 43 || c == 45 || c == 47 || c == 95;
                try this.checkedDecode(string(data)) returns (bytes memory result) {
                    require(valid && result.length == n * 3 / 4);
                } catch (bytes memory reason) {
                    require(!valid && keccak256(reason) == keccak256(hex"a164f8fe"));
                }
            }
        }
        return true;
    }

    function checkedDecode(string memory data) external pure returns (bytes memory) {
        return Base64.decode(data);
    }

    // Validation must remain observable even when the result is unused.
    function discard(string memory data) public pure returns (bool) {
        Base64.decode(data);
        return true;
    }

}
