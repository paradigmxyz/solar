// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../src/NestedMemory.sol";

contract NestedMemoryTest {
    NestedMemory public nm;

    struct Array { uint256[] data; }

    function reserve(Array memory a, uint256 n) internal pure {
        a.data = new uint256[](n);
    }

    function resize(Array memory a, uint256 n) internal pure {
        reserve(a, n);
        assembly {
            let data := mload(a)
            if n {
                calldatacopy(add(data, 32), calldatasize(), shl(5, n))
            }
            mstore(data, n)
        }
    }

    function testResizeAfterHelper() public pure {
        Array memory a;
        resize(a, 37);
        require(a.data.length == 37);
        for (uint256 i; i < a.data.length; ++i) require(a.data[i] == 0);
    }


    function setUp() public {
        nm = new NestedMemory();
    }

    function testNestedSum() public view {
        uint256 result = nm.nestedSum();
        require(result == 6, "nestedSum should return 6");
    }

    function testNestedValues() public view {
        (uint256 a, uint256 b, uint256 c) = nm.nestedValues();
        require(a == 100, "a mismatch");
        require(b == 200, "b mismatch");
        require(c == 300, "c mismatch");
    }

    function testMultipleNested() public view {
        uint256 result = nm.multipleNested();
        require(result == 66, "multipleNested should return 66");
    }
}
