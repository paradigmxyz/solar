contract DirtyMisc {
    mapping(bytes4 => uint256) m4;
    mapping(address => uint256) ma;
    mapping(bool => uint256) mb;
    mapping(int8 => uint256) mi;
    uint256[] arr;

    function injectU8(uint256 raw) internal pure returns (uint8 x) { assembly { x := raw } }
    function injectB(uint256 raw) internal pure returns (bool x) { assembly { x := raw } }
    function inject4(uint256 raw) internal pure returns (bytes4 x) { assembly { x := raw } }
    function injectA(uint256 raw) internal pure returns (address x) { assembly { x := raw } }
    function injectI8(uint256 raw) internal pure returns (int8 x) { assembly { x := raw } }

    function decodeMsgData(uint256) external pure returns (uint8) { return abi.decode(msg.data[4:], (uint8)); }
    function decodeMsgDataBool(uint256) external pure returns (bool) { return abi.decode(msg.data[4:], (bool)); }
    function decodeSlice(bytes calldata b) external pure returns (uint8) { return abi.decode(b, (uint8)); }
    function decodeSliceTuple(bytes calldata b) external pure returns (uint8, int8) { return abi.decode(b, (uint8, int8)); }
    function decodeSliceStatic(bytes calldata b) external pure returns (uint8) { uint8[2] memory a = abi.decode(b, (uint8[2])); return a[1]; }
    function keyBytes4(uint256 raw) external returns (uint256) { m4[bytes4(uint32(1))] = 7; return m4[inject4(raw)]; }
    function keyAddress(uint256 raw) external returns (uint256) { ma[address(1)] = 7; return ma[injectA(raw)]; }
    function keyBool(uint256 raw) external returns (uint256) { mb[true] = 7; return mb[injectB(raw)]; }
    function keyInt8(uint256 raw) external returns (uint256) { mi[-1] = 7; return mi[injectI8(raw)]; }
    function allocLen(uint256 raw) external pure returns (uint256) { uint256[] memory a = new uint256[](injectU8(raw)); return a.length; }
    function allocBytesLen(uint256 raw) external pure returns (uint256) { bytes memory b = new bytes(injectU8(raw)); return b.length; }
    function requireDirty(uint256 raw) external pure returns (uint256) { require(injectB(raw), "x"); return 1; }
    function assertDirty(uint256 raw) external pure returns (uint256) { assert(injectB(raw)); return 1; }
    function whileDirty(uint256 raw) external pure returns (uint256 n) { bool b = injectB(raw); while (b) { n++; b = false; } }
    function boolToUint(uint256 raw) external pure returns (uint256) { return injectB(raw) ? 1 : 0; }
    function boolAndArith(uint256 raw) external pure returns (uint256 r) { bool b = injectB(raw); r = 0; if (b) r += 1; if (!b) r += 2; }
    function selectorDirty(uint256 raw) external pure returns (bytes memory) { return abi.encodeWithSelector(inject4(raw), uint256(1)); }
    function encodeCallDirty(uint256 raw) external view returns (bytes memory) { return abi.encodeCall(this.allocLen, (uint256(injectU8(raw)))); }
    function pushDirty(uint256 raw) external returns (uint256) { arr.push(injectU8(raw)); return arr[0]; }
    function popToDirtyIndex(uint256 raw) external returns (uint256) { arr.push(1); arr.push(2); arr.push(3); return arr[injectU8(raw)]; }
    function bytesIndexWrite(uint256 raw, uint256 raw2) external pure returns (bytes memory) { bytes memory b = new bytes(4); b[injectU8(raw)] = bytes1(injectU8(raw2)); return b; }
    function stringConcatDirty(uint256 raw) external pure returns (bytes memory) { return bytes.concat(bytes1(injectU8(raw)), bytes2(uint16(injectU8(raw)))); }
    function unaryMinusWide(uint256 raw) external pure returns (int256) { int8 x = injectI8(raw); return -int256(x); }
    function shiftByDirty(uint256 raw) external pure returns (uint256) { return 1 << injectU8(raw); }
    function shiftByDirtySigned(uint256 raw) external pure returns (int256) { return int256(-256) >> injectU8(raw); }
    function expByDirty(uint256 raw) external pure returns (uint256) { return 2 ** injectU8(raw); }
    function expByDirtyUnchecked(uint256 raw) external pure returns (uint256) { unchecked { return 3 ** injectU8(raw); } }
    function mulmodDirty(uint256 raw, uint256 raw2) external pure returns (uint256) { return mulmod(injectU8(raw), injectU8(raw2), 251); }
    function u8ArrayLiteral(uint256 raw) external pure returns (uint8[3] memory) { return [injectU8(raw), 1, 2]; }
    function u8ArrayLiteralWide(uint256 raw) external pure returns (uint256[3] memory a) { uint8[3] memory b = [injectU8(raw), 1, 2]; a[0] = b[0]; a[1] = b[1]; a[2] = b[2]; }
    function structLiteral(uint256 raw) external pure returns (uint8, uint256) { S memory s = S(injectU8(raw), 5); return (s.a, s.a); }
    function returnMultiple(uint256 raw) external pure returns (uint8, uint256, int8) { return (injectU8(raw), injectU8(raw), injectI8(raw)); }
    function eventless(uint256 raw) external pure returns (bytes32) { return keccak256(abi.encode(injectU8(raw), injectB(raw), inject4(raw), injectA(raw), injectI8(raw))); }
    struct S { uint8 a; uint256 b; }
}
