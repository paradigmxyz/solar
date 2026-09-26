//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: toHex 3735928559, 4 => 0x6465616462656566
//@ run-call: toHex 1461501637330902918203684832716283019655932538316, 20 => 0x66666666666666666666666666666666666666666666666666666666666666666666666665646363
//@ run-call: toHex 0, 1 => 0x3030
//@ run-call: toHex 115792089237316195423570985008687907853269984665640564039457584007913129639935, 32 => 0x66666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666
//@ run-call: toHex 2174026233105990997908215097954566016, 17 => 0x30303031613262336334643565366637303831393261336234633564366537663830
//@ run-call-fail: toHex 256, 1 => 0x2194895a

// Hex in checked Solidity: sixteen bytes of the value are thirty-two
// characters, one word. The nibbles are spread one to a byte by halving the
// packing five times, turned into digits with two additions, and stored with
// one write; what is left under sixteen bytes goes two digits at a time.
// CHECK-LABEL: fn @toHex
// CHECK: 0xf0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f
// CHECK-NOT: icall @writeBytes32
import {Bytes} from "solar:core/v1/Bytes.sol";

contract Safe {
    error HexLengthInsufficient();

    bytes16 private constant HEX = "0123456789abcdef";
    uint256 private constant MASK_64 =
        0x0000000000000000ffffffffffffffff0000000000000000ffffffffffffffff;
    uint256 private constant MASK_32 =
        0x00000000ffffffff00000000ffffffff00000000ffffffff00000000ffffffff;
    uint256 private constant MASK_16 =
        0x0000ffff0000ffff0000ffff0000ffff0000ffff0000ffff0000ffff0000ffff;
    uint256 private constant MASK_8 =
        0x00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff;
    uint256 private constant MASK_4 =
        0x0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f;
    uint256 private constant SPREAD_1 =
        0x0101010101010101010101010101010101010101010101010101010101010101;
    uint256 private constant SPREAD_6 =
        0x0606060606060606060606060606060606060606060606060606060606060606;
    uint256 private constant SPREAD_ASCII_0 =
        0x3030303030303030303030303030303030303030303030303030303030303030;

    function toHex(uint256 value, uint256 byteCount) public pure returns (bytes memory out) {
        out = new bytes(byteCount * 2);
        uint256 i = byteCount;
        while (i >= 16) {
            i -= 16;
            Bytes.writeBytes32(out, i * 2, word(value & type(uint128).max));
            value >>= 128;
        }
        while (i != 0) {
            --i;
            out[i * 2 + 1] = HEX[value & 15];
            value >>= 4;
            out[i * 2] = HEX[value & 15];
            value >>= 4;
        }
        if (value != 0) revert HexLengthInsufficient();
    }

    /// @dev The thirty-two hex characters of the sixteen bytes in `x`.
    function word(uint256 x) private pure returns (bytes32) {
        x = (x | (x << 64)) & MASK_64;
        x = (x | (x << 32)) & MASK_32;
        x = (x | (x << 16)) & MASK_16;
        x = (x | (x << 8)) & MASK_8;
        x = (x | (x << 4)) & MASK_4;
        uint256 letters = ((x + SPREAD_6) >> 4) & SPREAD_1;
        return bytes32(x + SPREAD_ASCII_0 + letters * 39);
    }
}
