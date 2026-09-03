contract Mappings {
    mapping(bytes => uint256) mb; mapping(string => uint256) ms; mapping(address => uint256) ma; mapping(bytes4 => uint256) mb4;
    mapping(int8 => uint256) mi8; mapping(bool => uint256) mbool; mapping(E => uint256) me; mapping(U => uint256) mu; mapping(bytes32 => uint256) mb32;
    mapping(uint8 => mapping(int8 => uint256)) mnest; mapping(uint256 => S) mstruct; mapping(uint256 => uint256[]) marr; mapping(uint256 => mapping(uint256 => uint8[])) mnestarr;
    mapping(bytes => mapping(string => bytes)) mdyn; mapping(uint256 => bytes) mbytes; mapping(uint256 => string) mstr;
    enum E { A, B, C } type U is uint32;
    struct S { uint256 a; uint8[] arr; mapping(uint256 => uint256) inner; }
    function _inject8(uint256 raw) internal pure returns (uint8 r) { assembly { r := raw } }
    function keyBytes(bytes calldata k, uint256 v) external returns (uint256, uint256) { mb[k] = v; return (mb[k], mb[bytes.concat(k, hex"00")]); }
    function keyBytesMem(bytes memory k, uint256 v) external returns (uint256) { mb[k] = v; return mb[k]; }
    function keyString(string calldata k, uint256 v) external returns (uint256, uint256) { ms[k] = v; return (ms[k], ms[string(bytes.concat(bytes(k), "x"))]); }
    function keyStringLit(uint256 v) external returns (uint256, uint256) { ms["hello"] = v; return (ms["hello"], ms["hell"]); }
    function keyBytesStrSame(bytes calldata k, uint256 v) external returns (uint256 a, uint256 b) { mb[k] = v; bytes32 h1; bytes32 h2; string memory s = string(k); assembly { mstore(0, 0) } a = mb[k]; b = ms[s]; }
    function keyAddr(address k, uint256 v) external returns (uint256) { ma[k] = v; return ma[k]; }
    function keyAddrDirty(uint256 raw, uint256 v) external returns (uint256, uint256) { address k; assembly { k := raw } ma[k] = v; return (ma[k], ma[address(uint160(raw))]); }
    function keyB4(bytes4 k, uint256 v) external returns (uint256) { mb4[k] = v; return mb4[k]; }
    function keyB4Dirty(uint256 raw, uint256 v) external returns (uint256, uint256) { bytes4 k; assembly { k := raw } mb4[k] = v; return (mb4[k], mb4[bytes4(bytes32(raw))]); }
    function keyI8(int8 k, uint256 v) external returns (uint256, uint256) { mi8[k] = v; return (mi8[k], mi8[-k]); }
    function keyI8Dirty(uint256 raw, uint256 v) external returns (uint256, uint256) { int8 k; assembly { k := raw } mi8[k] = v; return (mi8[k], mi8[int8(uint8(raw))]); }
    function keyBool(bool k, uint256 v) external returns (uint256, uint256) { mbool[k] = v; return (mbool[k], mbool[!k]); }
    function keyBoolDirty(uint256 raw, uint256 v) external returns (uint256, uint256) { bool k; assembly { k := raw } mbool[k] = v; return (mbool[true], mbool[false]); }
    function keyEnum(E k, uint256 v) external returns (uint256) { me[k] = v; return me[k] + me[E.A]; }
    function keyEnumDirty(uint256 raw, uint256 v) external returns (uint256, uint256) { E k; assembly { k := raw } me[k] = v; return (me[k], me[E.B]); }
    function keyUdvt(U k, uint256 v) external returns (uint256) { mu[k] = v; return mu[k] + mu[U.wrap(0)]; }
    function keyUdvtDirty(uint256 raw, uint256 v) external returns (uint256, uint256) { U k; assembly { k := raw } mu[k] = v; return (mu[k], mu[U.wrap(uint32(raw))]); }
    function keyB32(bytes32 k, uint256 v) external returns (uint256) { mb32[k] = v; return mb32[k]; }
    function keyB32FromHash(bytes calldata d, uint256 v) external returns (uint256) { mb32[keccak256(d)] = v; return mb32[keccak256(abi.encodePacked(d))]; }
    function nested(uint8 a, int8 b, uint256 v) external returns (uint256, uint256) { mnest[a][b] = v; return (mnest[a][b], mnest[a][b + 1 > b ? b + 1 : b - 1]); }
    function nestedDirty(uint256 raw, uint256 v) external returns (uint256) { uint8 a = _inject8(raw); int8 b; assembly { b := raw } mnest[a][b] = v; return mnest[uint8(raw)][int8(uint8(raw))]; }
    function structMap(uint256 k, uint256 v) external returns (uint256, uint256, uint256) { S storage s = mstruct[k]; s.a = v; s.arr.push(uint8(v)); s.inner[v] = k; return (mstruct[k].a, mstruct[k].arr[0], mstruct[k].inner[v]); }
    function structMapDelete(uint256 k, uint256 v) external returns (uint256, uint256, uint256) { mstruct[k].a = v; mstruct[k].arr.push(1); mstruct[k].inner[1] = 1; delete mstruct[k]; return (mstruct[k].a, mstruct[k].arr.length, mstruct[k].inner[1]); }
    function arrMap(uint256 k, uint256 v) external returns (uint256, uint256) { marr[k].push(v); marr[k].push(v + 1); marr[k + 1].push(9); marr[k].pop(); return (marr[k].length, marr[k][0] + marr[k + 1][0]); }
    function arrMapCopy(uint256 k, uint256[] calldata v) external returns (uint256) { marr[k] = v; return marr[k].length > 0 ? marr[k][marr[k].length - 1] : 0; }
    function nestArr(uint256 a, uint256 b, uint8 v) external returns (uint8, uint256) { mnestarr[a][b].push(v); mnestarr[a][b].push(v); mnestarr[b][a].push(1); return (mnestarr[a][b][1], mnestarr[b][a].length); }
    function dynDyn(bytes calldata k1, string calldata k2, bytes calldata v) external returns (bytes memory) { mdyn[k1][k2] = v; return mdyn[k1][k2]; }
    function bytesVal(uint256 k, bytes calldata v) external returns (bytes memory, uint256) { mbytes[k] = v; mbytes[k].push(0x01); return (mbytes[k], mbytes[k].length); }
    function strVal(uint256 k, string calldata v) external returns (string memory) { mstr[k] = v; return mstr[k]; }
    function deleteKey(address k, uint256 v) external returns (uint256) { ma[k] = v; delete ma[k]; return ma[k]; }
    function deleteArrVal(uint256 k) external returns (uint256) { marr[k].push(1); delete marr[k]; return marr[k].length; }
    function deleteBytesVal(uint256 k, bytes calldata v) external returns (uint256) { mbytes[k] = v; delete mbytes[k]; return mbytes[k].length; }
    function slotOf(address k) external pure returns (bytes32) { return keccak256(abi.encode(k, uint256(2))); }
    function rawSlot(address k, uint256 v) external returns (bool) { ma[k] = v; uint256 r; bytes32 sl = keccak256(abi.encode(k, uint256(2))); assembly { r := sload(sl) } return r == v; }
    function rawSlotBytes(bytes calldata k, uint256 v) external returns (bool) { mb[k] = v; uint256 r; bytes32 sl = keccak256(abi.encodePacked(k, uint256(0))); assembly { r := sload(sl) } return r == v; }
    function rawSlotI8(int8 k, uint256 v) external returns (bool) { mi8[k] = v; uint256 r; bytes32 sl = keccak256(abi.encode(k, uint256(4))); assembly { r := sload(sl) } return r == v; }
    function rawSlotB4(bytes4 k, uint256 v) external returns (bool) { mb4[k] = v; uint256 r; bytes32 sl = keccak256(abi.encode(k, uint256(3))); assembly { r := sload(sl) } return r == v; }
    function storagePtrParam(uint256 k, uint256 v) external returns (uint256) { return _set(mstruct[k], v); }
    function _set(S storage s, uint256 v) internal returns (uint256) { s.a = v; return s.a + 1; }
    function mapInLoop(uint256 n) external returns (uint256 s) { require(n < 6); for (uint256 i; i < n; i++) ma[address(uint160(i))] = i * 2; for (uint256 i; i < n; i++) s += ma[address(uint160(i))]; }
    function keyExpr(uint256 a, uint256 b, uint256 v) external returns (uint256) { mstruct[a + b].a = v; return mstruct[b + a].a; }
    function keySideEffect(uint256 v) external returns (uint256, uint256) { uint256 c; ma[address(uint160(c++))] = v; ma[address(uint160(c++))] += v; return (ma[address(0)] + ma[address(1)], c); }
    function keyBytesShort(uint256 v) external returns (uint256, uint256) { mb[""] = v; return (mb[""], mb[hex""]); }
    function keyStrUnicode(uint256 v) external returns (uint256) { ms[unicode"héllo"] = v; return ms["h\xc3\xa9llo"]; }
}
