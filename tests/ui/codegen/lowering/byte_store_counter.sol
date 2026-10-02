//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@ run-call: encode 0x => 0x
//@ run-call: encode 0x0123ab => 0x303132336162
//@ run-call: encodeTwice 0xab => 0x36313632
//@ run-call: scan 0x => true
//@ run-call: scan 0x616263 => true
//@ run-call: scan 0x807f => false
//@ run-call: scan 0x7f80 => false
//@ run-call: scan 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20 => true
//@ run-call: twice 0x807f, 0x616263 => false, true
//@ run-call: twice 0x616263, 0x7f80 => true, false
//@ run-call: firstHigh 0x010280 => 2
//@ run-call: firstHigh 0x010203 => 3
//@ run-call: checkedScan 255 => true
//@ run-call-fail: checkedScan 256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

contract ByteStoreCounter {
    bytes16 private constant HEX = "0123456789abcdef";

    function encode(bytes memory input) public pure returns (bytes memory) {
        return encodeImpl(input);
    }

    function encodeTwice(bytes memory input) public pure returns (bytes memory) {
        return encodeImpl(encodeImpl(input));
    }

    // Two checked output accesses amortize keeping the loop counter resident.
    function encodeImpl(bytes memory input) internal pure returns (bytes memory out) {
        out = new bytes(input.length * 2);
        for (uint256 i; i < input.length; ++i) {
            uint256 byteValue = uint8(input[i]);
            out[2 * i] = HEX[byteValue >> 4];
            out[2 * i + 1] = HEX[byteValue & 15];
        }
    }

    function scan(bytes memory input) public pure returns (bool) {
        return scanImpl(input);
    }

    function twice(bytes memory first, bytes memory second) public pure returns (bool, bool) {
        return (scanImpl(first), scanImpl(second));
    }

    function scanImpl(bytes memory input) internal pure returns (bool) {
        for (uint256 i; i < input.length; ++i) {
            if (input[i] > 0x7f) return false;
        }
        return true;
    }

    // An exit returning the loop counter still needs its value after the branch.
    function firstHigh(bytes memory input) public pure returns (uint256) {
        for (uint256 i; i < input.length; ++i) {
            if (input[i] > 0x7f) return i;
        }
        return input.length;
    }

    // Promoting the counter must preserve the reachable uint8 overflow panic.
    function checkedScan(uint256 length) public pure returns (bool) {
        bytes memory input = new bytes(length);
        for (uint8 i; i < input.length; ++i) {
            if (input[i] > 0x7f) return false;
        }
        return true;
    }
}
