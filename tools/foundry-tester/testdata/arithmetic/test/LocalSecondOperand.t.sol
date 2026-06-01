// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/LocalSecondOperand.sol";

contract LocalSecondOperandTest {
    LocalSecondOperand target;

    function setUp() public {
        target = new LocalSecondOperand();
        target.setX(10);
    }

    function test_DivideByLocal() public {
        (uint256 a, uint256 b) = target.divideByLocal();
        assert(a == 100);  // 1000 / 10
        assert(b == 200);  // 2000 / 10
    }

    function test_LocalAsFirst() public {
        (uint256 a, uint256 b) = target.localAsFirst();
        assert(a == 11);  // 10 + 1
        assert(b == 12);  // 10 + 2
    }

    function test_SubtractFromLocal() public {
        (uint256 a, uint256 b) = target.subtractFromLocal();
        assert(a == 990);  // 1000 - 10
        assert(b == 490);  // 500 - 10
    }

    function test_ModuloByLocal() public {
        target.setX(7);
        (uint256 a, uint256 b) = target.moduloByLocal();
        assert(a == 6);   // 1000 % 7
        assert(b == 5);   // 2000 % 7
    }
}
