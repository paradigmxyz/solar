contract DirtyWiden2 {
    error E(uint256 v);
    error E8(uint8 v);
    struct S { mapping(uint256 => uint256) m; }
    S s;
    mapping(int256 => uint256) mi;
    mapping(uint256 => mapping(uint256 => uint256)) mm;
    mapping(uint256 => uint256) m;
    uint256 su;
    int256 si;
    uint256[] arr;

    function injectU8(uint256 raw) internal pure returns (uint8 x) { assembly { x := raw } }
    function injectI8(uint256 raw) internal pure returns (int8 x) { assembly { x := raw } }
    function injectU128(uint256 raw) internal pure returns (uint128 x) { assembly { x := raw } }

    function storageWrite(uint256 raw) external returns (uint256) { su = injectU8(raw); return su; }
    function storageWriteSigned(uint256 raw) external returns (int256) { si = injectI8(raw); return si; }
    function customError(uint256 raw) external pure { revert E(injectU8(raw)); }
    function customErrorNarrow(uint256 raw) external pure { revert E8(injectU8(raw)); }
    function sliceStart(uint256 raw, bytes calldata b) external pure returns (uint256) { bytes memory c = b[injectU8(raw):]; return c.length; }
    function sliceEnd(uint256 raw, bytes calldata b) external pure returns (uint256) { bytes memory c = b[:injectU8(raw)]; return c.length; }
    function keySigned(uint256 raw) external returns (uint256) { mi[-1] = 7; return mi[injectI8(raw)]; }
    function keyU128(uint256 raw) external returns (uint256) { m[1] = 7; return m[injectU128(raw)]; }
    function keyNested(uint256 raw) external returns (uint256) { mm[1][1] = 7; return mm[injectU8(raw)][injectU8(raw)]; }
    function keyStruct(uint256 raw) external returns (uint256) { s.m[1] = 7; return s.m[injectU8(raw)]; }
    function keyWrite(uint256 raw) external returns (uint256) { m[injectU8(raw)] = 7; return m[1]; }
    function keyDelete(uint256 raw) external returns (uint256) { m[1] = 7; delete m[injectU8(raw)]; return m[1]; }
    function newNarrowLen(uint256 raw) external pure returns (uint256) { uint8[] memory a = new uint8[](injectU8(raw)); return a.length; }
    function newNestedLen(uint256 raw) external pure returns (uint256) { uint256[][] memory a = new uint256[][](injectU8(raw)); return a.length; }
    function newStringLen(uint256 raw) external pure returns (uint256) { string memory st = new string(injectU8(raw)); return bytes(st).length; }
    function encodeSelector(uint256 raw) external pure returns (bytes memory) { return abi.encodeWithSelector(bytes4(0x12345678), injectU8(raw)); }
    function encodeSignature(uint256 raw) external pure returns (bytes memory) { return abi.encodeWithSignature("f(uint256)", injectU8(raw)); }
    function pushWide(uint256 raw) external returns (uint256) { arr.push(injectU8(raw)); return arr[0]; }
    function pushLen(uint256 raw) external returns (uint256) { arr.push(); arr.push(); return arr.length + injectU8(raw); }
    function requireMsg(uint256 raw) external pure returns (uint256) { require(injectU8(raw) == 1, "bad"); return 1; }
    function ifWide(uint256 raw) external pure returns (uint256) { if (injectU8(raw) == uint256(1)) return 1; return 2; }
    function condWide(uint256 raw) external pure returns (uint256) { return injectU8(raw) > uint256(255) ? 1 : 2; }
    function indexByI8(uint256 raw) external pure returns (uint256) { uint256[300] memory a; a[1] = 9; return a[uint8(injectI8(raw))]; }
    function shiftWide(uint256 raw) external pure returns (uint256) { return uint256(injectU8(raw)) << 8 | injectU8(raw); }
    function orWide(uint256 raw) external pure returns (uint256) { return uint256(0x10000) | injectU8(raw); }
    function xorWide(uint256 raw) external pure returns (uint256) { return uint256(0x10000) ^ injectU8(raw); }
    function andWide(uint256 raw) external pure returns (uint256) { return uint256(0x1ffff) & injectU8(raw); }
    function divWide(uint256 raw) external pure returns (uint256) { return uint256(0x10000) / (uint256(injectU8(raw)) + 1); }
    function modWide(uint256 raw) external pure returns (uint256) { return uint256(0x10000) % (injectU8(raw) + uint256(1)); }
    function keccakWide(uint256 raw) external pure returns (bytes32) { return keccak256(abi.encode(uint256(1), injectU8(raw))); }
    function packedWide(uint256 raw) external pure returns (bytes memory) { return abi.encodePacked(uint256(1), injectU8(raw)); }
}
