contract MemoryBytesOps {
    function copyLoop(bytes calldata d) external pure returns (bytes memory out) { out = new bytes(d.length); for (uint256 i; i < d.length; i++) out[i] = d[i]; }
    function reverse(bytes memory d) external pure returns (bytes memory) { uint256 n = d.length; for (uint256 i; i < n / 2; i++) { (d[i], d[n - 1 - i]) = (d[n - 1 - i], d[i]); } return d; }
    function upper(bytes memory d) external pure returns (bytes memory) { for (uint256 i; i < d.length; i++) { if (d[i] >= 0x61 && d[i] <= 0x7a) d[i] = bytes1(uint8(d[i]) - 32); } return d; }
    function sliceMem(bytes memory d, uint256 s, uint256 e) external pure returns (bytes memory r) { require(s <= e && e <= d.length); r = new bytes(e - s); for (uint256 i = s; i < e; i++) r[i - s] = d[i]; }
    function sliceAsm(bytes memory d, uint256 s, uint256 e) external pure returns (bytes memory r) { require(s <= e && e <= d.length); r = new bytes(e - s); assembly { mcopy(add(r, 32), add(add(d, 32), s), sub(e, s)) } }
    function concat3(bytes memory a, bytes memory b, bytes memory c) external pure returns (bytes memory) { return bytes.concat(a, b, c); }
    function concatPacked(bytes memory a, bytes memory b) external pure returns (bytes memory) { return abi.encodePacked(a, b); }
    function concatEq(bytes memory a, bytes memory b) external pure returns (bool) { return keccak256(bytes.concat(a, b)) == keccak256(abi.encodePacked(a, b)); }
    function wordRead(bytes memory d, uint256 off) external pure returns (bytes32 w) { require(off + 32 <= d.length); assembly { w := mload(add(add(d, 32), off)) } }
    function wordWrite(bytes memory d, uint256 off, bytes32 w) external pure returns (bytes memory) { require(off + 32 <= d.length); assembly { mstore(add(add(d, 32), off), w) } return d; }
    function wordWriteTail(bytes memory d, bytes32 w) external pure returns (bytes memory, uint256) { assembly { mstore(add(add(d, 32), mload(d)), w) } uint256[] memory n = new uint256[](1); return (d, n.length); }
    function lenWriteAsm(bytes memory d, uint256 n) external pure returns (bytes memory) { require(n <= d.length); assembly { mstore(d, n) } return d; }
    function lenGrowAsm(bytes memory d) external pure returns (uint256, bytes1) { assembly { mstore(d, add(mload(d), 1)) } return (d.length, d[d.length - 1]); }
    function toU256Array(bytes memory d) external pure returns (uint256[] memory r) { require(d.length % 32 == 0); r = new uint256[](d.length / 32); for (uint256 i; i < r.length; i++) { bytes32 w; assembly { w := mload(add(add(d, 32), mul(i, 32))) } r[i] = uint256(w); } }
    function fromU256Array(uint256[] memory a) external pure returns (bytes memory r) { r = new bytes(a.length * 32); for (uint256 i; i < a.length; i++) { bytes32 w = bytes32(a[i]); assembly { mstore(add(add(r, 32), mul(i, 32)), w) } } }
    function padRight(bytes memory d) external pure returns (bytes memory r) { uint256 n = (d.length + 31) / 32 * 32; r = new bytes(n); for (uint256 i; i < d.length; i++) r[i] = d[i]; }
    function zeroTail(bytes memory d) external pure returns (bytes32 w) { assembly { w := mload(add(add(d, 32), mload(d))) } }
    function zeroTailAfterAlloc(bytes memory d) external pure returns (bytes32 w) { bytes memory e = new bytes(3); e[0] = 0xff; assembly { w := mload(add(add(d, 32), mload(d))) } }
    function newBytesZero(uint256 n) external pure returns (bool) { require(n < 200); bytes memory b = new bytes(n); for (uint256 i; i < n; i++) if (b[i] != 0) return false; return true; }
    function dirtyThenNew(uint256 n) external pure returns (bool) { require(n < 100); assembly { let p := mload(0x40) mstore(p, not(0)) mstore(add(p, 32), not(0)) mstore(add(p, 64), not(0)) mstore(add(p, 96), not(0)) } bytes memory b = new bytes(n); for (uint256 i; i < n; i++) if (b[i] != 0) return false; return true; }
    function dirtyThenNewArr(uint256 n) external pure returns (bool) { require(n < 4); assembly { let p := mload(0x40) mstore(p, not(0)) mstore(add(p, 32), not(0)) mstore(add(p, 64), not(0)) mstore(add(p, 96), not(0)) mstore(add(p, 128), not(0)) } uint256[] memory b = new uint256[](n); for (uint256 i; i < n; i++) if (b[i] != 0) return false; return true; }
    function dirtyThenStruct(uint256 x) external pure returns (uint256, uint256, uint256) { assembly { let p := mload(0x40) mstore(p, not(0)) mstore(add(p, 32), not(0)) mstore(add(p, 64), not(0)) mstore(add(p, 96), not(0)) } S memory s; s.a = x; return (s.a, s.b, s.arr.length); }
    struct S { uint256 a; uint256 b; uint256[] arr; }
    function dirtyThenStaticArr(uint256 x) external pure returns (uint256, uint256, uint256) { assembly { let p := mload(0x40) mstore(p, not(0)) mstore(add(p, 32), not(0)) mstore(add(p, 64), not(0)) } uint256[3] memory a; a[1] = x; return (a[0], a[1], a[2]); }
    function dirtyThenEncode(uint256 x) external pure returns (bytes memory) { assembly { let p := mload(0x40) mstore(p, not(0)) mstore(add(p, 32), not(0)) mstore(add(p, 64), not(0)) mstore(add(p, 96), not(0)) mstore(add(p, 128), not(0)) } return abi.encode(uint8(x), true, bytes1(uint8(x))); }
    function dirtyThenConcat(uint8 x) external pure returns (bytes memory) { assembly { let p := mload(0x40) mstore(p, not(0)) mstore(add(p, 32), not(0)) mstore(add(p, 64), not(0)) } return bytes.concat(bytes1(x), hex"aa"); }
    function dirtyThenString(uint256 x) external pure returns (string memory) { assembly { let p := mload(0x40) mstore(p, not(0)) mstore(add(p, 32), not(0)) mstore(add(p, 64), not(0)) } return x > 0 ? "short" : "a longer string literal exceeding thirty-two bytes"; }
    function dirtyThenReturnNarrow(uint256 x) external pure returns (uint8, bool, bytes2) { assembly { mstore(0x80, not(0)) mstore(0xa0, not(0)) mstore(0xc0, not(0)) } return (uint8(x), x > 5, bytes2(uint16(x))); }
    function dirtyScratchThenHash(uint256 x) external pure returns (bytes32) { assembly { mstore(0, not(0)) mstore(0x20, not(0)) } return keccak256(abi.encodePacked(uint8(x))); }
    function dirtyScratchThenMapping(uint256 x) external returns (uint256) { assembly { mstore(0, not(0)) mstore(0x20, not(0)) } m[x] = 5; return m[x]; }
    mapping(uint256 => uint256) m;
    function dirtyZeroSlot(uint256 x) external pure returns (uint256) { assembly { mstore(0x60, not(0)) } uint256[] memory e; return e.length + x; }
    function dirtyZeroSlotBytes(uint256 x) external pure returns (uint256) { assembly { mstore(0x60, not(0)) } bytes memory e; return e.length + x; }
    function dirtyZeroSlotString() external pure returns (string memory) { assembly { mstore(0x60, not(0)) } string memory e; return e; }
    function dirtyZeroSlotStruct() external pure returns (uint256) { assembly { mstore(0x60, not(0)) } S memory s; return s.arr.length; }
    function emptyArrayPtr() external pure returns (uint256 p1, uint256 p2) { uint256[] memory a; bytes memory b; assembly { p1 := a p2 := b } }
    function emptyArrayPtrEq() external pure returns (bool, bool) { uint256[] memory a; bytes memory b; string memory c; uint256 pa; uint256 pb; uint256 pq; assembly { pa := a pb := b pq := c } return (pa == pb, pb == pq); }
    function emptyArrayLen() external pure returns (uint256, uint256, uint256) { uint256[] memory a; bytes memory b; string memory c; return (a.length, b.length, bytes(c).length); }
    function emptyArrayIdx() external pure returns (uint256) { uint256[] memory a; return a[0]; }
    function emptyArrayEnc() external pure returns (bytes memory) { uint256[] memory a; bytes memory b; return abi.encode(a, b); }
    function emptyStructEnc() external pure returns (bytes memory) { S memory s; return abi.encode(s); }
    function largeAlloc(uint256 n) external pure returns (uint256) { require(n <= 4096); bytes memory b = new bytes(n); return b.length; }
    function largeAllocLoop(uint256 n) external pure returns (uint256 total) { require(n < 20); for (uint256 i; i < n; i++) { bytes memory b = new bytes(i * 100); total += b.length; } }
    function memPtrAfterAlloc(uint256 n) external pure returns (uint256 delta) { require(n < 200); uint256 p0; uint256 p1; assembly { p0 := mload(0x40) } bytes memory b = new bytes(n); assembly { p1 := mload(0x40) } delta = p1 - p0 + b.length * 0; }
    function memPtrAfterAllocArr(uint256 n) external pure returns (uint256 delta) { require(n < 20); uint256 p0; uint256 p1; assembly { p0 := mload(0x40) } uint256[] memory b = new uint256[](n); assembly { p1 := mload(0x40) } delta = p1 - p0 + b.length * 0; }
    function memPtrAfterConcat(bytes calldata d) external pure returns (uint256 delta) { uint256 p0; uint256 p1; assembly { p0 := mload(0x40) } bytes memory b = bytes.concat(d, d); assembly { p1 := mload(0x40) } delta = p1 - p0 + b.length * 0; }
    function memPtrAfterEncode(uint256 x) external pure returns (uint256 delta) { uint256 p0; uint256 p1; assembly { p0 := mload(0x40) } bytes memory b = abi.encode(x, x); assembly { p1 := mload(0x40) } delta = p1 - p0 + b.length * 0; }
    function memPtrAfterStruct() external pure returns (uint256 delta) { uint256 p0; uint256 p1; assembly { p0 := mload(0x40) } S memory s; assembly { p1 := mload(0x40) } delta = p1 - p0 + s.a; }
    function memPtrAfterString() external pure returns (uint256 delta) { uint256 p0; uint256 p1; assembly { p0 := mload(0x40) } string memory s = "hello"; assembly { p1 := mload(0x40) } delta = p1 - p0 + bytes(s).length * 0; }
    function memPtrAfterCdCopy(uint256[] calldata a) external pure returns (uint256 delta) { uint256 p0; uint256 p1; assembly { p0 := mload(0x40) } uint256[] memory b = a; assembly { p1 := mload(0x40) } delta = p1 - p0 + b.length * 0; }
    function memPtrAfterStorageCopy() external returns (uint256 delta) { sarr.push(1); sarr.push(2); uint256 p0; uint256 p1; assembly { p0 := mload(0x40) } uint256[] memory b = sarr; assembly { p1 := mload(0x40) } delta = p1 - p0 + b.length * 0; }
    uint256[] sarr;
    function bytesCmpLoop(bytes memory a, bytes memory b) external pure returns (bool) { if (a.length != b.length) return false; for (uint256 i; i < a.length; i++) if (a[i] != b[i]) return false; return true; }
    function bytesFind(bytes memory h, bytes1 n) external pure returns (int256) { for (uint256 i; i < h.length; i++) if (h[i] == n) return int256(i); return -1; }
    function bytesCount(bytes calldata h, bytes1 n) external pure returns (uint256 c) { for (uint256 i; i < h.length; i++) if (h[i] == n) c++; }
    function bytesSum(bytes calldata h) external pure returns (uint256 s) { for (uint256 i; i < h.length; i++) s += uint8(h[i]); }
    function bytesXor(bytes memory a, bytes memory b) external pure returns (bytes memory) { require(a.length == b.length); for (uint256 i; i < a.length; i++) a[i] ^= b[i]; return a; }
    function bytesShiftLeft1(bytes memory a) external pure returns (bytes memory) { for (uint256 i; i + 1 < a.length; i++) a[i] = a[i + 1]; if (a.length > 0) a[a.length - 1] = 0; return a; }
    function bytesToHex(bytes memory d) external pure returns (string memory) { bytes memory hexChars = "0123456789abcdef"; bytes memory r = new bytes(d.length * 2); for (uint256 i; i < d.length; i++) { r[2 * i] = hexChars[uint8(d[i]) >> 4]; r[2 * i + 1] = hexChars[uint8(d[i]) & 0x0f]; } return string(r); }
}
