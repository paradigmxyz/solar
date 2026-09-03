contract LoopsOpt {
    uint256 st; uint256[] arr; mapping(uint256 => uint256) m;
    function hoistDiv(uint256 n, uint256 d) external pure returns (uint256 s) { for (uint256 i; i < n; i++) s += 100 / d; }
    function hoistDivZeroIter(uint256 d) external pure returns (uint256 s) { for (uint256 i; i < 0; i++) s += 100 / d; }
    function hoistMod(uint256 n, uint256 d) external pure returns (uint256 s) { for (uint256 i; i < n; i++) s += i % d; }
    function hoistOverflow(uint256 n, uint256 a) external pure returns (uint256 s) { for (uint256 i; i < n; i++) s = a + a; }
    function hoistMul(uint256 n, uint256 a) external pure returns (uint256 s) { for (uint256 i; i < n; i++) s = a * a; }
    function hoistIdx(uint256 n, uint256 k) external pure returns (uint256 s) { uint256[2] memory a; a[1] = 3; for (uint256 i; i < n; i++) s += a[k]; }
    function hoistEnum(uint256 n, uint256 k) external pure returns (uint256 s) { for (uint256 i; i < n; i++) s += uint256(E(k)); }
    enum E { A, B }
    function hoistSload(uint256 n) external returns (uint256 s) { st = 5; for (uint256 i; i < n; i++) { s += st; if (i == 2) st = 10; } }
    function hoistSloadWrite(uint256 n) external returns (uint256) { for (uint256 i; i < n; i++) { st = st + i; } return st; }
    function hoistMload(uint256 n) external pure returns (uint256 s) { uint256[] memory a = new uint256[](1); a[0] = 1; for (uint256 i; i < n; i++) { s += a[0]; a[0] = a[0] * 2; } }
    function hoistKeccak(uint256 n, uint256 k) external returns (uint256 s) { for (uint256 i; i < n; i++) { m[k] += 1; s += m[k]; } }
    function hoistAlloc(uint256 n) external pure returns (uint256 s) { require(n < 5); for (uint256 i; i < n; i++) { uint256[] memory a = new uint256[](2); a[0] = i; s += a[0]; } }
    function hoistAllocPtr(uint256 n) external pure returns (uint256 s) { require(n < 5); uint256 prev; for (uint256 i; i < n; i++) { uint256[] memory a = new uint256[](1); uint256 p; assembly { p := a } s += (p != prev) ? 1 : 0; prev = p; } }
    function loopCondSide(uint256 n) external returns (uint256) { require(n < 8); while (st++ < n) {} return st; }
    function loopCondStorage(uint256 n) external returns (uint256 c) { require(n < 8); st = n; while (st > 0) { st--; c++; } }
    function loopCondMapping(uint256 n) external returns (uint256 c) { require(n < 6); m[0] = n; while (m[0] > 0) { m[0] -= 1; c++; } }
    function loopCarriedNarrow(uint8 n) external pure returns (uint8 acc) { acc = 1; for (uint8 i; i < n; i++) acc = acc * 2; }
    function loopCarriedNarrowU(uint8 n) external pure returns (uint8 acc) { acc = 1; unchecked { for (uint8 i; i < n; i++) acc = acc * 3; } }
    function loopCarriedSigned(int8 n) external pure returns (int8 acc) { for (int8 i = n; i < 0; i++) acc -= 1; }
    function loopCarriedBool(uint256 n) external pure returns (bool b) { for (uint256 i; i < n; i++) b = !b; }
    function loopCarriedBytes(uint256 n) external pure returns (bytes4 b) { b = 0x01000000; for (uint256 i; i < n; i++) b = b >> 8; }
    function loopCarriedTwo(uint256 n) external pure returns (uint256 a, uint256 b) { a = 0; b = 1; for (uint256 i; i < n; i++) { (a, b) = (b, a + b); } }
    function loopIndexWrap(uint8 start) external pure returns (uint256 c) { unchecked { for (uint8 i = start; i != start - 1; i++) { c++; if (c > 300) break; } } }
    function loopDecToZero(uint256 n) external pure returns (uint256 c) { require(n < 10); for (uint256 i = n; i > 0; i--) c += i; }
    function loopDecPastZero(uint256 n) external pure returns (uint256 c) { require(n < 10); for (uint256 i = n; i >= 0; i--) { c++; if (c > 12) break; } }
    function loopStep(uint256 n, uint256 step) external pure returns (uint256 c) { require(n < 20); for (uint256 i; i < n; i += step) { c++; if (c > 30) break; } }
    function loopStepZero(uint256 n) external pure returns (uint256 c) { for (uint256 i; i < n; i += 0) { c++; if (c > 3) break; } }
    function loopMulStep(uint256 n) external pure returns (uint256 c) { for (uint256 i = 1; i < n; i *= 2) c++; }
    function loopArrayPush(uint256 n) external returns (uint256) { require(n < 6); for (uint256 i; i < n; i++) arr.push(i); return arr.length; }
    function loopArrayLenChange(uint256 n) external returns (uint256 c) { require(n < 6); arr.push(0); for (uint256 i; i < arr.length; i++) { if (arr.length < n) arr.push(i); c++; } }
    function loopArrayPop(uint256 n) external returns (uint256 c) { require(n < 6); for (uint256 i; i < n; i++) arr.push(i); while (arr.length > 0) { arr.pop(); c++; } }
    function loopMemLenChange(uint256 n) external pure returns (uint256 c) { require(n < 6); uint256[] memory a = new uint256[](n); for (uint256 i; i < a.length; i++) { if (i == 1) a = new uint256[](1); c++; } }
    function nestedInvariant(uint256 n, uint256 d) external pure returns (uint256 s) { require(n < 4); for (uint256 i; i < n; i++) for (uint256 j; j < n; j++) s += (i * n) / d; }
    function nestedBreakOuter(uint256 n) external pure returns (uint256 s) { require(n < 6); for (uint256 i; i < n; i++) { bool done; for (uint256 j; j < n; j++) { if (i + j == 4) { done = true; break; } s++; } if (done) break; } }
    function loopReturnMid(uint256 n) external pure returns (uint256) { for (uint256 i; i < n; i++) { if (i * i > 20) return i; } return 0; }
    function loopRevertLate(uint256 n) external returns (uint256) { for (uint256 i; i < n; i++) { st += i; } require(st < 10); return st; }
    function loopPhiMemory(uint256 n) external pure returns (uint256) { require(n < 5); uint256[] memory a = new uint256[](1); uint256[] memory b = new uint256[](1); b[0] = 9; uint256[] memory p = a; for (uint256 i; i < n; i++) p = i % 2 == 0 ? b : a; return p[0]; }
    function loopStructPhi(uint256 n) external pure returns (uint256) { require(n < 5); S memory s; for (uint256 i; i < n; i++) { S memory t; t.x = i; s = t; } return s.x; }
    struct S { uint256 x; }
    function loopAliasWrite(uint256 n) external pure returns (uint256) { require(n < 5); uint256[] memory a = new uint256[](2); uint256[] memory b = a; for (uint256 i; i < n; i++) { a[0] = i; b[1] = b[0] + 1; } return a[1]; }
    function loopStorageAlias(uint256 n) external returns (uint256) { require(n < 5); arr.push(0); arr.push(0); uint256[] storage p = arr; for (uint256 i; i < n; i++) { arr[0] = i; p[1] = p[0] + 1; } return arr[1]; }
    function loopMappingAlias(uint256 n, uint256 k) external returns (uint256) { require(n < 5); for (uint256 i; i < n; i++) { m[k] = i; m[k + 1] = m[k] + 1; } return m[k + 1]; }
    function whileTrueBreak(uint256 n) external pure returns (uint256 i) { while (true) { if (i == n || i > 5) break; i++; } }
    function doWhileFalse(uint256 n) external pure returns (uint256 c) { do { c += n; } while (false); }
    function loopUncheckedIdx(uint256 n) external pure returns (uint256 s) { require(n < 5); uint256[] memory a = new uint256[](n); for (uint256 i; i < n;) { a[i] = i; unchecked { ++i; } } for (uint256 i; i < n;) { s += a[i]; unchecked { i++; } } }
    function loopShadowInvariant(uint256 n, uint256 x) external pure returns (uint256 s) { require(n < 5); for (uint256 i; i < n; i++) { uint256 y = x + 1; s += y; } }
    function loopHoistCmp(uint256 n, uint8 x) external pure returns (uint256 s) { require(n < 5); for (uint256 i; i < n; i++) { if (x > 255) s += 1000; else s += x; } }
    function loopDeadInvariant(uint256 n, uint256 d) external pure returns (uint256 s) { require(n < 5); for (uint256 i; i < n; i++) { uint256 y = 1 / d; y; s += i; } }
    function loopCountToOverflow(uint256 start) external pure returns (uint256 c) { for (uint256 i = start; i < start + 3; i++) c++; }
    function loopCountToOverflowU(uint256 start) external pure returns (uint256 c) { unchecked { for (uint256 i = start; i < start + 3; i++) { c++; if (c > 5) break; } } }
    function loopSignedCross(int8 a, int8 b) external pure returns (uint256 c) { for (int8 i = a; i < b; i++) { c++; if (c > 300) break; } }
    function loopZeroIterDiv(uint256 n, uint256 d) external pure returns (uint256 s) { for (uint256 i; i < n && i < 0; i++) s += 1 / d; }
    function loopStructArrStorage(uint256 n) external returns (uint256 s) { require(n < 4); for (uint256 i; i < n; i++) { ss.push(); ss[i].x = i; } for (uint256 i; i < ss.length; i++) s += ss[i].x; }
    S[] ss;
    function loopBytesStorage(uint256 n) external returns (uint256) { require(n < 40); for (uint256 i; i < n; i++) bs.push(bytes1(uint8(i))); uint256 s; for (uint256 i; i < bs.length; i++) s += uint8(bs[i]); return s; }
    bytes bs;
    function loopStringBuild(uint256 n) external pure returns (string memory s) { require(n < 5); for (uint256 i; i < n; i++) s = string.concat(s, "ab"); }
    function loopBytesConcat(uint256 n) external pure returns (bytes memory b) { require(n < 5); for (uint256 i; i < n; i++) b = bytes.concat(b, bytes1(uint8(i))); }
}
