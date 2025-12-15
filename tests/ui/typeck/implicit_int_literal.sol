//@compile-flags: -Ztypeck
function f() {
    // === Non-negative literals to uint ===
    // Value must fit in the unsigned range [0, 2^N - 1]
    uint8 u8_max = 255;
    uint8 u8_overflow = 256; //~ ERROR: mismatched types
    uint16 u16_max = 65535;
    uint16 u16_overflow = 65536; //~ ERROR: mismatched types
    uint32 u32_max = 4294967295;
    uint256 u256_max = 115792089237316195423570985008687907853269984665640564039457584007913129639935;

    // === Non-negative literals to int ===
    // TypeSize stores the actual bit length. For signed types, non-negative values
    // need strict bit comparison since the sign bit takes one slot.
    // E.g., 127 needs 7 bits, and int8 has 7 bits for magnitude, so it fits.
    // But 128 needs 8 bits, which exceeds int8's 7-bit positive range.

    // int_literal[1] (1-8 bits) -> int8+ works for values that fit (0-127)
    int8 i8_max = 127;
    // int_literal[1] (8 bits) -> int16+ works for 128-255 (needs 8 bits, exceeds int8's 7-bit positive range)
    int16 i16_from_128 = 128;
    int16 i16_from_255 = 255;

    // int_literal[2] (9-16 bits) -> int32+ works for 256-32767
    int16 i16_max = 32767;
    // 32768 needs 16 bits, exceeds int16's 15-bit positive range
    int32 i32_from_32768 = 32768;
    int32 i32_from_65535 = 65535;

    // Overflow cases
    int16 i16_overflow = 65536; //~ ERROR: mismatched types

    // === Zero and small values ===
    // Zero needs 1 bit, works with uint8+ and int8+
    uint8 zero_u8 = 0;
    uint256 zero_u256 = 0;
    uint8 one_u8 = 1;
    uint256 one_u256 = 1;

    int8 zero_i8 = 0;
    int256 zero_i256 = 0;
    int8 one_i8 = 1;
    int256 one_i256 = 1;
}
