contract DeleteSemantics {
    struct S { uint8 a; int8 b; bytes c; uint256[] d; mapping(uint256 => uint256) m; string e; }
    struct P { uint256 x; bytes4 y; }
    uint8 u8; int8 i8; bool bl; bytes4 b4; address ad; E en; uint256[] arr; uint8[] arr8; uint256[3] fixedArr; bytes bs; string str; S s; P p; P[] parr; mapping(uint256 => uint256) m; mapping(uint256 => P) mp; uint256[][] arr2d; bytes[] bsArr;
    enum E { A, B, C }
    function delScalars(uint256 raw) external returns (uint8, int8, bool, bytes4, address, E, uint256 slot) { assembly { sstore(u8.slot, raw) } delete u8; delete bl; delete ad; assembly { slot := sload(u8.slot) } return (u8, i8, bl, b4, ad, en, slot); }
    function delScalarsAll(uint256 raw) external returns (uint256 slot) { assembly { sstore(u8.slot, raw) } delete u8; delete i8; delete bl; delete b4; delete ad; delete en; assembly { slot := sload(u8.slot) } }
    function delLocal(uint8 v) external pure returns (uint8, int8, bool, bytes4, E) { uint8 a = v; int8 b = -1; bool c = true; bytes4 d = 0x01020304; E e = E.C; delete a; delete b; delete c; delete d; delete e; return (a, b, c, d, e); }
    function delLocalDirty(uint256 raw) external pure returns (uint256 r) { uint8 a; assembly { a := raw } delete a; assembly { r := a } }
    function delArr(uint256 n) external returns (uint256, uint256 raw) { require(n < 5); for (uint256 i; i < n; i++) arr.push(i + 1); delete arr; uint256 sl; assembly { mstore(0, arr.slot) sl := keccak256(0, 32) raw := sload(sl) } return (arr.length, raw); }
    function delArrElem(uint256 n, uint256 i) external returns (uint256) { require(n < 5); for (uint256 k; k < n; k++) arr.push(k + 1); delete arr[i]; return arr[i]; }
    function delArr8(uint256 n) external returns (uint256, uint256 raw) { require(n < 70); for (uint256 i; i < n; i++) arr8.push(uint8(i + 1)); delete arr8; uint256 sl; assembly { mstore(0, arr8.slot) sl := keccak256(0, 32) raw := sload(add(sl, 1)) } return (arr8.length, raw); }
    function delArr8Elem(uint256 i) external returns (uint8, uint256 raw) { require(i < 33); for (uint256 k; k < 33; k++) arr8.push(0xff); delete arr8[i]; uint256 sl; assembly { mstore(0, arr8.slot) sl := keccak256(0, 32) raw := sload(add(sl, div(i, 32))) } return (arr8[i], raw); }
    function delFixed(uint256 v) external returns (uint256, uint256, uint256) { fixedArr = [v, v, v]; delete fixedArr; return (fixedArr[0], fixedArr[1], fixedArr[2]); }
    function delFixedElem(uint256 v, uint256 i) external returns (uint256, uint256) { fixedArr = [v, v, v]; delete fixedArr[i]; return (fixedArr[i], fixedArr[(i + 1) % 3]); }
    function delBytes(bytes calldata d) external returns (uint256, uint256 raw) { bs = d; delete bs; assembly { raw := sload(bs.slot) } return (bs.length, raw); }
    function delBytesLong(bytes calldata d) external returns (uint256, uint256 raw, uint256 data) { require(d.length > 32); bs = d; delete bs; uint256 sl; assembly { raw := sload(bs.slot) mstore(0, bs.slot) sl := keccak256(0, 32) data := sload(sl) } return (bs.length, raw, data); }
    function delBytesElem(bytes calldata d, uint256 i) external returns (bytes1) { bs = d; delete bs[i]; return bs[i]; }
    function delStr(string calldata d) external returns (uint256) { str = d; delete str; return bytes(str).length; }
    function delStruct(uint8 a, bytes calldata c) external returns (uint8, int8, uint256, uint256, uint256, uint256) { s.a = a; s.b = -3; s.c = c; s.d.push(1); s.d.push(2); s.m[1] = 5; s.e = "hello there, a long string over 32 bytes"; delete s; return (s.a, s.b, s.c.length, s.d.length, s.m[1], bytes(s.e).length); }
    function delStructMember(uint8 a) external returns (uint8, int8, uint256) { s.a = a; s.b = -3; s.d.push(1); delete s.b; delete s.d; return (s.a, s.b, s.d.length); }
    function delP(uint256 x) external returns (uint256, bytes4, uint256 raw0, uint256 raw1) { p = P(x, 0xaabbccdd); delete p; assembly { raw0 := sload(p.slot) raw1 := sload(add(p.slot, 1)) } return (p.x, p.y, raw0, raw1); }
    function delParr(uint256 x) external returns (uint256, uint256 raw) { parr.push(P(x, 0x00000001)); parr.push(P(x, 0x00000002)); delete parr; uint256 sl; assembly { mstore(0, parr.slot) sl := keccak256(0, 32) raw := sload(add(sl, 2)) } return (parr.length, raw); }
    function delParrElem(uint256 x) external returns (uint256, bytes4, uint256, bytes4) { parr.push(P(x, 0x00000001)); parr.push(P(x, 0x00000002)); delete parr[0]; return (parr[0].x, parr[0].y, parr[1].x, parr[1].y); }
    function delMap(uint256 k, uint256 v) external returns (uint256) { m[k] = v; delete m[k]; return m[k]; }
    function delMapStruct(uint256 k, uint256 v) external returns (uint256, bytes4) { mp[k] = P(v, 0x000000ff); delete mp[k]; return (mp[k].x, mp[k].y); }
    function delArr2d(uint256 v) external returns (uint256, uint256) { arr2d.push(); arr2d[0].push(v); arr2d.push(); arr2d[1].push(v); delete arr2d[0]; return (arr2d[0].length, arr2d[1].length); }
    function delArr2dAll(uint256 v) external returns (uint256, uint256 raw) { arr2d.push(); arr2d[0].push(v); delete arr2d; uint256 sl; uint256 sl2; assembly { mstore(0, arr2d.slot) sl := keccak256(0, 32) mstore(0, sl) sl2 := keccak256(0, 32) raw := sload(sl2) } return (arr2d.length, raw + sload_(sl)); }
    function sload_(uint256 sl) internal view returns (uint256 r) { assembly { r := sload(sl) } }
    function delBsArr(bytes calldata d) external returns (uint256, uint256) { bsArr.push(d); bsArr.push(d); delete bsArr[0]; uint256 l0 = bsArr[0].length; delete bsArr; return (l0, bsArr.length); }
    function delMemArr(uint256 v) external pure returns (uint256, uint256) { uint256[] memory a = new uint256[](2); a[0] = v; a[1] = v; uint256[] memory b = a; delete a; return (a.length, b[0] + b[1]); }
    function delMemArrElem(uint256 v) external pure returns (uint256, uint256) { uint256[] memory a = new uint256[](2); a[0] = v; a[1] = v; delete a[0]; return (a[0], a[1]); }
    function delMemFixed(uint256 v) external pure returns (uint256, uint256) { uint256[2] memory a = [v, v]; delete a; return (a[0], a[1]); }
    function delMemBytes(bytes memory d) external pure returns (uint256, uint256) { bytes memory e = d; delete d; return (d.length, e.length); }
    function delMemBytesElem(bytes memory d, uint256 i) external pure returns (bytes1) { delete d[i]; return d[i]; }
    function delMemStr(string memory d) external pure returns (uint256) { delete d; return bytes(d).length; }
    function delMemStruct(uint256 x) external pure returns (uint256, bytes4, uint256) { P memory q = P(x, 0x000000ff); P memory r = q; delete q; return (q.x, q.y, r.x); }
    function delMemStructNested(uint256 x) external pure returns (uint256, uint256, uint256) { N memory n = N(x, new uint256[](2), P(x, 0x00000001)); n.arr[0] = x; N memory o = n; delete n; return (n.v + n.arr.length + n.p.x, o.v, o.arr.length); }
    struct N { uint256 v; uint256[] arr; P p; }
    function delMemNestedMember(uint256 x) external pure returns (uint256, uint256) { N memory n = N(x, new uint256[](2), P(x, 0x00000001)); uint256[] memory keep = n.arr; delete n.arr; return (n.arr.length, keep.length); }
    function delFnPtr(uint256 v) external pure returns (uint256) { function(uint256) internal pure returns (uint256) f = _id; delete f; return f(v); }
    function _id(uint256 v) internal pure returns (uint256) { return v; }
    function delExtFnPtr() external view returns (address, bytes4) { function() external view returns (address, bytes4) f = this.delExtFnPtr; delete f; return (f.address, f.selector); }
    function delInLoop(uint256 n) external returns (uint256 sum) { require(n < 5); for (uint256 i; i < n; i++) arr.push(i + 1); for (uint256 i; i < n; i++) { if (i % 2 == 0) delete arr[i]; } for (uint256 i; i < n; i++) sum += arr[i]; }
    function delThenPush(uint256 v) external returns (uint256, uint256) { arr.push(v); arr.push(v); delete arr; arr.push(v + 1); return (arr.length, arr[0]); }
    function delThenPushBytes(bytes calldata d) external returns (uint256, bytes1) { bs = d; delete bs; bs.push(0x42); return (bs.length, bs[0]); }
    function delTuple(uint256 v) external returns (uint256, uint256) { m[1] = v; arr.push(v); delete m[1]; delete arr; return (m[1], arr.length); }
    function delEnumDirty(uint256 raw) external returns (E, uint256 slot) { assembly { sstore(en.slot, raw) } delete en; assembly { slot := sload(en.slot) } return (en, slot); }
    function delAddrDirty(uint256 raw) external returns (address, uint256 slot) { assembly { sstore(ad.slot, raw) } delete ad; assembly { slot := sload(ad.slot) } return (ad, slot); }
}
