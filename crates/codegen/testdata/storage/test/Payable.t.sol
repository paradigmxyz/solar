// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "../src/Payable.sol";

contract PayableTest {
    Payable payable_contract;

    function setUp() public {
        payable_contract = new Payable();
    }

    function test_InitialBalanceZero() public view {
        assert(payable_contract.getBalance() == 0);
    }

    function test_DepositWithValue() public {
        payable_contract.deposit{value: 1 ether}();
        assert(payable_contract.balance() == 1 ether);
    }

    function test_MultipleDeposits() public {
        payable_contract.deposit{value: 1 ether}();
        payable_contract.deposit{value: 2 ether}();
        assert(payable_contract.balance() == 3 ether);
    }

    function test_ViewRejectsValue() public view {
        uint256 bal = payable_contract.getBalance();
        assert(bal == 0);
    }

    function test_NonPayableRejectsValue() public {
        payable_contract.deposit{value: 1 ether}();
        payable_contract.withdraw();
        assert(payable_contract.balance() == 0);
    }
}
