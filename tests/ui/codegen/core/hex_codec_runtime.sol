//@ codegen-matrix: standard
//@ run-call: encode 0x => ""
//@ run-call: encode 0x00 => "00"
//@ run-call: encode 0x00ff => "00ff"
//@ run-call: encode 0x000102030405060708090a0b0c0d0e => "000102030405060708090a0b0c0d0e"
//@ run-call: encode 0x000102030405060708090a0b0c0d0e0f => "000102030405060708090a0b0c0d0e0f"
//@ run-call: encode 0x000102030405060708090a0b0c0d0e0f10 => "000102030405060708090a0b0c0d0e0f10"
//@ run-call: encode 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e => "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e"
//@ run-call: encode 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f => "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
//@ run-call: encode 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20 => "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20"
//@ run-call: encode 0x030a11181f262d343b424950575e656c737a81888f969da4abb2b9c0c7ced5dce3eaf1f8ff060d141b222930373e454c535a61686f767d848b9299a0a7aeb5bcc3cad1d8dfe6edf4fb020910171e252c333a41484f565d646b727980878e959ca3aab1b8 => "030a11181f262d343b424950575e656c737a81888f969da4abb2b9c0c7ced5dce3eaf1f8ff060d141b222930373e454c535a61686f767d848b9299a0a7aeb5bcc3cad1d8dfe6edf4fb020910171e252c333a41484f565d646b727980878e959ca3aab1b8"
//@ run-call: prefixed 0x => "0x"
//@ run-call: prefixed 0x00 => "0x00"
//@ run-call: prefixed 0x00ff => "0x00ff"
//@ run-call: prefixed 0x000102030405060708090a0b0c0d0e => "0x000102030405060708090a0b0c0d0e"
//@ run-call: prefixed 0x000102030405060708090a0b0c0d0e0f => "0x000102030405060708090a0b0c0d0e0f"
//@ run-call: prefixed 0x000102030405060708090a0b0c0d0e0f10 => "0x000102030405060708090a0b0c0d0e0f10"
//@ run-call: prefixed 0x00 => "0x00"
//@ run-call: prefixed 0x000102030405060708090a0b0c0d0e => "0x000102030405060708090a0b0c0d0e"
//@ run-call: prefixed 0x000102030405060708090a0b0c0d0e0f => "0x000102030405060708090a0b0c0d0e0f"
//@ run-call: prefixed 0x000102030405060708090a0b0c0d0e0f10 => "0x000102030405060708090a0b0c0d0e0f10"
//@ run-call: prefixed 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e => "0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e"
//@ run-call: prefixed 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f => "0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
//@ run-call: prefixed 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20 => "0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20"
//@ run-call: decode "" => 0x
//@ run-call: decode "00" => 0x00
//@ run-call: decode "00ff" => 0x00ff
//@ run-call: decode "000102030405060708090a0b0c0d0e" => 0x000102030405060708090a0b0c0d0e
//@ run-call: decode "000102030405060708090a0b0c0d0e0f" => 0x000102030405060708090a0b0c0d0e0f
//@ run-call: decode "000102030405060708090a0b0c0d0e0f10" => 0x000102030405060708090a0b0c0d0e0f10
//@ run-call: decode "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e" => 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e
//@ run-call: decode "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f" => 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f
//@ run-call: decode "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20" => 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20
//@ run-call: decode "030a11181f262d343b424950575e656c737a81888f969da4abb2b9c0c7ced5dce3eaf1f8ff060d141b222930373e454c535a61686f767d848b9299a0a7aeb5bcc3cad1d8dfe6edf4fb020910171e252c333a41484f565d646b727980878e959ca3aab1b8" => 0x030a11181f262d343b424950575e656c737a81888f969da4abb2b9c0c7ced5dce3eaf1f8ff060d141b222930373e454c535a61686f767d848b9299a0a7aeb5bcc3cad1d8dfe6edf4fb020910171e252c333a41484f565d646b727980878e959ca3aab1b8
//@ run-call: decode "0x00FFaB" => 0x00ffab
//@ run-call: decode "0X10" => 0x10
//@ run-call: decode "0x" => 0x
//@ run-call-fail: decode "0" => 0xcbbc48a0
//@ run-call-fail: decode "0x0" => 0xcbbc48a0
//@ run-call-fail: decode "zz" => 0xcbbc48a0
//@ run-call-fail: decode "0xzz" => 0xcbbc48a0
//@ run-call-fail: decode "12 4" => 0xcbbc48a0
//@ run-call-fail: decode "0x12345" => 0xcbbc48a0

// `encode` and `encodePrefixed` convert sixteen bytes to a word of digits,
// with the last partial chunk shifted up and its artificial digits cleared.
// `decode` is source code over `Bytes`: it takes either case and an optional
// prefix and reverts with `InvalidHex()` on anything else or an odd number of
// digits.
import {Hex} from "solar:core/v1/codecs/Hex.sol";

contract Test {
    function encode(bytes memory data) public pure returns (string memory) {
        return Hex.encode(data);
    }

    function prefixed(bytes memory data) public pure returns (string memory) {
        return Hex.encodePrefixed(data);
    }

    function decode(string memory data) public pure returns (bytes memory) {
        return Hex.decode(data);
    }
}
