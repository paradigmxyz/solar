// SPDX-License-Identifier: MIT
pragma solidity >=0.8.0;

// Regression workload for scalar replacement, joined bounds, shared helpers,
// expression scheduling, and packed/full-word storage forwarding.
contract CompilerOptimizations {
    struct Pair { uint256 x; uint256 y; }
    uint256 public word;
    uint8 public low;
    uint8 public high;

    function aggregate(bool choice, uint256 n) external pure returns (uint256) {
        Pair memory p;
        if (choice) p.x = 7; else p.x = 11;
        for (uint256 i; i < n; ++i) { p.x += i; p.y += p.x; }
        return p.y;
    }

    function bounds(bool choice, uint256 a, uint256 b) external pure returns (uint256) {
        uint256 x;
        if (choice) { require(a < 100); x = a; }
        else { require(b < 80); x = b; }
        require(x < 100);
        return x + 1;
    }

    function packed(uint8 a, uint8 b) external {
        low = a;
        high = b;
    }

    function overwrite(bool choice, uint256 x) external {
        word = x;
        if (choice) word = x + 1; else word = x + 2;
    }

    function stackShape(uint256 a, uint256 b) external pure returns (uint256 d, uint256 next, bool equal) {
        unchecked { d = a - b; next = d + 1; }
        equal = a == b;
    }

    function first(uint256 x) external pure returns (uint256) { return helper(false, x); }
    function second(uint256 x) external pure returns (uint256) { return helper(false, x); }
    function third(uint256 x) external pure returns (uint256) { return helper(false, x); }
    function generic(bool mode, uint256 x) external pure returns (uint256) { return helper(mode, x); }

    function helper(bool mode, uint256 x) internal pure returns (uint256) {
        if (mode) return x * x * x * x;
        return x + 7;
    }
}
