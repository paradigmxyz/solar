contract DirtyWiden {
    uint256[] sarr;
    uint256[4] sfixed;
    mapping(uint256 => uint256) m;

    function injectU8(uint256 raw) internal pure returns (uint8 x) { assembly { x := raw } }
    function injectI8(uint256 raw) internal pure returns (int8 x) { assembly { x := raw } }
    function take(uint256 v) internal pure returns (uint256) { return v; }
    function takeI(int256 v) internal pure returns (int256) { return v; }

    function assign(uint256 raw) external pure returns (uint256) { uint256 y = injectU8(raw); return y; }
    function assignSigned(uint256 raw) external pure returns (int256) { int256 y = injectI8(raw); return y; }
    function ret(uint256 raw) external pure returns (uint256) { return injectU8(raw); }
    function retSigned(uint256 raw) external pure returns (int256) { return injectI8(raw); }
    function callArg(uint256 raw) external pure returns (uint256) { return take(injectU8(raw)); }
    function callArgSigned(uint256 raw) external pure returns (int256) { return takeI(injectI8(raw)); }
    function binop(uint256 raw) external pure returns (uint256) { return injectU8(raw) + uint256(1000); }
    function binopSigned(uint256 raw) external pure returns (int256) { return injectI8(raw) + int256(1000); }
    function binopMul(uint256 raw) external pure returns (uint256) { return uint256(3) * injectU8(raw); }
    function binopCmp(uint256 raw) external pure returns (bool) { return injectU8(raw) < uint256(300); }
    function binopCmpSigned(uint256 raw) external pure returns (bool) { return injectI8(raw) < int256(0); }
    function newLen(uint256 raw) external pure returns (uint256) { uint256[] memory a = new uint256[](injectU8(raw)); return a.length; }
    function newBytesLen(uint256 raw) external pure returns (uint256) { bytes memory b = new bytes(injectU8(raw)); return b.length; }
    function memIndex(uint256 raw) external pure returns (uint256) { uint256[300] memory a; a[1] = 9; a[2] = 8; return a[injectU8(raw)]; }
    function memDynIndex(uint256 raw) external pure returns (uint256) { uint256[] memory a = new uint256[](300); a[1] = 9; return a[injectU8(raw)]; }
    function storageIndex(uint256 raw) external returns (uint256) { sarr.push(1); sarr.push(2); sarr.push(3); return sarr[injectU8(raw)]; }
    function storageFixedIndex(uint256 raw) external returns (uint256) { sfixed[1] = 9; return sfixed[injectU8(raw)]; }
    function calldataIndex(uint256 raw, uint256[] calldata a) external pure returns (uint256) { return a[injectU8(raw)]; }
    function mappingKey(uint256 raw) external returns (uint256) { m[1] = 7; return m[injectU8(raw)]; }
    function loopBound(uint256 raw) external pure returns (uint256 n) { for (uint256 i = 0; i < injectU8(raw); i++) { n++; if (n > 600) break; } }
    function ternary(uint256 raw, uint256 raw2) external pure returns (uint256) { return raw2 > 0 ? injectU8(raw) : uint256(5); }
    function shiftAmount(uint256 raw) external pure returns (uint256) { return uint256(1) << injectU8(raw); }
    function expAmount(uint256 raw) external pure returns (uint256) { return uint256(2) ** injectU8(raw); }
    function arrayLiteral(uint256 raw) external pure returns (uint256) { uint256[2] memory a = [uint256(injectU8(raw)), 1]; return a[0]; }
    function encodeWide(uint256 raw) external pure returns (bytes memory) { return abi.encode(uint256(1), injectU8(raw)); }
    function structField(uint256 raw) external pure returns (uint256) { S memory s = S(injectU8(raw)); return s.v; }
    function tupleAssign(uint256 raw) external pure returns (uint256 a, uint256 b) { (a, b) = (injectU8(raw), injectU8(raw)); }
    function compoundAssign(uint256 raw) external pure returns (uint256) { uint256 y = 1000; y += injectU8(raw); return y; }
    function compoundSub(uint256 raw) external pure returns (uint256) { uint256 y = 100000; y -= injectU8(raw); return y; }
    function delegateIndex(uint256 raw) external pure returns (uint256) { uint8 i = injectU8(raw); uint256 j = i; return j; }
    function bytesLen(uint256 raw) external pure returns (uint256) { bytes memory b = new bytes(300); return b[injectU8(raw)] == 0 ? 1 : 2; }
    function pushLen(uint256 raw) external returns (uint256) { uint8 n = injectU8(raw); for (uint8 i = 0; i < n; i++) sarr.push(i); return sarr.length; }
    function addressWiden(uint256 raw) external pure returns (uint256) { address a; assembly { a := raw } return uint256(uint160(a)) + 1; }
    function boolWiden(uint256 raw) external pure returns (uint256) { bool b; assembly { b := raw } return (b ? 1 : 0) + 10; }
    struct S { uint256 v; }
}
