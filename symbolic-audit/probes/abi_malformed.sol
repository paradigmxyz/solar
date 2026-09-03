contract AbiMalformed {
    struct S { uint256 a; uint256[] arr; bytes b; }
    function decU256Arr(bytes calldata d) external pure returns (uint256, uint256) { uint256[] memory a = abi.decode(d, (uint256[])); return (a.length, a.length > 0 ? a[0] : 0); }
    function decBytes(bytes calldata d) external pure returns (uint256, bytes1) { bytes memory b = abi.decode(d, (bytes)); return (b.length, b.length > 0 ? b[0] : bytes1(0)); }
    function decString(bytes calldata d) external pure returns (uint256) { string memory s = abi.decode(d, (string)); return bytes(s).length; }
    function decStruct(bytes calldata d) external pure returns (uint256, uint256, uint256) { S memory s = abi.decode(d, (S)); return (s.a, s.arr.length, s.b.length); }
    function decTwoDyn(bytes calldata d) external pure returns (uint256, uint256) { (bytes memory x, bytes memory y) = abi.decode(d, (bytes, bytes)); return (x.length, y.length); }
    function decStaticArr(bytes calldata d) external pure returns (uint256) { uint256[2] memory a = abi.decode(d, (uint256[2])); return a[0] + a[1]; }
    function decNested(bytes calldata d) external pure returns (uint256) { uint256[][] memory a = abi.decode(d, (uint256[][])); return a.length; }
    function decBytesArr(bytes calldata d) external pure returns (uint256) { bytes[] memory a = abi.decode(d, (bytes[])); return a.length + (a.length > 0 ? a[0].length : 0); }
    function decU8Arr(bytes calldata d) external pure returns (uint256) { uint8[] memory a = abi.decode(d, (uint8[])); return a.length + (a.length > 0 ? a[0] : 0); }
    function decBool(bytes calldata d) external pure returns (bool) { return abi.decode(d, (bool)); }
    function decAddr(bytes calldata d) external pure returns (address) { return abi.decode(d, (address)); }
    function decShort(bytes calldata d) external pure returns (uint256) { return abi.decode(d, (uint256)); }
    function decTuple3(bytes calldata d) external pure returns (uint256) { (uint256 a, uint256 b, uint256 c) = abi.decode(d, (uint256, uint256, uint256)); return a + b + c; }
    function cdU256Arr(uint256[] calldata a) external pure returns (uint256, uint256) { return (a.length, a.length > 0 ? a[0] : 0); }
    function cdBytes(bytes calldata b) external pure returns (uint256, bytes1) { return (b.length, b.length > 0 ? b[0] : bytes1(0)); }
    function cdStruct(S calldata s) external pure returns (uint256, uint256, uint256) { return (s.a, s.arr.length, s.b.length); }
    function cdTwoDyn(bytes calldata x, bytes calldata y) external pure returns (uint256, uint256) { return (x.length, y.length); }
    function cdNested(uint256[][] calldata a) external pure returns (uint256) { return a.length + (a.length > 0 ? a[0].length : 0); }
    function cdBytesArr(bytes[] calldata a) external pure returns (uint256) { return a.length + (a.length > 0 ? a[0].length : 0); }
    function cdStructMem(S memory s) external pure returns (uint256, uint256, uint256) { return (s.a, s.arr.length, s.b.length); }
    function cdU256ArrMem(uint256[] memory a) external pure returns (uint256) { return a.length; }
    function cdStringMem(string memory s) external pure returns (uint256) { return bytes(s).length; }
    function cdUnusedDyn(uint256 x, bytes calldata) external pure returns (uint256) { return x; }
    function cdUnusedArr(uint256 x, uint256[] calldata) external pure returns (uint256) { return x; }
    function cdUnusedStruct(uint256 x, S calldata) external pure returns (uint256) { return x; }
    function cdUnusedMem(uint256 x, bytes memory) external pure returns (uint256) { return x; }
}
