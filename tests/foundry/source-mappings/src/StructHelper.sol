// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

contract StructHelper {
    struct S {
        int256 x;
        int256 y;
        int256 e;
    }

    function set(int256 x, int256 e) internal pure returns (S memory) {
        return S({x: x, y: 0, e: e});
    }

    function sum(int256 a, int256 b) external pure returns (int256) {
        S memory p = set(a, 1);
        S memory q = set(b, 2);
        S memory r = set(a, b);
        return p.x + q.x + r.e;
    }
}
