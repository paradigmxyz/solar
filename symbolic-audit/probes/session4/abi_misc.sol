contract AbiMisc {
    struct S { uint8 a; bytes2 b; int16 c; }
    struct D { uint256 a; bytes b; S[] s; }
    struct N { S[2] fixedS; uint256[2][2] grid; }
    function encSel(uint8 a, int8 b, bytes2 c, bool d) external pure returns (bytes memory) { return abi.encodeWithSelector(0x12345678, a, b, c, d); }
    function encSig(uint8 a, address b) external pure returns (bytes memory) { return abi.encodeWithSignature("f(uint8,address)", a, b); }
    function encCall(uint8 a, int8 b) external view returns (bytes memory) { return abi.encodeCall(this.target, (a, b)); }
    function target(uint8, int8) external pure returns (uint256) { return 0; }
    function encCallImplicit(uint8 a) external view returns (bytes memory) { return abi.encodeCall(this.target2, (a, int8(a))); }
    function target2(uint256, int256) external pure returns (uint256) { return 0; }
    function encStruct(uint8 a, bytes2 b, int16 c) external pure returns (bytes memory) { return abi.encode(S(a, b, c)); }
    function encStructArr(uint8 a) external pure returns (bytes memory) { S[] memory s = new S[](2); s[1] = S(a, 0x0102, -1); return abi.encode(s); }
    function encDyn(uint256 a, bytes calldata b, uint8 n) external pure returns (bytes memory) { require(n < 3); S[] memory s = new S[](n); return abi.encode(D(a, b, s)); }
    function encNested(uint8 a) external pure returns (bytes memory) { N memory n; n.fixedS[1].a = a; n.grid[1][0] = a; return abi.encode(n); }
    function encNestedCd(N calldata n) external pure returns (bytes memory) { return abi.encode(n); }
    function encStaticArrStruct(S[2] calldata s) external pure returns (bytes memory) { return abi.encode(s); }
    function encMulti(uint8 a, string calldata s, uint8[] calldata arr, bytes3 b) external pure returns (bytes memory) { return abi.encode(a, s, arr, b); }
    function encEmpty() external pure returns (bytes memory, bytes memory) { return (abi.encode(), abi.encodePacked()); }
    function encBool(bool b) external pure returns (bytes memory) { return abi.encode(b, !b); }
    function encPackedSigned(int8 a, int16 b, int256 c) external pure returns (bytes memory) { return abi.encodePacked(a, b, c); }
    function encPackedBool(bool a, bool b) external pure returns (bytes memory) { return abi.encodePacked(a, b); }
    function encPackedAddr(address a, bytes20 b) external pure returns (bytes memory) { return abi.encodePacked(a, b); }
    function encPackedB(bytes1 a, bytes32 b, bytes calldata c, string calldata s) external pure returns (bytes memory) { return abi.encodePacked(a, b, c, s); }
    function encPackedLit() external pure returns (bytes memory) { return abi.encodePacked(uint8(1), int8(-1), true, "str", hex"aabb", bytes2(0x0102), uint256(5)); }
    function encPackedArrStatic(uint8[3] calldata a, int8[2] calldata b) external pure returns (bytes memory) { return abi.encodePacked(a, b); }
    function encPackedArrDyn(bool[] calldata a, address[] calldata b) external pure returns (bytes memory) { return abi.encodePacked(a, b); }
    function encPackedArrMem(uint16[] memory a) external pure returns (bytes memory) { return abi.encodePacked(a); }
    function encPackedEnum(E e) external pure returns (bytes memory) { return abi.encodePacked(e, E.B); }
    enum E { A, B }
    function encPackedUdvt(U u) external pure returns (bytes memory) { return abi.encodePacked(u); }
    type U is int24;
    function encUdvt(U u) external pure returns (bytes memory) { return abi.encode(u); }
    function decUdvt(bytes calldata d) external pure returns (U) { return abi.decode(d, (U)); }
    function decStruct(bytes calldata d) external pure returns (S memory) { return abi.decode(d, (S)); }
    function decStructMem(bytes memory d) external pure returns (S memory) { return abi.decode(d, (S)); }
    function decDyn(bytes calldata d) external pure returns (uint256, uint256, uint256) { D memory x = abi.decode(d, (D)); return (x.a, x.b.length, x.s.length); }
    function decNested(bytes calldata d) external pure returns (uint8, uint256) { N memory n = abi.decode(d, (N)); return (n.fixedS[1].a, n.grid[1][0]); }
    function decMulti(bytes calldata d) external pure returns (uint8, string memory, uint8[] memory, bytes3) { return abi.decode(d, (uint8, string, uint8[], bytes3)); }
    function decStaticArr(bytes calldata d) external pure returns (uint8[3] memory) { return abi.decode(d, (uint8[3])); }
    function decStaticArrStruct(bytes calldata d) external pure returns (S[2] memory) { return abi.decode(d, (S[2])); }
    function decBool(bytes calldata d) external pure returns (bool) { return abi.decode(d, (bool)); }
    function decEnum(bytes calldata d) external pure returns (E) { return abi.decode(d, (E)); }
    function decAddr(bytes calldata d) external pure returns (address) { return abi.decode(d, (address)); }
    function decI8(bytes calldata d) external pure returns (int8) { return abi.decode(d, (int8)); }
    function decB4(bytes calldata d) external pure returns (bytes4) { return abi.decode(d, (bytes4)); }
    function roundTrip(uint8 a, bytes2 b, int16 c) external pure returns (S memory) { return abi.decode(abi.encode(S(a, b, c)), (S)); }
    function roundTripDyn(uint256 a, bytes calldata b) external pure returns (uint256, bytes memory) { D memory d = abi.decode(abi.encode(D(a, b, new S[](1))), (D)); return (d.a, d.b); }
    function encLen(uint8 n) external pure returns (uint256, uint256) { require(n < 5); uint8[] memory a = new uint8[](n); return (abi.encode(a).length, abi.encodePacked(a).length); }
    function selectors() external pure returns (bytes4, bytes4, bytes4) { return (this.encSel.selector, this.target.selector, this.decStruct.selector); }
    function selectorConst() external pure returns (bytes4) { return bytes4(keccak256("target(uint8,int8)")); }
    function encWithSelFromConst(uint8 a) external pure returns (bytes memory) { return abi.encodeWithSelector(bytes4(keccak256("target(uint8,int8)")), a, int8(-1)); }
    function encDirty(uint256 raw) external pure returns (bytes memory) { uint8 a; bytes2 b; int16 c; bool d; assembly { a := raw b := raw c := raw d := raw } return abi.encode(a, b, c, d); }
    function encPackedDirty(uint256 raw) external pure returns (bytes memory) { uint8 a; bytes2 b; int16 c; bool d; assembly { a := raw b := raw c := raw d := raw } return abi.encodePacked(a, b, c, d); }
    function encSelDirty(uint256 raw) external pure returns (bytes memory) { uint8 a; int8 c; assembly { a := raw c := raw } return abi.encodeWithSelector(0xaabbccdd, a, c); }
    function encStructDirty(uint256 raw) external pure returns (bytes memory) { S memory s; assembly { mstore(s, raw) mstore(add(s, 32), raw) mstore(add(s, 64), raw) } return abi.encode(s); }
    function encArrDirty(uint256 raw) external pure returns (bytes memory) { uint8[] memory a = new uint8[](2); assembly { mstore(add(a, 32), raw) mstore(add(a, 64), raw) } return abi.encode(a); }
    function encPackedArrDirty(uint256 raw) external pure returns (bytes memory) { int8[] memory a = new int8[](2); assembly { mstore(add(a, 32), raw) mstore(add(a, 64), raw) } return abi.encodePacked(a); }
    function encStaticDirty(uint256 raw) external pure returns (bytes memory) { bool[2] memory a; assembly { mstore(a, raw) mstore(add(a, 32), raw) } return abi.encode(a); }
    function hashEnc(uint8 a, string calldata s) external pure returns (bytes32, bytes32) { return (keccak256(abi.encode(a, s)), keccak256(abi.encodePacked(a, s))); }
    function hashStr(string calldata s) external pure returns (bytes32, bytes32) { return (keccak256(bytes(s)), keccak256(abi.encodePacked(s))); }
    function hashLit() external pure returns (bytes32, bytes32, bytes32) { return (keccak256(""), keccak256("abc"), keccak256(abi.encodePacked(uint256(1)))); }
    function sha(bytes calldata d) external pure returns (bytes32, bytes20) { return (sha256(d), ripemd160(d)); }
    function shaLit() external pure returns (bytes32, bytes20) { return (sha256(""), ripemd160("abc")); }
    function ecrec(bytes32 h, uint8 v, bytes32 r, bytes32 s) external pure returns (address) { return ecrecover(h, v, r, s); }
    function encTwoDyn(bytes calldata a, bytes calldata b) external pure returns (bytes memory) { return abi.encode(a, b); }
    function encStrArr(string[] calldata a) external pure returns (bytes memory) { return abi.encode(a); }
    function decStrArr(bytes calldata d) external pure returns (string[] memory) { return abi.decode(d, (string[])); }
    function encBytesArr2(bytes[2] calldata a) external pure returns (bytes memory) { return abi.encode(a); }
    function decBytesArr2(bytes calldata d) external pure returns (bytes[2] memory) { return abi.decode(d, (bytes[2])); }
    function enc2dDyn(uint8[][] calldata a) external pure returns (bytes memory) { return abi.encode(a); }
    function encMemFromCd(uint8[][] calldata a) external pure returns (bytes memory) { uint8[][] memory m = a; return abi.encode(m); }
}
