contract DirtyAbiDecode {
    function decodeUint8(uint256 raw) external pure returns (uint8) { return abi.decode(abi.encode(raw), (uint8)); }
    function decodeInt8(uint256 raw) external pure returns (int8) { return abi.decode(abi.encode(raw), (int8)); }
    function decodeBool(uint256 raw) external pure returns (bool) { return abi.decode(abi.encode(raw), (bool)); }
    function decodeAddress(uint256 raw) external pure returns (address) { return abi.decode(abi.encode(raw), (address)); }
    function decodeBytes4(uint256 raw) external pure returns (bytes4) { return abi.decode(abi.encode(raw), (bytes4)); }
    function decodeEnum(uint256 raw) external pure returns (E) { return abi.decode(abi.encode(raw), (E)); }
    function decodeUdvt(uint256 raw) external pure returns (Small) { return abi.decode(abi.encode(raw), (Small)); }
    function decodeTuple(uint256 raw, uint256 raw2) external pure returns (uint8, bool) { return abi.decode(abi.encode(raw, raw2), (uint8, bool)); }
    function decodeStaticArray(uint256 raw) external pure returns (uint8[2] memory) { return abi.decode(abi.encode(raw, raw), (uint8[2])); }
    function decodeStruct(uint256 raw) external pure returns (S memory) { return abi.decode(abi.encode(raw, raw), (S)); }
    function decodeDynArray(uint256 raw) external pure returns (uint8[] memory) { return abi.decode(abi.encode(uint256(32), uint256(1), raw), (uint8[])); }
    function calldataUint8(uint8 x) external pure returns (uint256 r) { assembly { r := x } }
    function calldataInt8(int8 x) external pure returns (uint256 r) { assembly { r := x } }
    function calldataBool(bool x) external pure returns (uint256 r) { assembly { r := x } }
    function calldataBytes4(bytes4 x) external pure returns (uint256 r) { assembly { r := x } }
    function calldataStruct(S calldata s) external pure returns (uint256 r) { uint8 a = s.a; assembly { r := a } }
    function calldataArray(uint8[] calldata a) external pure returns (uint256 r) { uint8 x = a[0]; assembly { r := x } }
    function calldataArrayLen(uint8[] calldata a) external pure returns (uint256) { return a.length; }
    function memoryArrayDirty(uint256 raw) external pure returns (uint8, uint256 w) { uint8[] memory a = new uint8[](2); assembly { mstore(add(a, 0x20), raw) } uint8 v = a[0]; assembly { w := mload(add(a, 0x20)) } return (v, w); }
    function memoryStructDirty(uint256 raw) external pure returns (uint8, bool) { S memory s; assembly { mstore(s, raw) mstore(add(s, 0x20), raw) } return (s.a, s.b); }
    function memoryStructEncode(uint256 raw) external pure returns (bytes memory) { S memory s; assembly { mstore(s, raw) mstore(add(s, 0x20), raw) } return abi.encode(s); }
    function memoryArrayEncode(uint256 raw) external pure returns (bytes memory) { uint8[] memory a = new uint8[](1); assembly { mstore(add(a, 0x20), raw) } return abi.encode(a); }
    function memoryArrayEncodePacked(uint256 raw) external pure returns (bytes memory) { uint8[] memory a = new uint8[](1); assembly { mstore(add(a, 0x20), raw) } return abi.encodePacked(a); }
    function memoryArrayCopyToStorage(uint256 raw) external returns (uint8, uint256 slot) { uint8[] memory a = new uint8[](1); assembly { mstore(add(a, 0x20), raw) } sa = a; uint8 v = sa[0]; assembly { slot := sload(add(keccak256(0, 0), 0)) } slot = 0; assembly { mstore(0, sa.slot) slot := sload(keccak256(0, 0x20)) } return (v, slot); }
    function bytesDirtyTail(uint256 raw) external pure returns (bytes memory) { bytes memory b = new bytes(1); assembly { mstore(add(b, 0x20), raw) } return b; }
    function bytesDirtyTailHash(uint256 raw) external pure returns (bytes32) { bytes memory b = new bytes(1); assembly { mstore(add(b, 0x20), raw) } return keccak256(b); }
    function bytesDirtyTailToStorage(uint256 raw) external returns (bytes memory) { bytes memory b = new bytes(1); assembly { mstore(add(b, 0x20), raw) } sb = b; return sb; }
    function stringDirtyEq(uint256 raw) external pure returns (bool) { bytes memory b = new bytes(1); assembly { mstore(add(b, 0x20), raw) } bytes memory c = new bytes(1); assembly { mstore(add(c, 0x20), shl(248, shr(248, raw))) } return keccak256(b) == keccak256(c); }

    enum E { A, B, C }
    struct S { uint8 a; bool b; }
    uint8[] sa;
    bytes sb;
}
type Small is uint8;
