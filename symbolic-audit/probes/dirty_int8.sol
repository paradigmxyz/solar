contract DirtyInt8 {
    int8 s;
    int8 s2;

    function inject(uint256 raw) internal pure returns (int8 x) { assembly { x := raw } }

    function ret(uint256 raw) external pure returns (int8) { return inject(raw); }
    function eq(uint256 raw, uint256 raw2) external pure returns (bool) { return inject(raw) == inject(raw2); }
    function lt(uint256 raw, uint256 raw2) external pure returns (bool) { return inject(raw) < inject(raw2); }
    function ltZero(uint256 raw) external pure returns (bool) { return inject(raw) < 0; }
    function addChecked(uint256 raw, uint256 raw2) external pure returns (int8) { return inject(raw) + inject(raw2); }
    function addUnchecked(uint256 raw, uint256 raw2) external pure returns (int8) { unchecked { return inject(raw) + inject(raw2); } }
    function subChecked(uint256 raw, uint256 raw2) external pure returns (int8) { return inject(raw) - inject(raw2); }
    function mulChecked(uint256 raw, uint256 raw2) external pure returns (int8) { return inject(raw) * inject(raw2); }
    function div(uint256 raw, uint256 raw2) external pure returns (int8) { return inject(raw) / inject(raw2); }
    function divUnchecked(uint256 raw, uint256 raw2) external pure returns (int8) { unchecked { return inject(raw) / inject(raw2); } }
    function mod(uint256 raw, uint256 raw2) external pure returns (int8) { return inject(raw) % inject(raw2); }
    function neg(uint256 raw) external pure returns (int8) { return -inject(raw); }
    function negUnchecked(uint256 raw) external pure returns (int8) { unchecked { return -inject(raw); } }
    function shr(uint256 raw, uint256 raw2) external pure returns (int8) { return inject(raw) >> uint8(raw2); }
    function shl(uint256 raw, uint256 raw2) external pure returns (int8) { return inject(raw) << uint8(raw2); }
    function bitnot(uint256 raw) external pure returns (int8) { return ~inject(raw); }
    function widen(uint256 raw) external pure returns (int256) { return int256(inject(raw)); }
    function widen16(uint256 raw) external pure returns (int16) { return int16(inject(raw)); }
    function toUint8(uint256 raw) external pure returns (uint8) { return uint8(inject(raw)); }
    function toUint256(uint256 raw) external pure returns (uint256) { return uint256(int256(inject(raw))); }
    function assemblyRead(uint256 raw) external pure returns (uint256 r) { int8 x = inject(raw); assembly { r := x } }
    function storeLoad(uint256 raw) external returns (int8, int8) { s = -3; s2 = 5; s = inject(raw); return (s, s2); }
    function memoryRoundTrip(uint256 raw) external pure returns (int8, uint256) { int8[2] memory a; a[0] = inject(raw); a[1] = 7; uint256 w; assembly { w := mload(a) } return (a[0], w); }
    function abs(uint256 raw) external pure returns (int8) { int8 x = inject(raw); return x < 0 ? -x : x; }
    function encode(uint256 raw) external pure returns (bytes memory) { return abi.encode(inject(raw)); }
    function inc(uint256 raw) external pure returns (int8) { int8 x = inject(raw); x++; return x; }
    function dec(uint256 raw) external pure returns (int8) { int8 x = inject(raw); x--; return x; }
    function expChecked(uint256 raw, uint256 raw2) external pure returns (int8) { return inject(raw) ** uint8(raw2); }
}
