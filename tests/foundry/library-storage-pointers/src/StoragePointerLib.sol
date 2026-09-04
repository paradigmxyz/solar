// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// A storage reference crosses an external library call boundary as its slot number, in both
// directions. The expected values below are solc's.
library StoragePointerLib {
    struct Plain {
        uint256 a;
        uint256 b;
    }

    struct Nested {
        uint256[] arr;
        mapping(uint256 => uint256[]) byKey;
    }

    function arrRef(uint256[] storage a) external pure returns (uint256[] storage) {
        return a;
    }

    function bytesRef(bytes storage b) external pure returns (bytes storage) {
        return b;
    }

    function plainRef(Plain storage p) external pure returns (Plain storage) {
        return p;
    }

    function pairRef(uint256[] storage a) external view returns (uint256, uint256[] storage) {
        return (a.length, a);
    }

    function firstOf(uint256[] storage a) external view returns (uint256) {
        return a[0];
    }

    // A pointer to a nested object is one slot word too.
    function memberArr(Nested storage n) external view returns (uint256[] storage) {
        return n.arr;
    }

    function valueArr(Nested storage n, uint256 key) external view returns (uint256[] storage) {
        return n.byKey[key];
    }

    function element(uint256[][] storage aa, uint256 i) external view returns (uint256[] storage) {
        return aa[i];
    }
}

contract StoragePointers {
    using StoragePointerLib for uint256[];

    uint256[] internal nums;
    bytes internal bs;
    StoragePointerLib.Plain internal plain;
    StoragePointerLib.Nested internal nested;
    uint256[][] internal grid;
    uint256 internal guard;

    constructor() {
        nums.push(5);
        nums.push(9);
        bs = hex"aabbcc";
        plain.a = 11;
        plain.b = 12;
        nested.arr.push(4);
        nested.byKey[1].push(6);
        grid.push();
        grid[0].push(1);
        grid[0].push(2);
        guard = 77;
    }

    function len() external view returns (uint256) {
        return StoragePointerLib.arrRef(nums).length;
    }

    function sum() external view returns (uint256 s) {
        uint256[] storage r = StoragePointerLib.arrRef(nums);
        for (uint256 i; i < r.length; i++) {
            s += r[i];
        }
    }

    function pushThrough(uint256 v) external returns (uint256, uint256) {
        StoragePointerLib.arrRef(nums).push(v);
        return (nums.length, guard);
    }

    function bytesLen() external view returns (uint256) {
        return StoragePointerLib.bytesRef(bs).length;
    }

    function bytesPushThrough(uint256 v) external returns (uint256, uint256) {
        StoragePointerLib.bytesRef(bs).push(bytes1(uint8(v)));
        return (bs.length, guard);
    }

    function members() external view returns (uint256, uint256) {
        StoragePointerLib.Plain storage p = StoragePointerLib.plainRef(plain);
        return (p.a, p.b);
    }

    function writeMember(uint256 v) external returns (uint256, uint256) {
        StoragePointerLib.plainRef(plain).b = v;
        return (plain.b, guard);
    }

    function pair() external view returns (uint256, uint256) {
        (uint256 n, uint256[] storage r) = StoragePointerLib.pairRef(nums);
        return (n, r.length);
    }

    function chained() external view returns (uint256) {
        return StoragePointerLib.firstOf(StoragePointerLib.arrRef(nums));
    }

    function attachedLen() external view returns (uint256) {
        return nums.arrRef().length;
    }

    function attachedPush(uint256 v) external returns (uint256, uint256) {
        nums.arrRef().push(v);
        return (nums.length, guard);
    }

    function memberArrLen() external view returns (uint256) {
        return StoragePointerLib.memberArr(nested).length;
    }

    function memberWrite(uint256 i, uint256 v) external returns (uint256, uint256) {
        StoragePointerLib.memberArr(nested)[i] = v;
        return (nested.arr[i], guard);
    }

    function valuePush(uint256 key, uint256 v) external returns (uint256, uint256) {
        StoragePointerLib.valueArr(nested, key).push(v);
        return (nested.byKey[key].length, guard);
    }

    function valueFirst(uint256 key) external view returns (uint256) {
        return StoragePointerLib.valueArr(nested, key)[0];
    }

    function gridPop() external returns (uint256, uint256) {
        StoragePointerLib.element(grid, 0).pop();
        return (grid[0].length, guard);
    }
}
