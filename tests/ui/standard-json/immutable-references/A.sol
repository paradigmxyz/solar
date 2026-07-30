// ported-from: test/cmdlineTests/standard_immutable_references/input.json
// SPDX-License-Identifier: GPL-3.0
pragma solidity >=0.0;

contract Widths {
    uint8 immutable a;
    address immutable b;

    constructor(uint8 a_, address b_) {
        a = a_;
        b = b_;
    }

    function values() external view returns (uint8, address) {
        return (a, b);
    }
}

contract A {
    uint256 immutable x = 1 + 3;

    function f() public pure returns (uint256) {
        return x;
    }
}
