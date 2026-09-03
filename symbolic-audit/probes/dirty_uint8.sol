contract DirtyUint8 {
    uint8 s;
    uint8 s2;
    mapping(uint8 => uint256) m;

    function inject(uint256 raw) internal pure returns (uint8 x) { assembly { x := raw } }

    function ret(uint256 raw) external pure returns (uint8) { return inject(raw); }
    function eq(uint256 raw, uint256 raw2) external pure returns (bool) { return inject(raw) == inject(raw2); }
    function eqConst(uint256 raw) external pure returns (bool) { return inject(raw) == 1; }
    function lt(uint256 raw, uint256 raw2) external pure returns (bool) { return inject(raw) < inject(raw2); }
    function addChecked(uint256 raw, uint256 raw2) external pure returns (uint8) { return inject(raw) + inject(raw2); }
    function addUnchecked(uint256 raw, uint256 raw2) external pure returns (uint8) { unchecked { return inject(raw) + inject(raw2); } }
    function subChecked(uint256 raw, uint256 raw2) external pure returns (uint8) { return inject(raw) - inject(raw2); }
    function mulChecked(uint256 raw, uint256 raw2) external pure returns (uint8) { return inject(raw) * inject(raw2); }
    function mulUnchecked(uint256 raw, uint256 raw2) external pure returns (uint8) { unchecked { return inject(raw) * inject(raw2); } }
    function div(uint256 raw, uint256 raw2) external pure returns (uint8) { return inject(raw) / inject(raw2); }
    function mod(uint256 raw, uint256 raw2) external pure returns (uint8) { return inject(raw) % inject(raw2); }
    function expChecked(uint256 raw, uint256 raw2) external pure returns (uint8) { return inject(raw) ** inject(raw2); }
    function expUnchecked(uint256 raw, uint256 raw2) external pure returns (uint8) { unchecked { return inject(raw) ** inject(raw2); } }
    function shl(uint256 raw, uint256 raw2) external pure returns (uint8) { return inject(raw) << inject(raw2); }
    function shr(uint256 raw, uint256 raw2) external pure returns (uint8) { return inject(raw) >> inject(raw2); }
    function shlWide(uint256 raw) external pure returns (uint256) { return uint256(inject(raw)) << 8; }
    function bitnot(uint256 raw) external pure returns (uint8) { return ~inject(raw); }
    function bitand(uint256 raw, uint256 raw2) external pure returns (uint8) { return inject(raw) & inject(raw2); }
    function bitor(uint256 raw, uint256 raw2) external pure returns (uint8) { return inject(raw) | inject(raw2); }
    function widen(uint256 raw) external pure returns (uint256) { return uint256(inject(raw)); }
    function widen16(uint256 raw) external pure returns (uint16) { return uint16(inject(raw)); }
    function toInt8(uint256 raw) external pure returns (int8) { return int8(inject(raw)); }
    function toInt256(uint256 raw) external pure returns (int256) { return int256(uint256(inject(raw))); }
    function toBytes1(uint256 raw) external pure returns (bytes1) { return bytes1(inject(raw)); }
    function inc(uint256 raw) external pure returns (uint8) { uint8 x = inject(raw); x++; return x; }
    function incUnchecked(uint256 raw) external pure returns (uint8) { uint8 x = inject(raw); unchecked { x++; } return x; }
    function neg(uint256 raw) external pure returns (uint8) { unchecked { return 0 - inject(raw); } }
    function assemblyRead(uint256 raw) external pure returns (uint256 r) { uint8 x = inject(raw); assembly { r := x } }
    function assemblyReadAfterCopy(uint256 raw) external pure returns (uint256 r) { uint8 x = inject(raw); uint8 y = x; assembly { r := y } }
    function storeLoad(uint256 raw) external returns (uint8, uint8) { s = 0xAA; s2 = 0xBB; s = inject(raw); return (s, s2); }
    function storeAssemblyLoad(uint256 raw) external returns (uint256 slot) { s = 0xAA; s2 = 0xBB; s = inject(raw); assembly { slot := sload(s.slot) } }
    function memoryRoundTrip(uint256 raw) external pure returns (uint8, uint256) { uint8[2] memory a; a[0] = inject(raw); a[1] = 0xCC; uint256 w; assembly { w := mload(a) } return (a[0], w); }
    function mappingKey(uint256 raw) external returns (uint256) { m[1] = 7; return m[inject(raw)]; }
    function arrayIndex(uint256 raw) external pure returns (uint256) { uint256[4] memory a; a[1] = 9; return a[inject(raw)]; }
    function ternary(uint256 raw) external pure returns (uint8) { return inject(raw) > 5 ? inject(raw) : 5; }
    function encode(uint256 raw) external pure returns (bytes memory) { return abi.encode(inject(raw)); }
    function encodePacked(uint256 raw) external pure returns (bytes memory) { return abi.encodePacked(inject(raw)); }
    function hash(uint256 raw) external pure returns (bytes32) { return keccak256(abi.encodePacked(inject(raw))); }
    function ifBranch(uint256 raw) external pure returns (uint256) { if (inject(raw) == 0) return 1; return 2; }
    function loopBound(uint256 raw) external pure returns (uint256 n) { for (uint8 i = 0; i < inject(raw); i++) { n++; if (n > 300) break; } }
    function compound(uint256 raw, uint256 raw2) external pure returns (uint8) { uint8 x = inject(raw); x += inject(raw2); return x; }
    function compoundUnchecked(uint256 raw, uint256 raw2) external pure returns (uint8) { uint8 x = inject(raw); unchecked { x -= inject(raw2); } return x; }
}
