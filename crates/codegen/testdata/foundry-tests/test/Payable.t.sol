// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../src/Payable.sol";

contract PayableTest {
    Payable public target;

    function setUp() public {
        target = new Payable();
    }

    function testDepositWithValue() public {
        target.deposit{value: 1 ether}();
        require(target.totalReceived() == 1 ether, "totalReceived should be 1 ether");
        require(target.getBalance() == 1 ether, "balance should be 1 ether");
    }

    function testDepositMultiple() public {
        target.deposit{value: 1 ether}();
        target.deposit{value: 2 ether}();
        require(target.totalReceived() == 3 ether, "totalReceived should be 3 ether");
    }

    function testNonPayableWithValueReverts() public {
        (bool success,) = address(target).call{value: 1 ether}(
            abi.encodeWithSelector(target.nonPayable.selector)
        );
        require(!success, "nonPayable with value should revert");
    }

    function testNonPayableWithoutValue() public {
        uint256 result = target.nonPayable();
        require(result == 42, "nonPayable should return 42");
    }

    function testReceiveFunction() public {
        (bool success,) = address(target).call{value: 1 ether}("");
        require(success, "receive should succeed");
        require(target.totalReceived() == 1 ether, "totalReceived should be 1 ether");
    }

    function testGetBalanceIsView() public {
        target.deposit{value: 1 ether}();
        uint256 balance = target.getBalance();
        require(balance == 1 ether, "getBalance should return 1 ether");
    }

    function testViewFunctionWithValueReverts() public {
        (bool success,) = address(target).call{value: 1 ether}(
            abi.encodeWithSelector(target.getBalance.selector)
        );
        require(!success, "view function with value should revert");
    }
}
