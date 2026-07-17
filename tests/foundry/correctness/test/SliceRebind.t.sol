// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract SliceRebind {
    function forward(bytes calldata data, uint256 start) external returns (uint256) {
        data = data[start:];
        helper(1);
        return data.length;
    }

    function helper(uint256 x) internal pure returns (uint256) {
        if (x != 0) return x;
        return 0;
    }
}

contract SliceRebindTest {
    function testSliceRebindAcrossNestedLowering() public {
        SliceRebind target = new SliceRebind();
        bytes memory data = hex"010203040506";

        assert(target.forward(data, 2) == 4);
    }
}
