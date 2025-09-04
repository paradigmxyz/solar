//@ revisions: ascii unicode short
//@[ascii] compile-flags: --error-format-human=ascii
//@[unicode] compile-flags: --error-format-human=unicode
//@[short] compile-flags: --error-format-human=short

// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract TestError {
    uint256 public number;

    function test() public {
        uint256 x = 1;
        uint256 y = 2;
        uint256 z = x + y
    } //~ ERROR: expected one of
}
