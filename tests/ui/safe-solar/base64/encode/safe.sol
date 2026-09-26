//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: encode 0x, false, false => 0x
//@ run-call: encode 0x0b, false, false => 0x43773d3d
//@ run-call: encode 0x0b, false, true => 0x4377
//@ run-call: encode 0x0b3055, false, false => 0x437a4256
//@ run-call: encode 0x0b30557a9fc4e90e33587da2c7ec11365b80a5caef1439, true, false => 0x437a425665705f453651347a57483269782d77524e6c75417063727646446b3d
//@ run-call: encode 0x0b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e, false, false => 0x437a425665702f453651347a57483269782b77524e6c75417063727646446c65
//@ run-call: encode 0x0b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e83, false, true => 0x437a425665702f453651347a57483269782b77524e6c75417063727646446c656777
//@ run-call: encode 0x0b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e83a8cdf2173c6186abd0f51a3f6489aed3f81d42678cb1d6, true, false => 0x437a425665705f453651347a57483269782d77524e6c75417063727646446c6567366a4e3868633859596172305055615032534a7274503448554a6e6a4c4857

// Base64 in checked Solidity: twenty-four input bytes are thirty-two
// characters, one word. The gather is one read, the alphabet is arithmetic on
// all thirty-two lanes at once, and the store is one write; the remainder
// under a block goes three bytes at a time through the table.
// CHECK-LABEL: fn @encode
// CHECK: 0x3f0000003f0000003f0000003f0000003f0000003f0000003f0000003f
// CHECK-NOT: icall @readBytes24
// CHECK-NOT: icall @writeBytes32
import {Bytes} from "solar:core/v1/Bytes.sol";

contract Safe {
    bytes private constant TABLE = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    uint256 private constant ONES =
        0x0101010101010101010101010101010101010101010101010101010101010101;
    uint256 private constant MASK_96 =
        0x0000000000000000000000000000000000000000ffffffffffffffffffffffff;
    uint256 private constant SPREAD_HIGH_48 =
        0x00000000ffffffffffff00000000000000000000ffffffffffff000000000000;
    uint256 private constant SPREAD_LOW_48 =
        0x00000000000000000000ffffffffffff00000000000000000000ffffffffffff;
    uint256 private constant SPREAD_HIGH_24 =
        0x0000ffffff0000000000ffffff0000000000ffffff0000000000ffffff000000;
    uint256 private constant SPREAD_LOW_24 =
        0x0000000000ffffff0000000000ffffff0000000000ffffff0000000000ffffff;
    uint256 private constant LANE_63 =
        0x0000003f0000003f0000003f0000003f0000003f0000003f0000003f0000003f;

    function encode(bytes memory data, bool fileSafe, bool noPadding)
        public
        pure
        returns (bytes memory out)
    {
        uint256 n = data.length;
        uint256 length = ((n + 2) / 3) * 4;
        if (noPadding && n % 3 != 0) length -= 3 - n % 3;
        out = new bytes(length);
        uint256 i;
        uint256 j;
        while (i + 24 <= n) {
            bytes32 chars = word(uint256(uint192(Bytes.readBytes24(data, i))), fileSafe);
            Bytes.writeBytes32(out, j, chars);
            i += 24;
            j += 32;
        }
        bytes memory table = TABLE;
        if (fileSafe) {
            table[62] = bytes1(uint8(0x2d));
            table[63] = bytes1(uint8(0x5f));
        }
        while (i < n) {
            uint256 group = uint256(uint8(data[i])) << 16;
            if (i + 1 < n) group |= uint256(uint8(data[i + 1])) << 8;
            if (i + 2 < n) group |= uint8(data[i + 2]);
            out[j] = table[group >> 18];
            out[j + 1] = table[(group >> 12) & 63];
            if (j + 2 < length) out[j + 2] = i + 1 < n ? table[(group >> 6) & 63] : bytes1("=");
            if (j + 3 < length) out[j + 3] = i + 2 < n ? table[group & 63] : bytes1("=");
            i += 3;
            j += 4;
        }
    }

    /// @dev The thirty-two characters of the twenty-four bytes in `packed`.
    function word(uint256 packed, bool fileSafe) private pure returns (bytes32) {
        uint256 stepDown = fileSafe ? 13 : 15;
        uint256 stepUp = fileSafe ? 49 : 3;
        uint256 x = ((packed >> 96) << 128) | (packed & MASK_96);
        x = ((x & SPREAD_HIGH_48) << 16) | (x & SPREAD_LOW_48);
        x = ((x & SPREAD_HIGH_24) << 8) | (x & SPREAD_LOW_24);
        uint256 v = (((x >> 18) & LANE_63) << 24) | (((x >> 12) & LANE_63) << 16)
            | (((x >> 6) & LANE_63) << 8) | (x & LANE_63);
        uint256 letter = ((v + 102 * ONES) >> 7) & ONES;
        uint256 digit = ((v + 76 * ONES) >> 7) & ONES;
        uint256 last2 = ((v + 66 * ONES) >> 7) & ONES;
        uint256 last1 = ((v + 65 * ONES) >> 7) & ONES;
        return bytes32(v + 65 * ONES + 6 * letter - 75 * digit - stepDown * last2 + stepUp * last1);
    }
}
