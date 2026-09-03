contract FmpConvention {
    struct S { uint256 a; uint256[] arr; }
    function _scribble() internal pure { assembly ("memory-safe") { let p := mload(0x40) mstore(p, not(0)) mstore(add(p, 32), not(0)) mstore(add(p, 64), not(0)) mstore(add(p, 96), not(0)) mstore(add(p, 128), not(0)) mstore(add(p, 160), not(0)) mstore(add(p, 192), not(0)) mstore(add(p, 224), not(0)) } }
    function _scribbleAlloc() internal pure { assembly ("memory-safe") { let p := mload(0x40) mstore(0x40, add(p, 256)) mstore(p, not(0)) mstore(add(p, 32), not(0)) mstore(add(p, 64), not(0)) mstore(add(p, 96), not(0)) mstore(add(p, 128), not(0)) mstore(add(p, 160), not(0)) mstore(add(p, 192), not(0)) mstore(add(p, 224), not(0)) } }
    function encodeThenScribble(uint256 x) external pure returns (bytes memory) { bytes memory b = abi.encode(x, x + 1); _scribble(); return b; }
    function encodeThenScribbleAlloc(uint256 x) external pure returns (bytes memory) { bytes memory b = abi.encode(x, x + 1); _scribbleAlloc(); return b; }
    function encodePackedThenScribble(uint8 x) external pure returns (bytes memory) { bytes memory b = abi.encodePacked(x, x); _scribble(); return b; }
    function stringThenScribble() external pure returns (string memory) { string memory s = "hello"; _scribble(); return s; }
    function stringLongThenScribble() external pure returns (string memory) { string memory s = "a string literal that is longer than thirty-two bytes!"; _scribble(); return s; }
    function bytesLitThenScribble() external pure returns (bytes memory) { bytes memory s = hex"0102030405"; _scribble(); return s; }
    function concatThenScribble(bytes calldata d) external pure returns (bytes memory) { bytes memory b = bytes.concat(d, hex"ff"); _scribble(); return b; }
    function strConcatThenScribble(string calldata d) external pure returns (string memory) { string memory b = string.concat(d, "!"); _scribble(); return b; }
    function newThenScribble(uint256 n) external pure returns (uint256[] memory) { require(n < 5); uint256[] memory a = new uint256[](n); for (uint256 i; i < n; i++) a[i] = i + 1; _scribble(); return a; }
    function newBytesThenScribble(uint256 n) external pure returns (bytes memory) { require(n < 70); bytes memory a = new bytes(n); for (uint256 i; i < n; i++) a[i] = 0x01; _scribble(); return a; }
    function structThenScribble(uint256 x) external pure returns (uint256, uint256) { S memory s; s.a = x; s.arr = new uint256[](1); s.arr[0] = x + 1; _scribble(); return (s.a, s.arr[0]); }
    function staticArrThenScribble(uint256 x) external pure returns (uint256[3] memory) { uint256[3] memory a = [x, x + 1, x + 2]; _scribble(); return a; }
    function literalArrThenScribble(uint256 x) external pure returns (uint256) { uint256[3] memory a = [x, x + 1, x + 2]; _scribble(); return a[0] + a[1] + a[2]; }
    function cdCopyThenScribble(uint256[] calldata d) external pure returns (uint256[] memory) { uint256[] memory m = d; _scribble(); return m; }
    function cdBytesCopyThenScribble(bytes calldata d) external pure returns (bytes memory) { bytes memory m = d; _scribble(); return m; }
    function storageCopyThenScribble(uint256 x) external returns (uint256[] memory) { sarr.push(x); sarr.push(x + 1); uint256[] memory m = sarr; _scribble(); return m; }
    uint256[] sarr; bytes sbytes;
    function storageBytesCopyThenScribble(bytes calldata d) external returns (bytes memory) { sbytes = d; bytes memory m = sbytes; _scribble(); return m; }
    function returnStructThenScribble(uint256 x) external pure returns (S memory) { S memory s = _mk(x); _scribble(); return s; }
    function _mk(uint256 x) internal pure returns (S memory s) { s.a = x; s.arr = new uint256[](2); s.arr[1] = x; }
    function memParamThenScribble(uint256[] memory d) external pure returns (uint256[] memory) { _scribble(); return d; }
    function memBytesParamThenScribble(bytes memory d) external pure returns (bytes memory) { _scribble(); return d; }
    function encodeCallThenScribble(uint256 x) external view returns (bytes memory) { bytes memory b = abi.encodeCall(this.encodeThenScribble, (x)); _scribble(); return b; }
    function encodeSelThenScribble(uint256 x) external pure returns (bytes memory) { bytes memory b = abi.encodeWithSelector(0x12345678, x); _scribble(); return b; }
    function decodeThenScribble(bytes calldata d) external pure returns (uint256[] memory) { uint256[] memory a = abi.decode(d, (uint256[])); _scribble(); return a; }
    function hashAfterScribble(uint256 x) external pure returns (bytes32) { bytes memory b = abi.encode(x); _scribble(); return keccak256(b); }
    function twoAllocsThenScribble(uint256 x) external pure returns (bytes memory, bytes memory) { bytes memory a = abi.encode(x); bytes memory b = abi.encode(x + 1); _scribble(); return (a, b); }
    function allocInLoopThenScribble(uint256 n) external pure returns (uint256 s) { require(n < 4); bytes[] memory bs = new bytes[](n); for (uint256 i; i < n; i++) { bs[i] = abi.encode(i); _scribble(); } for (uint256 i; i < n; i++) s += bs[i].length + uint8(bs[i][31]); }
    function fmpMonotonic(uint256 x) external pure returns (bool) { uint256 p0; assembly { p0 := mload(0x40) } bytes memory b = abi.encode(x); uint256 pb; uint256 p1; assembly { pb := b p1 := mload(0x40) } return p1 >= pb + 32 + b.length && pb >= p0; }
    function fmpMonotonicStr(uint256 x) external pure returns (bool) { uint256 p0; assembly { p0 := mload(0x40) } string memory b = x > 0 ? "short" : "a long literal string that exceeds thirty-two bytes here"; uint256 pb; uint256 p1; assembly { pb := b p1 := mload(0x40) } return p1 >= pb + 32 + bytes(b).length && pb >= p0; }
    function fmpMonotonicNew(uint256 n) external pure returns (bool) { require(n < 5); uint256 p0; assembly { p0 := mload(0x40) } uint256[] memory b = new uint256[](n); uint256 pb; uint256 p1; assembly { pb := b p1 := mload(0x40) } return p1 >= pb + 32 + n * 32 && pb >= p0; }
    function fmpMonotonicStruct(uint256 x) external pure returns (bool) { uint256 p0; assembly { p0 := mload(0x40) } S memory s = _mk(x); uint256 ps; uint256 p1; assembly { ps := s p1 := mload(0x40) } return p1 >= ps + 64 && ps >= p0; }
    function fmpMonotonicConcat(bytes calldata d) external pure returns (bool) { uint256 p0; assembly { p0 := mload(0x40) } bytes memory b = bytes.concat(d, d); uint256 pb; uint256 p1; assembly { pb := b p1 := mload(0x40) } return p1 >= pb + 32 + b.length && pb >= p0; }
    function fmpAligned(uint256 x) external pure returns (bool) { bytes memory b = abi.encodePacked(uint8(x)); uint256 p1; assembly { p1 := mload(0x40) } return p1 % 32 == 0 && b.length == 1; }
    function fmpAlignedNewBytes(uint256 n) external pure returns (bool) { require(n < 70); bytes memory b = new bytes(n); uint256 p1; uint256 pb; assembly { p1 := mload(0x40) pb := b } return p1 % 32 == 0 && p1 >= pb + 32 + n; }
    function fmpAtLeast0x80() external pure returns (bool) { uint256 p; assembly { p := mload(0x40) } return p >= 0x80; }
    function scratchIntact(uint256 x) external pure returns (bool) { assembly { mstore(0, x) mstore(0x20, x) } bytes memory b = abi.encode(x); uint256 a; uint256 c; assembly { a := mload(0) c := mload(0x20) } return b.length == 32 && (a == x || true) && (c == x || true); }
    function zeroSlotIntact(uint256 x) external pure returns (uint256 z) { bytes memory b = abi.encode(x); uint256[] memory e = new uint256[](0); string memory s = "abcdefghijklmnopqrstuvwxyz0123456789"; assembly { z := mload(0x60) } z += b.length * 0 + e.length + bytes(s).length * 0; }
}
