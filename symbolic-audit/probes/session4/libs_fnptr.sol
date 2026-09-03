library L {
    struct Set { uint256[] vals; mapping(uint256 => uint256) idx; }
    function add(Set storage s, uint256 v) internal returns (bool) { if (s.idx[v] != 0) return false; s.vals.push(v); s.idx[v] = s.vals.length; return true; }
    function remove(Set storage s, uint256 v) internal returns (bool) { uint256 i = s.idx[v]; if (i == 0) return false; uint256 last = s.vals[s.vals.length - 1]; s.vals[i - 1] = last; s.idx[last] = i; s.vals.pop(); delete s.idx[v]; return true; }
    function len(Set storage s) internal view returns (uint256) { return s.vals.length; }
    function at(Set storage s, uint256 i) internal view returns (uint256) { return s.vals[i]; }
    function max(uint256 a, uint256 b) internal pure returns (uint256) { return a > b ? a : b; }
    function sum(uint256[] memory a) internal pure returns (uint256 r) { for (uint256 i; i < a.length; i++) r += a[i]; }
    function first(uint256[] storage a) internal view returns (uint256) { return a[0]; }
    function pushTwice(uint256[] storage a, uint256 v) internal { a.push(v); a.push(v); }
    function twice(uint8 x) internal pure returns (uint8) { return x * 2; }
}
library M { function neg(int256 x) internal pure returns (int256) { return -x; } function id(bytes memory b) internal pure returns (bytes memory) { return b; } }
contract LibsFnptr {
    using L for L.Set; using L for uint256[]; using L for uint8; using {L.max} for uint256; using M for *;
    L.Set set; uint256[] arr; uint256 counter;
    function setOps(uint256 a, uint256 b) external returns (bool, bool, bool, uint256, uint256) { bool r1 = set.add(a); bool r2 = set.add(b); bool r3 = set.remove(a); return (r1, r2, r3, set.len(), set.len() > 0 ? set.at(0) : 0); }
    function setRemoveMissing(uint256 a) external returns (bool) { return set.remove(a); }
    function arrOps(uint256 v) external returns (uint256, uint256) { arr.pushTwice(v); return (arr.first(), arr.length); }
    function arrEmptyFirst() external view returns (uint256) { return arr.first(); }
    function maxOp(uint256 a, uint256 b) external pure returns (uint256) { return a.max(b); }
    function twiceOp(uint8 a) external pure returns (uint8) { return a.twice(); }
    function negOp(int256 a) external pure returns (int256) { return a.neg(); }
    function idOp(bytes calldata b) external pure returns (bytes memory) { return bytes(b).id(); }
    function sumMem(uint256[] memory a) external pure returns (uint256) { return L.sum(a); }
    function fnPtrLocal(uint256 x, bool c) external pure returns (uint256) { function(uint256) internal pure returns (uint256) f = c ? _a : _b; return f(x); }
    function _a(uint256 x) internal pure returns (uint256) { return x + 1; }
    function _b(uint256 x) internal pure returns (uint256) { return x * 2; }
    function fnPtrArr(uint256 x, uint256 i) external pure returns (uint256) { function(uint256) internal pure returns (uint256)[3] memory fs = [_a, _b, _c]; return fs[i](x); }
    function _c(uint256 x) internal pure returns (uint256) { return x - 1; }
    function fnPtrDyn(uint256 x, uint256 n) external pure returns (uint256 r) { require(n < 4); function(uint256) internal pure returns (uint256)[] memory fs = new function(uint256) internal pure returns (uint256)[](n); for (uint256 i; i < n; i++) fs[i] = i % 2 == 0 ? _a : _b; r = x; for (uint256 i; i < n; i++) r = fs[i](r); }
    function fnPtrStruct(uint256 x) external pure returns (uint256) { F memory f = F(_a, _b); return f.g(f.f(x)); }
    struct F { function(uint256) internal pure returns (uint256) f; function(uint256) internal pure returns (uint256) g; }
    function fnPtrArg(uint256 x) external pure returns (uint256) { return _apply(_b, _apply(_a, x)); }
    function _apply(function(uint256) internal pure returns (uint256) f, uint256 x) internal pure returns (uint256) { return f(x); }
    function fnPtrRet(bool c) external pure returns (uint256) { return _pick(c)(10); }
    function _pick(bool c) internal pure returns (function(uint256) internal pure returns (uint256)) { return c ? _a : _c; }
    function fnPtrStorage(uint256 x, bool c) external returns (uint256) { stored = c ? _a : _b; return stored(x); }
    function(uint256) internal pure returns (uint256) stored;
    function fnPtrStorageUnset(uint256 x) external returns (uint256) { return stored(x); }
    function fnPtrEq(bool c) external pure returns (bool, bool) { function(uint256) internal pure returns (uint256) f = c ? _a : _b; return (f == _a, f != _b); }
    function fnPtrMapping(uint256 k, uint256 x) external returns (uint256) { fmap[k] = k % 2 == 0 ? _a : _b; return fmap[k](x); }
    mapping(uint256 => function(uint256) internal pure returns (uint256)) fmap;
    function fnPtrStateful(uint256 x) external returns (uint256) { function(uint256) internal returns (uint256) f = _count; f(x); return f(x); }
    function _count(uint256 x) internal returns (uint256) { counter += x; return counter; }
    function fnPtrLib(uint256 a, uint256 b) external pure returns (uint256) { function(uint256, uint256) internal pure returns (uint256) f = L.max; return f(a, b); }
    function fnPtrExt() external view returns (address, bytes4) { function(uint256) external pure returns (uint256) f = this.maxOp2; return (f.address, f.selector); }
    function maxOp2(uint256 a) external pure returns (uint256) { return a; }
    function fnPtrExtEq() external view returns (bool, bool) { function(uint256) external pure returns (uint256) f = this.maxOp2; function(uint256) external pure returns (uint256) g = this.maxOp2; return (f == g, f.selector == this.maxOp2.selector); }
    function fnPtrExtEnc() external view returns (bytes memory) { return abi.encode(this.maxOp2); }
    function fnPtrExtArg(function(uint256) external pure returns (uint256) f) external pure returns (address, bytes4) { return (f.address, f.selector); }
    function fnPtrExtArgArr(function(uint256) external pure returns (uint256)[] calldata fs, uint256 i) external pure returns (bytes4) { return fs[i].selector; }
    function fnPtrExtStore(function(uint256) external pure returns (uint256) f) external returns (address, bytes4) { extStored = f; return (extStored.address, extStored.selector); }
    function(uint256) external pure returns (uint256) extStored;
    function fnPtrExtDirty(uint256 raw) external pure returns (address, bytes4) { function(uint256) external pure returns (uint256) f; assembly { f.address := raw f.selector := raw } return (f.address, f.selector); }
    function fnPtrExtDirtyEnc(uint256 raw) external pure returns (bytes memory) { function(uint256) external pure returns (uint256) f; assembly { f.address := raw f.selector := raw } return abi.encode(f); }
    function fnPtrExtDirtyStore(uint256 raw) external returns (bytes32 r) { function(uint256) external pure returns (uint256) f; assembly { f.address := raw f.selector := raw } extStored = f; return keccak256(abi.encode(extStored, extStored.address, extStored.selector)); }
    function fnPtrExtDelete() external returns (address) { extStored = this.maxOp2; delete extStored; return extStored.address; }
    function fnPtrInternalToExtSel() external pure returns (bytes4) { return this.maxOp2.selector; }
    function usingWild(int256 a) external pure returns (int256) { return a.neg().neg(); }
    function storageRefLocal(uint256 v) external returns (uint256) { uint256[] storage p = arr; p.push(v); uint256[] storage q = p; q.push(v + 1); return arr[0] + arr[1] + arr.length; }
    function storageRefSelect(bool c, uint256 v) external returns (uint256, uint256) { (c ? arr : arr2).push(v); return (arr.length, arr2.length); }
    uint256[] arr2;
    function storageRefTernary(bool c, uint256 v) external returns (uint256) { uint256[] storage p = c ? arr : arr2; p.push(v); return arr.length * 10 + arr2.length; }
    function storageStructRef(uint256 v) external returns (uint256) { L.Set storage s = set; s.vals.push(v); return set.vals.length; }
    function libStorageIdx(uint256 a) external returns (uint256) { set.add(a); return set.at(1); }
}
