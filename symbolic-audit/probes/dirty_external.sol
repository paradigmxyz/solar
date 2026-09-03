contract DirtyExternal {
    enum E { A, B, C }
    struct S { uint8 a; bool b; }
    type Small is uint8;

    function rawU8() external pure returns (uint8) { assembly { mstore(0, 0x101) return(0, 32) } }
    function rawI8() external pure returns (int8) { assembly { mstore(0, 0xff) return(0, 32) } }
    function rawBool() external pure returns (bool) { assembly { mstore(0, 2) return(0, 32) } }
    function rawAddr() external pure returns (address) { assembly { mstore(0, shl(160, 1)) return(0, 32) } }
    function rawB4() external pure returns (bytes4) { assembly { mstore(0, 1) return(0, 32) } }
    function rawEnum() external pure returns (E) { assembly { mstore(0, 3) return(0, 32) } }
    function rawSmall() external pure returns (Small) { assembly { mstore(0, 0x101) return(0, 32) } }
    function rawStruct() external pure returns (S memory) { assembly { mstore(0, 0x101) mstore(0x20, 1) return(0, 64) } }
    function rawArr() external pure returns (uint8[2] memory) { assembly { mstore(0, 1) mstore(0x20, 0x101) return(0, 64) } }
    function rawShort() external pure returns (uint256) { assembly { mstore(0, 1) return(0, 31) } }
    function rawLong() external pure returns (uint8) { assembly { mstore(0, 1) mstore(0x20, 0xffff) return(0, 64) } }
    function rawBytes() external pure returns (bytes memory) { assembly { mstore(0, 0x20) mstore(0x20, 1) mstore(0x40, not(0)) return(0, 0x60) } }
    function rawBytesBadOffset() external pure returns (bytes memory) { assembly { mstore(0, 0x40) mstore(0x20, 0) mstore(0x40, 1) mstore(0x60, 0) return(0, 0x80) } }

    function callU8() external view returns (uint8) { return this.rawU8(); }
    function callI8() external view returns (int8) { return this.rawI8(); }
    function callBool() external view returns (bool) { return this.rawBool(); }
    function callAddr() external view returns (address) { return this.rawAddr(); }
    function callB4() external view returns (bytes4) { return this.rawB4(); }
    function callEnum() external view returns (E) { return this.rawEnum(); }
    function callSmall() external view returns (Small) { return this.rawSmall(); }
    function callStruct() external view returns (uint8, bool) { S memory s = this.rawStruct(); return (s.a, s.b); }
    function callArr() external view returns (uint8, uint8) { uint8[2] memory a = this.rawArr(); return (a[0], a[1]); }
    function callShort() external view returns (uint256) { return this.rawShort(); }
    function callLong() external view returns (uint8) { return this.rawLong(); }
    function callBytes() external view returns (bytes memory) { return this.rawBytes(); }
    function callBytesLen() external view returns (uint256) { return this.rawBytes().length; }
    function callBytesBadOffset() external view returns (bytes memory) { return this.rawBytesBadOffset(); }
    function callU8Raw() external view returns (uint256 r) { uint8 v = this.rawU8(); assembly { r := v } }
    function lowLevelDecode() external view returns (uint8) { (bool ok, bytes memory ret) = address(this).staticcall(abi.encodeWithSignature("rawU8()")); require(ok); return abi.decode(ret, (uint8)); }
    function tryCall() external view returns (bool, uint8) { try this.rawU8() returns (uint8 v) { return (true, v); } catch { return (false, 0); } }

    function inU8(uint8 x) external pure returns (uint256 r) { assembly { r := x } }
    function inI8(int8 x) external pure returns (uint256 r) { assembly { r := x } }
    function inBool(bool x) external pure returns (uint256 r) { assembly { r := x } }
    function inAddr(address x) external pure returns (uint256 r) { assembly { r := x } }
    function inB4(bytes4 x) external pure returns (uint256 r) { assembly { r := x } }
    function inEnum(E x) external pure returns (uint256 r) { assembly { r := x } }
    function inSmall(Small x) external pure returns (uint256 r) { assembly { r := x } }
    function inU8Unused(uint8 x) external pure returns (uint256) { x; return 1; }
    function inU8Wide(uint8 x) external pure returns (uint256) { return x; }
    function inStruct(S calldata s) external pure returns (uint256 r) { uint8 a = s.a; assembly { r := a } }
    function inStructUnused(S calldata s) external pure returns (uint256) { s; return 1; }
    function inStructMem(S memory s) external pure returns (uint256 r) { uint8 a = s.a; assembly { r := a } }
    function inArr(uint8[] calldata a) external pure returns (uint256 r) { uint8 x = a[0]; assembly { r := x } }
    function inArrUnused(uint8[] calldata a) external pure returns (uint256) { return a.length; }
    function inArrMem(uint8[] memory a) external pure returns (uint256 r) { uint8 x = a[0]; assembly { r := x } }
    function inStaticArr(uint8[2] calldata a) external pure returns (uint256 r) { uint8 x = a[1]; assembly { r := x } }
    function inStaticArrMem(uint8[2] memory a) external pure returns (uint256 r) { uint8 x = a[1]; assembly { r := x } }
    function inBytesLen(bytes calldata b) external pure returns (uint256) { return b.length; }
    function inStringMem(string memory s) external pure returns (uint256) { return bytes(s).length; }
    function inFnAddr(function() external f) external pure returns (address) { return f.address; }
    function inTwo(uint8 x, bool y) external pure returns (uint256 r) { assembly { r := or(shl(8, x), y) } }
}
