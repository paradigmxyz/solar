// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// A storage reference crosses an external library call boundary as its slot number, in both
// directions. The expected values below are solc's.
library StoragePointerLib {
    struct Plain {
        uint256 a;
        uint256 b;
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
}

contract StoragePointers {
    using StoragePointerLib for uint256[];

    uint256[] internal nums;
    bytes internal bs;
    StoragePointerLib.Plain internal plain;
    uint256 internal guard;

    constructor() {
        nums.push(5);
        nums.push(9);
        bs = hex"aabbcc";
        plain.a = 11;
        plain.b = 12;
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
}
