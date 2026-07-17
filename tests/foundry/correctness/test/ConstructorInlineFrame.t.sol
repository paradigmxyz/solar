// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ConstructorInlineFrame {
    uint256 public value;

    constructor(uint256 input, uint256 poison) {
        value = helper(input);
    }

    function helper(uint256 input) internal pure returns (uint256 result) {
        if (input == 0) result = 7;
        else result = input + 1;
    }
}

contract ConstructorInlineFrameTest {
    function testConstructorInlineFrame() public {
        ConstructorInlineFrame zero = new ConstructorInlineFrame(0, 0x1234);
        ConstructorInlineFrame nonzero = new ConstructorInlineFrame(41, 0x1234);

        assert(zero.value() == 7);
        assert(nonzero.value() == 42);
    }
}
