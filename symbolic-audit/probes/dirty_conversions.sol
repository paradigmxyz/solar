contract DirtyConversions {
    function u256ToU8(uint256 raw) external pure returns (uint8) { return uint8(raw); }
    function u256ToU8Wide(uint256 raw) external pure returns (uint256) { return uint8(raw); }
    function u256ToI8(uint256 raw) external pure returns (int8) { return int8(uint8(raw)); }
    function u256ToI8Wide(uint256 raw) external pure returns (int256) { return int8(uint8(raw)); }
    function i256ToI8(int256 raw) external pure returns (int8) { return int8(raw); }
    function i256ToU8(int256 raw) external pure returns (uint8) { return uint8(uint256(raw)); }
    function i8ToU256(int256 raw) external pure returns (uint256) { return uint256(uint8(int8(raw))); }
    function i8ToI256(int256 raw) external pure returns (int256) { return int8(raw); }
    function u256ToAddr(uint256 raw) external pure returns (address) { return address(uint160(raw)); }
    function u256ToAddrWide(uint256 raw) external pure returns (uint256) { return uint256(uint160(address(uint160(raw)))); }
    function b32ToB4(bytes32 raw) external pure returns (bytes4) { return bytes4(raw); }
    function b32ToB4Wide(bytes32 raw) external pure returns (bytes32) { return bytes32(bytes4(raw)); }
    function b32ToB4ToUint(bytes32 raw) external pure returns (uint256) { return uint32(bytes4(raw)); }
    function u256ToB4(uint256 raw) external pure returns (bytes4) { return bytes4(uint32(raw)); }
    function u256ToB4Wide(uint256 raw) external pure returns (bytes32) { return bytes4(uint32(raw)); }
    function u8ToB1ToU8(uint256 raw) external pure returns (uint8) { return uint8(bytes1(uint8(raw))); }
    function u32ToB4ToB2(uint256 raw) external pure returns (bytes2) { return bytes2(bytes4(uint32(raw))); }
    function b4ToB8(bytes32 raw) external pure returns (bytes8) { return bytes8(bytes4(raw)); }
    function b4ToU64(bytes32 raw) external pure returns (uint64) { return uint64(bytes8(bytes4(raw))); }
    function u256ToBool(uint256 raw) external pure returns (bool) { return raw != 0; }
    function u256ToEnum(uint256 raw) external pure returns (E) { return E(raw); }
    function u256ToEnumToUint(uint256 raw) external pure returns (uint256) { return uint256(E(raw)); }
    function u8ToEnum(uint256 raw) external pure returns (E) { return E(uint8(raw)); }
    function shiftThenNarrow(uint256 raw) external pure returns (uint8) { return uint8(raw >> 8); }
    function narrowThenShift(uint256 raw) external pure returns (uint8) { return uint8(raw) >> 4; }
    function narrowShl(uint256 raw) external pure returns (uint8) { return uint8(raw) << 4; }
    function narrowShlWide(uint256 raw) external pure returns (uint256) { return uint256(uint8(raw) << 4); }
    function narrowMulWide(uint256 raw) external pure returns (uint256) { unchecked { return uint256(uint8(raw) * uint8(raw)); } }
    function narrowMulWideChecked(uint256 raw) external pure returns (uint256) { return uint256(uint8(raw) * uint8(raw)); }
    function narrowAddWide(uint256 raw, uint256 raw2) external pure returns (uint256) { unchecked { return uint256(uint8(raw) + uint8(raw2)); } }
    function narrowSubWide(uint256 raw, uint256 raw2) external pure returns (uint256) { unchecked { return uint256(uint8(raw) - uint8(raw2)); } }
    function narrowNegWide(uint256 raw) external pure returns (uint256) { unchecked { return uint256(uint8(0) - uint8(raw)); } }
    function narrowNotWide(uint256 raw) external pure returns (uint256) { return uint256(~uint8(raw)); }
    function signedNarrowWide(int256 raw) external pure returns (int256) { unchecked { return int256(int8(raw) + int8(1)); } }
    function signedNarrowMulWide(int256 raw) external pure returns (int256) { unchecked { return int256(int8(raw) * int8(raw)); } }
    function signedNarrowNegWide(int256 raw) external pure returns (int256) { unchecked { return int256(-int8(raw)); } }
    function signedNarrowShrWide(int256 raw, uint256 n) external pure returns (int256) { return int256(int8(raw) >> uint8(n)); }
    function signedNarrowShlWide(int256 raw, uint256 n) external pure returns (int256) { return int256(int8(raw) << uint8(n)); }
    function signedNarrowDivWide(int256 raw, int256 raw2) external pure returns (int256) { unchecked { return int256(int8(raw) / int8(raw2)); } }
    function signedNarrowModWide(int256 raw, int256 raw2) external pure returns (int256) { return int256(int8(raw) % int8(raw2)); }
    function signedToUnsignedCmp(int256 raw) external pure returns (bool) { return uint8(int8(raw)) > 127; }
    function expNarrow(uint256 raw, uint256 raw2) external pure returns (uint256) { return uint256(uint8(raw) ** uint8(raw2)); }
    function expNarrowUnchecked(uint256 raw, uint256 raw2) external pure returns (uint256) { unchecked { return uint256(uint8(raw) ** uint8(raw2)); } }
    function expSigned(int256 raw, uint256 raw2) external pure returns (int256) { return int256(int8(raw) ** uint8(raw2)); }
    function expSignedUnchecked(int256 raw, uint256 raw2) external pure returns (int256) { unchecked { return int256(int8(raw) ** uint8(raw2)); } }
    function addmodNarrow(uint256 raw, uint256 raw2) external pure returns (uint256) { return addmod(uint8(raw), uint8(raw2), 7); }
    function mulmodNarrow(uint256 raw, uint256 raw2) external pure returns (uint256) { return mulmod(uint8(raw), uint8(raw2), 7); }
    function ternaryNarrow(uint256 raw, uint256 raw2) external pure returns (uint256) { return uint256(raw2 > 0 ? uint8(raw) : uint8(1)); }
    function bytesIndexToUint(bytes32 raw, uint256 i) external pure returns (uint256) { return uint8(raw[i]); }
    function b1ToBool(bytes32 raw) external pure returns (bool) { return raw[0] != 0; }
    enum E { A, B, C }
}
