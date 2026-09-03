contract StorageStructsDeep {
    struct Inner { uint8 a; bytes b; uint256[] arr; }
    struct Mid { Inner inner; Inner[] inners; uint16 tag; }
    struct MidMap { Inner inner; mapping(uint256 => Inner) map; }
    MidMap mm;
    struct Outer { Mid mid; Mid[2] mids; string name; }
    Outer o; Outer o2; Inner i1; Inner i2; Inner[] ilist; mapping(uint256 => Outer) omap; uint256[][] nested; uint8[3][] nestedFixed; bytes[] blist;
    function _fill(Inner storage x, uint8 v) internal { x.a = v; x.b = new bytes(v % 40); x.arr.push(v); x.arr.push(v + 1); }
    function innerCopy(uint8 v) external returns (uint8, uint256, uint256) { _fill(i1, v); i2 = i1; i1.arr.push(9); i1.b = ""; return (i2.a, i2.b.length, i2.arr.length); }
    function innerCopySelf(uint8 v) external returns (uint8, uint256) { _fill(i1, v); i1 = i1; return (i1.a, i1.arr.length); }
    function innerCopyMem(uint8 v) external returns (uint8, uint256, uint256) { _fill(i1, v); Inner memory m = i1; m.arr[0] = 77; i2 = m; return (i2.a, i2.arr[0], i2.b.length); }
    function innerDelete(uint8 v) external returns (uint8, uint256, uint256) { _fill(i1, v); delete i1; return (i1.a, i1.b.length, i1.arr.length); }
    function innerListPush(uint8 v) external returns (uint256, uint8, uint256) { _fill(i1, v); ilist.push(i1); ilist.push(); _fill(ilist[1], v + 1); ilist.push(ilist[0]); return (ilist.length, ilist[2].a, ilist[1].arr.length); }
    function innerListPop(uint8 v) external returns (uint256, uint8) { _fill(i1, v); ilist.push(i1); ilist.push(i1); ilist.pop(); ilist.push(); return (ilist.length, ilist[1].a); }
    function innerListPopClears(uint8 v) external returns (uint256 raw) { _fill(i1, v); ilist.push(i1); ilist.pop(); uint256 s; assembly { mstore(0, ilist.slot) s := keccak256(0, 32) raw := sload(s) } uint256 r2; assembly { r2 := sload(add(s, 1)) } raw += r2; }
    function midOps(uint8 v) external returns (uint8, uint256, uint16, uint8) { _fill(o.mid.inner, v); o.mid.inners.push(); _fill(o.mid.inners[0], v + 1); _fill(mm.map[7], v + 2); o.mid.tag = 0xbeef; return (o.mid.inner.a, o.mid.inners[0].arr[1], o.mid.tag, mm.map[7].a); }
    function midsFixed(uint8 v) external returns (uint8, uint8, uint256) { _fill(o.mids[0].inner, v); _fill(o.mids[1].inner, v + 1); o.mids[1].inners.push(); return (o.mids[0].inner.a, o.mids[1].inner.a, o.mids[1].inners.length); }
    function outerName(string calldata s, uint8 v) external returns (string memory, uint8) { o.name = s; _fill(o.mid.inner, v); return (o.name, o.mid.inner.a); }
    function outerMapCopyInner(uint8 v) external returns (uint8, uint256) { _fill(omap[1].mid.inner, v); omap[2].mid.inner = omap[1].mid.inner; omap[1].mid.inner.a = 0; return (omap[2].mid.inner.a, omap[2].mid.inner.arr.length); }
    function midsCopy(uint8 v) external returns (uint8, uint256) { _fill(o.mids[0].inner, v); o.mids[0].inners.push(); o.mids[1].inner = o.mids[0].inner; o.mids[1].inners = o.mids[0].inners; return (o.mids[1].inner.a, o.mids[1].inners.length); }
    function deleteMid(uint8 v) external returns (uint8, uint256, uint16, uint8) { _fill(o.mid.inner, v); o.mid.inners.push(); _fill(mm.map[3], v); o.mid.tag = 5; delete o.mid.inner; delete o.mid.inners; delete o.mid.tag; delete mm.inner; return (o.mid.inner.a, o.mid.inners.length, o.mid.tag, mm.map[3].a); }
    function nestedPush(uint256 v) external returns (uint256, uint256, uint256) { nested.push(); nested.push(); nested[0].push(v); nested[1].push(v + 1); nested[1].push(v + 2); nested[0] = nested[1]; nested[1].pop(); return (nested[0].length, nested[0][1], nested[1].length); }
    function nestedFixedPush(uint8 v) external returns (uint8, uint8, uint256 raw) { nestedFixed.push([v, v, v]); nestedFixed.push(); nestedFixed[1][2] = v + 1; uint256 s; assembly { mstore(0, nestedFixed.slot) s := keccak256(0, 32) raw := sload(s) } return (nestedFixed[0][1], nestedFixed[1][2], raw); }
    function nestedFixedCopy(uint8 v) external returns (uint8, uint8) { nestedFixed.push([v, v + 1, v + 2]); nestedFixed.push(nestedFixed[0]); nestedFixed[0][0] = 0; return (nestedFixed[1][0], nestedFixed[1][2]); }
    function nestedMemCopy(uint256 v) external returns (uint256, uint256) { uint256[][] memory m = new uint256[][](2); m[1] = new uint256[](2); m[1][1] = v; nested = m; return (nested.length, nested[1][1]); }
    function nestedToMem(uint256 v) external returns (uint256[][] memory) { nested.push(); nested[0].push(v); nested.push(); nested[1].push(v + 1); nested[1].push(v + 2); return nested; }
    function blistOps(bytes calldata d) external returns (uint256, bytes memory, uint256) { blist.push(d); blist.push(); blist[1] = bytes.concat(d, d); blist.push(blist[1]); blist[0].push(0x01); return (blist.length, blist[2], blist[0].length); }
    function blistDelete(bytes calldata d) external returns (uint256, uint256) { blist.push(d); blist.push(d); delete blist[0]; return (blist[0].length, blist[1].length); }
    function blistToMem(bytes calldata d) external returns (bytes[] memory) { blist.push(d); blist.push(bytes.concat(d, hex"ff")); return blist; }
    function blistFromMem(uint8 n) external returns (uint256, uint256) { require(n < 4); bytes[] memory m = new bytes[](n); for (uint8 k; k < n; k++) m[k] = new bytes(k * 20); blist = m; return (blist.length, n > 0 ? blist[n - 1].length : 0); }
    function outerCopy(uint8 v) external returns (uint8, uint256, string memory, uint8) { _fill(o.mid.inner, v); o.mid.inners.push(); o.name = "outer"; _fill(o.mids[1].inner, v + 1); o2.mid.inner = o.mid.inner; o2.mid.inners = o.mid.inners; o2.name = o.name; o2.mids[1] .inner = o.mids[1].inner; return (o2.mid.inner.a, o2.mid.inners.length, o2.name, o2.mids[1].inner.a); }
    function encodeInner(uint8 v) external returns (bytes memory) { _fill(i1, v); return abi.encode(i1); }
    function encodeIlist(uint8 v) external returns (bytes memory) { _fill(i1, v); ilist.push(i1); ilist.push(i1); return abi.encode(ilist); }
    function encodeNested(uint256 v) external returns (bytes memory) { nested.push(); nested[0].push(v); nested.push(); return abi.encode(nested); }
    function encodeNestedFixed(uint8 v) external returns (bytes memory) { nestedFixed.push([v, 0, v]); return abi.encode(nestedFixed); }
    function returnInnerList(uint8 v) external returns (Inner[] memory) { _fill(i1, v); ilist.push(i1); ilist.push(); return ilist; }
    function returnInnerPtr(uint8 v) external returns (uint8, uint256) { _fill(i1, v); Inner storage p = _pick(v); p.a += 1; return (i1.a, i2.a); }
    function _pick(uint8 v) internal view returns (Inner storage) { return v % 2 == 0 ? i1 : i2; }
    function slotLayout() external pure returns (uint256 a, uint256 b, uint256 c, uint256 d) { assembly { a := o.slot b := o2.slot c := i1.slot d := ilist.slot } }
    function innerRaw(uint8 v) external returns (uint256 s0, uint256 s1, uint256 s2) { _fill(i1, v); assembly { s0 := sload(i1.slot) s1 := sload(add(i1.slot, 1)) s2 := sload(add(i1.slot, 2)) } }
    function memStructOfStructs(uint8 v) external pure returns (uint8, uint256, uint256) { Outer memory m; m.mid.inner.a = v; m.mid.inners = new Inner[](2); m.mid.inners[1].arr = new uint256[](3); m.mids[1].inner.b = "xyz"; return (m.mid.inner.a, m.mid.inners[1].arr.length, m.mids[1].inner.b.length); }
    function memStructToStorageDeep(uint8 v) external returns (uint8, uint256, uint256, uint16) { Outer memory m; m.mid.inner.a = v; m.mid.inners = new Inner[](1); m.mid.inners[0].arr = new uint256[](2); m.mid.inners[0].arr[1] = v; m.mid.tag = 9; m.name = "n"; o = m; return (o.mid.inner.a, o.mid.inners[0].arr[1], o.mid.inners.length, o.mid.tag); }
}
