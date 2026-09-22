//@ codegen-matrix: standard portable
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@[mir] filecheck:
//@ run-call: encode 0x => ""
//@ run-call: encode 0x66 => "Zg=="
//@ run-call: encode 0x666f => "Zm8="
//@ run-call: encode 0x666f6f => "Zm9v"
//@ run-call: encode 0x666f6f62 => "Zm9vYg=="
//@ run-call: encode 0x666f6f6261 => "Zm9vYmE="
//@ run-call: encode 0x666f6f626172 => "Zm9vYmFy"
//@ run-call: encode 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e => "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xNTk9QUVJTVFVWV1hZWltcXV4="
//@ run-call: encode 0x0b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e83a8cdf2173c6186abd0f51a3f6489aed3f81d42678cb1d6fb20456a8fb4d9fe23486d92b7dc01264b7095badf04294e7398bde2072c51769bc0e50a2f54799ec3e80d32577ca1c6 => "CzBVep/E6Q4zWH2ix+wRNluApcrvFDleg6jN8hc8YYar0PUaP2SJrtP4HUJnjLHW+yBFao+02f4jSG2St9wBJktwlbrfBClOc5i94gcsUXabwOUKL1R5nsPoDTJXfKHG"
//@ run-call: encode 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f606162636465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9fa0a1a2a3a4a5a6a7a8a9aaabacadaeafb0b1b2b3b4b5b6b7b8b9babbbcbdbebfc0c1c2c3c4c5c6c7c8c9cacbcccdcecfd0d1d2d3d4d5d6d7d8d9dadbdcdddedfe0e1e2e3e4e5e6e7e8e9eaebecedeeeff0f1f2f3f4f5f6f7f8f9fafbfcfdfeff => "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xNTk9QUVJTVFVWV1hZWltcXV5fYGFiY2RlZmdoaWprbG1ub3BxcnN0dXZ3eHl6e3x9fn+AgYKDhIWGh4iJiouMjY6PkJGSk5SVlpeYmZqbnJ2en6ChoqOkpaanqKmqq6ytrq+wsbKztLW2t7i5uru8vb6/wMHCw8TFxsfIycrLzM3Oz9DR0tPU1dbX2Nna29zd3t/g4eLj5OXm5+jp6uvs7e7v8PHy8/T19vf4+fr7/P3+/w=="
//@ run-call: encodeUrl 0x => ""
//@ run-call: encodeUrl 0x66 => "Zg"
//@ run-call: encodeUrl 0x666f => "Zm8"
//@ run-call: encodeUrl 0x666f6f => "Zm9v"
//@ run-call: encodeUrl 0x666f6f62 => "Zm9vYg"
//@ run-call: encodeUrl 0x666f6f6261 => "Zm9vYmE"
//@ run-call: encodeUrl 0x666f6f626172 => "Zm9vYmFy"
//@ run-call: encodeUrl 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e => "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0-P0BBQkNERUZHSElKS0xNTk9QUVJTVFVWV1hZWltcXV4"
//@ run-call: decode "" => 0x
//@ run-call: decode "Zg==" => 0x66
//@ run-call: decode "Zm8=" => 0x666f
//@ run-call: decode "Zm9v" => 0x666f6f
//@ run-call: decode "Zm9vYg==" => 0x666f6f62
//@ run-call: decode "Zm9vYmE=" => 0x666f6f6261
//@ run-call: decode "Zm9vYmFy" => 0x666f6f626172
//@ run-call: decode "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xNTk9QUVJTVFVWV1hZWltcXV4=" => 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e
//@ run-call: decode "CzBVep/E6Q4zWH2ix+wRNluApcrvFDleg6jN8hc8YYar0PUaP2SJrtP4HUJnjLHW+yBFao+02f4jSG2St9wBJktwlbrfBClOc5i94gcsUXabwOUKL1R5nsPoDTJXfKHG" => 0x0b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e83a8cdf2173c6186abd0f51a3f6489aed3f81d42678cb1d6fb20456a8fb4d9fe23486d92b7dc01264b7095badf04294e7398bde2072c51769bc0e50a2f54799ec3e80d32577ca1c6
//@ run-call: decode "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xNTk9QUVJTVFVWV1hZWltcXV5fYGFiY2RlZmdoaWprbG1ub3BxcnN0dXZ3eHl6e3x9fn+AgYKDhIWGh4iJiouMjY6PkJGSk5SVlpeYmZqbnJ2en6ChoqOkpaanqKmqq6ytrq+wsbKztLW2t7i5uru8vb6/wMHCw8TFxsfIycrLzM3Oz9DR0tPU1dbX2Nna29zd3t/g4eLj5OXm5+jp6uvs7e7v8PHy8/T19vf4+fr7/P3+/w==" => 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f606162636465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9fa0a1a2a3a4a5a6a7a8a9aaabacadaeafb0b1b2b3b4b5b6b7b8b9babbbcbdbebfc0c1c2c3c4c5c6c7c8c9cacbcccdcecfd0d1d2d3d4d5d6d7d8d9dadbdcdddedfe0e1e2e3e4e5e6e7e8e9eaebecedeeeff0f1f2f3f4f5f6f7f8f9fafbfcfdfeff
//@ run-call: decode "Zg" => 0x66
//@ run-call: decode "Zm8" => 0x666f
//@ run-call: decode "Zm9v" => 0x666f6f
//@ run-call: decode "Zm9vYg" => 0x666f6f62
//@ run-call: decode "Zm9vYmE" => 0x666f6f6261
//@ run-call: decode "Zm9vYmFy" => 0x666f6f626172
//@ run-call: decode "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0-P0BBQkNERUZHSElKS0xNTk9QUVJTVFVWV1hZWltcXV4" => 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e
//@ run-call-fail: decode "Z" => 0xa164f8fe
//@ run-call-fail: decode "Zm9v!" => 0xa164f8fe
//@ run-call: decode "Zm9" => 0x666f
//@ run-call-fail: decode "Zg==Zg==" => 0xa164f8fe
//@ run-call-fail: decode "=" => 0xa164f8fe
//@ run-call-fail: decode "====" => 0xa164f8fe
//@ run-call-fail: decode "Zm 9v" => 0xa164f8fe
//@ run-call-fail: decode "Zg=" => 0xa164f8fe
//@ run-call-fail: decode "Z===" => 0xa164f8fe
//@ run-call-fail: decode "Zm9vZ" => 0xa164f8fe

//@ run-call: decodeImap ",,,," => 0xffffff
//@ run-call-fail: decode ",,,," => 0xa164f8fe
//@ run-call-fail: decode "AA!A" => 0xa164f8fe
//@ run-call-fail: decode "AA!" => 0xa164f8fe
//@ run-call-fail: decode "A!" => 0xa164f8fe

// Both intrinsic lowering and the checked portable body are checked against
// the reference encoder at every length class, including the word-at-a-time
// path from 96 bytes. `decode` is strict: either alphabet, with or without
// padding, and `InvalidBase64()` for a character outside both, padding
// anywhere but the end of a padded input, or a length no encoding has.
import {Base64} from "solar:core/v1/codecs/Base64.sol";

contract Test {
    // CHECK-LABEL: fn @encode.
    // CHECK: memory_object_len memorybytes
    // CHECK: div {{.*}}, 24
    // CHECK: memory_object_store_word memorybytes
    function encode(bytes memory data) public pure returns (string memory) {
        return Base64.encode(data);
    }

    function encodeUrl(bytes memory data) public pure returns (string memory) {
        return Base64.encode(data, true, true);
    }

    function decodeImap(string memory data) public pure returns (bytes memory) {
        return Base64.decode(data, true);
    }

    // CHECK-LABEL: fn @decode.
    // CHECK: memory_object_len memorybytes
    // CHECK: div {{.*}}, 32
    // CHECK: mstore
    function decode(string memory data) public pure returns (bytes memory) {
        return Base64.decode(data);
    }
}
