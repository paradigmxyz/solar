contract InlineCse {
    uint256 st; uint256[] arr; mapping(uint256 => uint256) m; bytes bs;
    struct S { uint256 a; uint256 b; }
    function readWriteRead(uint256 v) external returns (uint256, uint256) { st = v; uint256 a = st; st = v + 1; uint256 b = st; return (a, b); }
    function readAsmWriteRead(uint256 v) external returns (uint256, uint256) { st = v; uint256 a = st; assembly { sstore(st.slot, add(v, 5)) } uint256 b = st; return (a, b); }
    function readCallRead(uint256 v) external returns (uint256, uint256) { st = v; uint256 a = st; _bump(); uint256 b = st; return (a, b); }
    function _bump() internal { st += 1; }
    function mappingReadWrite(uint256 k, uint256 v) external returns (uint256, uint256) { m[k] = v; uint256 a = m[k]; m[k] = v + 1; return (a, m[k]); }
    function mappingAliasKeys(uint256 k, uint256 v) external returns (uint256, uint256) { m[k] = v; uint256 a = m[k]; m[k + 0] = v + 1; return (a, m[k]); }
    function mappingTwoKeys(uint256 k, uint256 j, uint256 v) external returns (uint256, uint256) { m[k] = v; uint256 a = m[j]; m[j] = 3; return (a, m[k]); }
    function arrAliasIdx(uint256 i, uint256 j, uint256 v) external returns (uint256, uint256) { arr.push(1); arr.push(2); arr.push(3); require(i < 3 && j < 3); uint256 a = arr[i]; arr[j] = v; return (a, arr[i]); }
    function memAlias(uint256 i, uint256 j, uint256 v) external pure returns (uint256, uint256) { uint256[] memory a = new uint256[](3); a[0] = 1; a[1] = 2; a[2] = 3; require(i < 3 && j < 3); uint256 x = a[i]; a[j] = v; return (x, a[i]); }
    function memAliasTwoArrays(uint256 v) external pure returns (uint256, uint256) { uint256[] memory a = new uint256[](1); uint256[] memory b = a; a[0] = 1; uint256 x = b[0]; a[0] = v; return (x, b[0]); }
    function memAliasStruct(uint256 v) external pure returns (uint256, uint256) { S memory s = S(1, 2); S memory t = s; uint256 x = t.a; s.a = v; return (x, t.a); }
    function memAsmWrite(uint256 v) external pure returns (uint256, uint256) { uint256[] memory a = new uint256[](1); a[0] = 1; uint256 x = a[0]; assembly { mstore(add(a, 32), v) } return (x, a[0]); }
    function memAsmWriteFmp(uint256 v) external pure returns (uint256, uint256) { uint256[] memory a = new uint256[](1); a[0] = 1; uint256 x = a[0]; assembly { let p := a mstore(add(p, 0x20), v) } uint256[] memory b = new uint256[](1); b[0] = 7; return (x + b[0], a[0]); }
    function keccakTwice(uint256 v) external pure returns (bool) { bytes memory b = abi.encode(v); bytes32 h1 = keccak256(b); b[0] = 0xff; bytes32 h2 = keccak256(b); return h1 == h2; }
    function keccakTwiceSame(uint256 v) external pure returns (bool) { bytes memory b = abi.encode(v); return keccak256(b) == keccak256(b); }
    function keccakAsmMutate(uint256 v) external pure returns (bool) { bytes memory b = abi.encode(v); bytes32 h1 = keccak256(b); assembly { mstore(add(b, 32), not(v)) } bytes32 h2 = keccak256(b); return h1 == h2; }
    function pureTwice(uint256 v) external pure returns (uint256) { return _sq(v) + _sq(v); }
    function _sq(uint256 x) internal pure returns (uint256) { return x * x; }
    function pureTwiceDiff(uint256 v) external pure returns (uint256) { return _sq(v) + _sq(v + 1); }
    function viewTwiceWrite(uint256 v) external returns (uint256) { st = v; uint256 a = _rd(); st = v + 1; return a + _rd(); }
    function _rd() internal view returns (uint256) { return st; }
    function statefulTwice(uint256 v) external returns (uint256, uint256) { return (_inc(v), _inc(v)); }
    function _inc(uint256 v) internal returns (uint256) { st += v; return st; }
    function argsEvalOnce(uint256 v) external returns (uint256, uint256) { uint256 r = _two(_inc(v), _inc(v)); return (r, st); }
    function _two(uint256 a, uint256 b) internal pure returns (uint256) { return a * 1000 + b; }
    function sameCallInCond(uint256 v) external returns (uint256) { if (_inc(v) > 5 && _inc(v) > 10) return st; return st + 1000; }
    function memReturnTwice(uint256 v) external pure returns (bool) { uint256[] memory a = _mk(v); uint256[] memory b = _mk(v); a[0] = 99; return a[0] == b[0]; }
    function _mk(uint256 v) internal pure returns (uint256[] memory a) { a = new uint256[](1); a[0] = v; }
    function memReturnPtr(uint256 v) external pure returns (bool) { uint256[] memory a = _mk(v); uint256[] memory b = _mk(v); uint256 pa; uint256 pb; assembly { pa := a pb := b } return pa == pb; }
    function structReturnTwice(uint256 v) external pure returns (uint256) { S memory a = _mkS(v); S memory b = _mkS(v); a.a = 0; return a.a + b.a; }
    function _mkS(uint256 v) internal pure returns (S memory) { return S(v, v); }
    function inlineWithRevert(uint256 v) external pure returns (uint256) { return _guard(v) + _guard(v + 1); }
    function _guard(uint256 v) internal pure returns (uint256) { require(v < 100, "big"); return v; }
    function inlineWithLoop(uint256 v) external pure returns (uint256) { require(v < 5); return _loop(v) + _loop(v); }
    function _loop(uint256 n) internal pure returns (uint256 s) { for (uint256 i; i < n; i++) s += i; }
    function inlineNested(uint256 v) external pure returns (uint256) { return _a(_a(_a(v))); }
    function _a(uint256 v) internal pure returns (uint256) { return _b(v) + 1; }
    function _b(uint256 v) internal pure returns (uint256) { return v * 2; }
    function inlineRecursive(uint256 v) external pure returns (uint256) { require(v < 6); return _fact(v); }
    function _fact(uint256 n) internal pure returns (uint256) { return n <= 1 ? 1 : n * _fact(n - 1); }
    function inlineMemArg(uint256 v) external pure returns (uint256) { uint256[] memory a = new uint256[](1); a[0] = v; _mut(a); _mut(a); return a[0]; }
    function _mut(uint256[] memory a) internal pure { a[0] += 1; }
    function inlineStorageArg(uint256 v) external returns (uint256) { arr.push(v); _mutS(arr); _mutS(arr); return arr[0]; }
    function _mutS(uint256[] storage a) internal { a[0] += 1; }
    function inlineManyRets(uint256 v) external pure returns (uint256) { (uint256 a, uint256 b) = _pair(v); (uint256 c, uint256 d) = _pair(v); return a + b + c + d; }
    function _pair(uint256 v) internal pure returns (uint256, uint256) { return (v, v + 1); }
    function inlineUnused(uint256 v) external pure returns (uint256) { _pair(v); _sq(v); return v; }
    function inlineUnusedRevert(uint256 v) external pure returns (uint256) { _guard(v); return v; }
    function inlineUnusedState(uint256 v) external returns (uint256) { _inc(v); return st; }
    function cseAcrossBranch(uint256 a, uint256 b, bool c) external pure returns (uint256) { uint256 x = a * b; if (c) { return x + a * b; } return a * b; }
    function cseAcrossBranchRevert(uint256 a, uint256 b, bool c) external pure returns (uint256) { if (c) { return a * b; } return a + b; }
    function cseDivBranch(uint256 a, uint256 b, bool c) external pure returns (uint256) { if (c) return a / b; return b; }
    function cseDivBoth(uint256 a, uint256 b, bool c) external pure returns (uint256) { if (c) return a / b; return a / b + 1; }
    function cseNarrow(uint8 a, uint8 b) external pure returns (uint256) { uint8 x = a + b; uint256 y = uint256(a) + uint256(b); return x == y ? 1 : 0; }
    function cseNarrowMul(uint8 a, uint8 b) external pure returns (uint256) { unchecked { uint8 x = a * b; uint256 y = uint256(a) * uint256(b); return x == y ? 1 : 0; } }
    function cseShift(uint8 a) external pure returns (uint256) { unchecked { uint8 x = a << 4; uint256 y = uint256(a) << 4; return x == y ? 1 : 0; } }
    function cseSignExt(int8 a) external pure returns (bool) { int256 x = a; int8 y = a; return x == y; }
    function cseMixedWidth(uint8 a) external pure returns (bool) { unchecked { uint8 x = a + 1; uint16 y = uint16(a) + 1; return x == y; } }
    function cseMixedNeg(int8 a) external pure returns (bool) { unchecked { int8 x = -a; int16 y = -int16(a); return x == y; } }
    function cseMixedNot(uint8 a) external pure returns (bool) { uint8 x = ~a; uint16 y = ~uint16(a); return x == y; }
    function cseMixedShr(int8 a) external pure returns (bool) { int8 x = a >> 1; int16 y = int16(a) >> 1; return x == y; }
    function cseMixedDiv(int8 a) external pure returns (bool) { int8 x = a / 2; int16 y = int16(a) / 2; return x == y; }
    function cseMixedMod(int8 a) external pure returns (bool) { int8 x = a % 3; int16 y = int16(a) % 3; return x == y; }
    function cseMixedExp(uint8 a) external pure returns (bool) { unchecked { uint8 x = a ** 2; uint16 y = uint16(a) ** 2; return x == y; } }
    function gvnRedundantLoad(uint256 v) external returns (uint256) { st = v; uint256 a = st; uint256 b = st; return a + b; }
    function gvnLoadStoreLoad(uint256 v) external returns (uint256) { st = v; uint256 a = st; st = a; return st; }
    function gvnDeadStore(uint256 v) external returns (uint256) { st = 1; st = 2; st = v; return st; }
    function gvnStoreSameVal(uint256 v) external returns (uint256) { st = v; st = v; return st; }
    function gvnBytesStorage(bytes calldata d) external returns (uint256) { bs = d; uint256 a = bs.length; bs.push(0x00); return a + bs.length; }
    function gvnArrLen(uint256 v) external returns (uint256) { arr.push(v); uint256 a = arr.length; arr.push(v); uint256 b = arr.length; arr.pop(); return a * 100 + b * 10 + arr.length; }
    function gvnMemLen(uint256 n) external pure returns (uint256) { require(n < 5); uint256[] memory a = new uint256[](n); uint256 l = a.length; a = new uint256[](n + 1); return l + a.length; }
    function gvnBytesMem(bytes memory b) external pure returns (uint256) { uint256 l = b.length; assembly { mstore(b, add(mload(b), 1)) } return l * 1000 + b.length; }
    function gvnCalldataLoad(uint256 a) external pure returns (uint256) { uint256 x; uint256 y; assembly { x := calldataload(4) y := calldataload(4) } return a + x + y; }
    function gvnMsgData() external pure returns (bytes32, bytes32) { return (keccak256(msg.data), keccak256(msg.data)); }
    function gvnCalldatasize(uint256 a) external pure returns (uint256) { return msg.data.length + msg.data.length + a; }
    function tstoreTwice(uint256 v) external returns (uint256, uint256) { uint256 a; uint256 b; assembly { tstore(1, v) a := tload(1) tstore(1, add(v, 1)) b := tload(1) } return (a, b); }
}
