contract Aliasing {
    struct S { uint256 a; uint256[] arr; }
    S[] items;
    S single;
    mapping(uint256 => S) ms;
    uint256[] nums;

    function storageRefs(uint256 v) external returns (uint256, uint256) { items.push(); S storage p = items[0]; S storage q = items[0]; p.a = v; q.a += 1; p.arr.push(v); return (q.a, q.arr.length); }
    function refAfterPush(uint256 v) external returns (uint256, uint256) { items.push(); S storage p = items[0]; p.a = v; items.push(); items.push(); p.a += 1; return (items[0].a, items.length); }
    function refAfterPop(uint256 v) external returns (uint256) { items.push(); items.push(); S storage p = items[1]; p.a = v; items.pop(); items.push(); return items[1].a; }
    function memAlias(uint256 v) external pure returns (uint256, uint256) { uint256[] memory a = new uint256[](2); uint256[] memory b = a; b[0] = v; a[1] = v + 1; return (a[0], b[1]); }
    function memStructAlias(uint256 v) external pure returns (uint256, uint256) { S memory a; S memory b = a; b.a = v; a.arr = new uint256[](1); b.arr[0] = v + 2; return (a.a, a.arr[0]); }
    function memCopyOnAssign(uint256 v) external pure returns (uint256, uint256) { uint256[] memory a = new uint256[](1); a[0] = v; uint256[] memory b = new uint256[](1); b = a; b[0] = v + 5; return (a[0], b[0]); }
    function memToStorageCopy(uint256 v) external returns (uint256, uint256) { uint256[] memory a = new uint256[](2); a[0] = v; nums = a; a[1] = v + 1; return (nums[1], a[1]); }
    function storageToMemCopy(uint256 v) external returns (uint256, uint256) { nums.push(v); uint256[] memory a = nums; a[0] = v + 3; return (nums[0], a[0]); }
    function structStorageToMem(uint256 v) external returns (uint256, uint256) { single.a = v; single.arr.push(v); S memory m = single; m.a += 1; m.arr[0] += 1; return (single.a * 1000 + single.arr[0], m.a * 1000 + m.arr[0]); }
    function structMemToStorage(uint256 v) external returns (uint256, uint256) { S memory m; m.a = v; m.arr = new uint256[](1); m.arr[0] = v; single = m; m.a += 1; m.arr[0] += 1; return (single.a * 1000 + single.arr[0], m.a * 1000 + m.arr[0]); }
    function mappingStructRef(uint256 v) external returns (uint256) { S storage p = ms[v]; p.a = 1; ms[v].a += 1; p.arr.push(7); return p.a * 10 + ms[v].arr.length; }
    function paramRef(uint256 v) external pure returns (uint256, uint256) { uint256[] memory a = new uint256[](1); a[0] = v; mutate(a); return (a[0], a.length); }
    function mutate(uint256[] memory x) internal pure { x[0] += 1; }
    function paramRefReassign(uint256 v) external pure returns (uint256) { uint256[] memory a = new uint256[](1); a[0] = v; reassign(a); return a[0]; }
    function reassign(uint256[] memory x) internal pure { x = new uint256[](1); x[0] = 99; }
    function returnedRef(uint256 v) external returns (uint256) { items.push(); pick().a = v; return items[0].a; }
    function pick() internal view returns (S storage) { return items[0]; }
    function deleteViaRef(uint256 v) external returns (uint256, uint256) { items.push(); S storage p = items[0]; p.a = v; p.arr.push(v); delete items[0]; return (p.a, p.arr.length); }
    function swapMem(uint256 v) external pure returns (uint256, uint256) { uint256[] memory a = new uint256[](1); uint256[] memory b = new uint256[](1); a[0] = v; b[0] = v + 1; (a, b) = (b, a); return (a[0], b[0]); }
    function bytesAlias(uint256 v) external pure returns (bytes memory, bytes memory) { bytes memory a = new bytes(2); bytes memory b = a; b[0] = bytes1(uint8(v)); return (a, b); }
    function stringStorageRef(uint256 v) external returns (string memory) { str = "ab"; string storage r = str; bytes(r).push(bytes1(uint8(v))); return str; }
    string str;
    function nestedArrayRef(uint256 v) external returns (uint256) { nested.push(); uint256[] storage inner = nested[0]; inner.push(v); nested.push(); inner.push(v + 1); return nested[0].length * 100 + nested[0][1]; }
    uint256[][] nested;
}
