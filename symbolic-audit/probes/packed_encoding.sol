contract PackedEncoding {
    struct S { uint8 a; uint256[] arr; string s; }
    struct T { uint8 a; bytes4 b; }
    function u8Arr(uint8[] calldata a) external pure returns (bytes memory) { return abi.encodePacked(a); }
    function u8ArrMem(uint8[] calldata a) external pure returns (bytes memory) { uint8[] memory m = a; return abi.encodePacked(m); }
    function u8Static(uint8[3] calldata a) external pure returns (bytes memory) { return abi.encodePacked(a); }
    function b4Arr(bytes4[] calldata a) external pure returns (bytes memory) { return abi.encodePacked(a); }
    function boolArr(bool[] calldata a) external pure returns (bytes memory) { return abi.encodePacked(a); }
    function addrArr(address[] calldata a) external pure returns (bytes memory) { return abi.encodePacked(a); }
    function i8Arr(int8[] calldata a) external pure returns (bytes memory) { return abi.encodePacked(a); }
    function mixed(uint8 a, bytes2 b, bool c, address d, int16 e, string calldata s, bytes calldata bs) external pure returns (bytes memory) { return abi.encodePacked(a, b, c, d, e, s, bs); }
    function nestedEncode(uint256[][] calldata a) external pure returns (bytes memory) { return abi.encode(a); }
    function structEncode(S calldata s) external pure returns (bytes memory) { return abi.encode(s); }
    function structArrEncode(T[] calldata t) external pure returns (bytes memory) { return abi.encode(t); }
    function structArrEncodePackedish(T[2] calldata t) external pure returns (bytes memory) { return abi.encode(t); }
    function stringArr(string[] calldata s) external pure returns (bytes memory) { return abi.encode(s); }
    function bytesArrArr(bytes[][] calldata s) external pure returns (bytes memory) { return abi.encode(s); }
    function encodeCallLike(uint8 a, uint8[] calldata b) external pure returns (bytes memory) { return abi.encodeWithSelector(0x01020304, a, b); }
    function encodeSig(uint8 a, string calldata s) external pure returns (bytes memory) { return abi.encodeWithSignature("f(uint8,string)", a, s); }
    function decodeRoundtrip(bytes calldata d) external pure returns (uint8, uint256[] memory, string memory) { S memory s = abi.decode(d, (S)); return (s.a, s.arr, s.s); }
    function decodeNested(bytes calldata d) external pure returns (uint256) { uint256[][] memory a = abi.decode(d, (uint256[][])); return a.length + (a.length > 0 ? a[0].length : 0); }
    function decodeStringArr(bytes calldata d) external pure returns (uint256) { string[] memory a = abi.decode(d, (string[])); return a.length + (a.length > 0 ? bytes(a[0]).length : 0); }
    function packedLiteral() external pure returns (bytes memory) { return abi.encodePacked(uint8(1), uint16(2), "ab", hex"cd", int8(-1), true, bytes3(0x010203)); }
    function packedEmpty() external pure returns (bytes memory) { return abi.encodePacked(); }
    function encodeEmptyArrays() external pure returns (bytes memory) { uint256[] memory a; string memory s; return abi.encode(a, s, new bytes(0)); }
    function concatMany(bytes calldata a, bytes calldata b) external pure returns (bytes memory) { return bytes.concat(a, b, a, hex"ff", bytes1(0x01)); }
    function stringConcat(string calldata a, string calldata b) external pure returns (string memory) { return string.concat(a, b, "x", a); }
}
