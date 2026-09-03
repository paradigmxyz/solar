contract ArraysMemory {
    struct S { uint256 a; uint256[] arr; bytes b; }
    struct T { S s; uint8 x; }
    uint256[] sarr; uint8[] s8; S ss; S[] sss; uint256[3] sfix; uint256[][] s2d;
    function newNested(uint256 n, uint256 m) external pure returns (uint256, uint256, uint256) {
        require(n < 4 && m < 4);
        uint256[][] memory a = new uint256[][](n);
        for (uint256 i; i < n; i++) { a[i] = new uint256[](m); for (uint256 j; j < m; j++) a[i][j] = i * 10 + j; }
        return (a.length, n > 0 ? a[n - 1].length : 0, n > 0 && m > 0 ? a[n - 1][m - 1] : 0);
    }
    function refSemantics(uint256 v) external pure returns (uint256, uint256) {
        uint256[] memory a = new uint256[](2); uint256[] memory b = a; b[0] = v; a[1] = v + 1; return (a[0], b[1]);
    }
    function structNested(uint256 v) external pure returns (uint256, uint256, uint256) {
        T memory t; t.s.arr = new uint256[](2); t.s.arr[1] = v; t.s.b = "xy"; t.x = 7;
        T memory u = t; u.s.a = 3; return (t.s.a, t.s.arr[1], t.s.b.length + u.x);
    }
    function literalTypes() external pure returns (uint256, uint8, uint16) {
        uint8[3] memory a = [1, 2, 3]; uint16[2] memory b = [uint16(300), 4]; return (a.length, a[2], b[0]);
    }
    function literalWiden() external pure returns (uint256[3] memory) { return [uint256(1), 2, 3]; }
    function literalOfLiterals(uint256 i) external pure returns (uint256) { return [uint256(10), 20, 30][i]; }
    function literalStrings(uint256 i) external pure returns (string memory) { string[3] memory a = ["a", "bb", "ccc"]; return a[i]; }
    function literalBytes(uint256 i) external pure returns (bytes memory) { bytes[2] memory a = [bytes("abc"), bytes(hex"0102")]; return a[i]; }
    function multiDim(uint256 i, uint256 j) external pure returns (uint256) { uint256[2][3] memory a; a[i][j] = 1; uint256 s; for (uint256 x; x < 3; x++) for (uint256 y; y < 2; y++) s += a[x][y] * (x * 2 + y + 1); return s; }
    function multiDimLen() external pure returns (uint256, uint256) { uint256[2][3] memory a; return (a.length, a[0].length); }
    function multiDimDyn(uint256 n) external pure returns (uint256) { require(n < 4); uint256[2][] memory a = new uint256[2][](n); if (n > 0) a[n - 1][1] = 9; return n > 0 ? a[n - 1][1] + a.length : a.length; }
    function memToStorageU8(uint8[] memory m) external returns (uint256 r, uint256 len) { s8 = m; uint256 s; assembly { mstore(0, s8.slot) s := keccak256(0, 32) r := sload(s) } return (r, s8.length); }
    function cdToStorageU8(uint8[] calldata m) external returns (uint256 r, uint256 len) { s8 = m; uint256 s; assembly { mstore(0, s8.slot) s := keccak256(0, 32) r := sload(s) } return (r, s8.length); }
    function storageToMem(uint256 n) external returns (uint256[] memory) { require(n < 5); for (uint256 i; i < n; i++) sarr.push(i + 1); return sarr; }
    function storageStructToMem(uint256 v) external returns (S memory) { ss.a = v; ss.arr.push(v); ss.arr.push(v + 1); ss.b = "hello world, this is longer than 32 bytes!!"; return ss; }
    function memStructToStorage(uint256 v) external returns (uint256, uint256, uint256) { S memory m = S(v, new uint256[](3), "abc"); m.arr[2] = v; ss = m; return (ss.a, ss.arr[2], ss.b.length); }
    function storageArrOfStruct(uint256 v) external returns (uint256, uint256) { sss.push(); sss[0].arr.push(v); sss.push(S(v, new uint256[](1), "")); return (sss[0].arr[0], sss[1].a + sss.length); }
    function popEmpty() external returns (uint256) { sarr.pop(); return sarr.length; }
    function popStruct(uint256 v) external returns (uint256, uint256) { sss.push(S(v, new uint256[](2), "abcdef")); sss.pop(); sss.push(); return (sss[0].a, sss[0].arr.length + sss[0].b.length); }
    function idxOOB(uint256 i) external pure returns (uint256) { uint256[] memory a = new uint256[](3); return a[i]; }
    function idxOOBStatic(uint256 i) external pure returns (uint256) { uint256[3] memory a; a[1] = 5; return a[i]; }
    function idxOOBStorage(uint256 i) external returns (uint256) { sarr.push(1); return sarr[i]; }
    function idxOOBFixedStorage(uint256 i) external returns (uint256) { sfix[1] = 1; return sfix[i]; }
    function deleteStorage(uint256 n) external returns (uint256, uint256) { require(n < 5); for (uint256 i; i < n; i++) sarr.push(i); delete sarr; uint256 s; uint256 r; assembly { mstore(0, sarr.slot) s := keccak256(0, 32) r := sload(s) } return (sarr.length, r); }
    function deleteElem(uint256 v) external returns (uint256, uint256) { sarr.push(v); sarr.push(v); delete sarr[0]; return (sarr[0], sarr[1]); }
    function deleteMem(uint256 v) external pure returns (uint256, uint256) { uint256[] memory a = new uint256[](2); a[0] = v; a[1] = v; delete a[0]; uint256[] memory b = a; delete b; return (a[0] + a[1], b.length); }
    function deleteMemStruct(uint256 v) external pure returns (uint256, uint256, uint256) { S memory m = S(v, new uint256[](2), "ab"); delete m; return (m.a, m.arr.length, m.b.length); }
    function push2d(uint256 v) external returns (uint256, uint256) { s2d.push(); s2d[0].push(v); s2d.push(s2d[0]); s2d[1].push(v + 1); return (s2d[0].length, s2d[1][1]); }
    function copyStorageArr(uint256 v) external returns (uint256, uint256) { sarr.push(v); sarr.push(v + 1); uint256[] memory m = sarr; m[0] = 0; sarr = m; sarr.push(7); return (sarr[0] + sarr[1], sarr.length); }
    function fixedStorageCopy(uint256 v) external returns (uint256[3] memory) { sfix = [v, v + 1, v + 2]; uint256[3] memory m = sfix; m[0]++; sfix = m; return sfix; }
    function memArrayArg(uint256[] memory a) external pure returns (uint256) { return a.length; }
    function structArrayMem(uint256 n) external pure returns (uint256 s) { require(n < 4); S[] memory a = new S[](n); for (uint256 i; i < n; i++) { a[i].a = i; a[i].arr = new uint256[](i); } for (uint256 i; i < n; i++) s += a[i].a + a[i].arr.length; }
    function lenAfterAssign(uint256 n) external pure returns (uint256) { require(n < 5); uint256[] memory a = new uint256[](n); uint256[] memory b = new uint256[](1); a = b; return a.length; }
    function encodeNested(uint256 v) external pure returns (bytes memory) { uint256[][] memory a = new uint256[][](2); a[0] = new uint256[](1); a[0][0] = v; return abi.encode(a); }
    function decodeNested(bytes calldata d) external pure returns (uint256) { uint256[][] memory a = abi.decode(d, (uint256[][])); return a.length > 0 && a[0].length > 0 ? a[0][0] : 0; }
    function cdArrIdx(uint256[] calldata a, uint256 i) external pure returns (uint256) { return a[i]; }
    function cdArrStructIdx(S[] calldata a, uint256 i) external pure returns (uint256) { return a[i].a + a[i].arr.length; }
    function cd2dIdx(uint256[][] calldata a, uint256 i, uint256 j) external pure returns (uint256) { return a[i][j]; }
    function cdStaticOfDyn(uint256[][2] calldata a) external pure returns (uint256) { return a[0].length + a[1].length; }
    function cdDynOfStatic(uint256[2][] calldata a, uint256 i) external pure returns (uint256) { return a[i][0] + a[i][1]; }
    function memArrCopyCd(uint256[2][] calldata a) external pure returns (uint256) { uint256[2][] memory m = a; return m.length > 0 ? m[0][1] : 0; }
    function returnStaticMem(uint256 v) external pure returns (uint256[2] memory r) { r[1] = v; }
    function returnMultiArr(uint256 v) external pure returns (uint256[] memory a, bytes memory b, uint256[2] memory c) { a = new uint256[](1); a[0] = v; b = hex"ff"; c[0] = v; }
    function swapElems(uint256[] memory a) external pure returns (uint256[] memory) { require(a.length >= 2); (a[0], a[1]) = (a[1], a[0]); return a; }
    function swapStorage(uint256 x, uint256 y) external returns (uint256, uint256) { sarr.push(x); sarr.push(y); (sarr[0], sarr[1]) = (sarr[1], sarr[0]); return (sarr[0], sarr[1]); }
    function arrEq(uint256[] memory a, uint256[] memory b) external pure returns (bool) { return keccak256(abi.encodePacked(a)) == keccak256(abi.encodePacked(b)); }
    function bytesEq(bytes memory a, bytes memory b) external pure returns (bool) { return keccak256(a) == keccak256(b); }
    function structCd(S calldata s) external pure returns (uint256) { return s.a + s.arr.length + s.b.length; }
    function structCdToMem(S calldata s) external pure returns (S memory) { return s; }
    function structTCd(T calldata t) external pure returns (uint256) { return t.s.a + t.x; }
    function newZero() external pure returns (uint256, uint256) { uint256[] memory a = new uint256[](0); bytes memory b = new bytes(0); return (a.length, b.length); }
    function newBytes(uint256 n) external pure returns (uint256, bytes1) { require(n < 100); bytes memory b = new bytes(n); if (n > 0) b[n - 1] = 0x01; return (b.length, n > 0 ? b[0] : bytes1(0)); }
    function newString(uint256 n) external pure returns (uint256) { require(n < 100); string memory s = new string(n); return bytes(s).length; }
    function memBytesPop(bytes memory b) external pure returns (uint256) { return b.length; }
    function bytesStorage(bytes calldata d) external returns (uint256, bytes1) { ss.b = d; ss.b.push(0x42); return (ss.b.length, ss.b[ss.b.length - 1]); }
    function bytesStoragePop(bytes calldata d) external returns (uint256) { ss.b = d; ss.b.pop(); return ss.b.length; }
    function bytesStorageIdx(bytes calldata d, uint256 i) external returns (bytes1) { ss.b = d; ss.b[i] = 0xaa; return ss.b[i]; }
}
