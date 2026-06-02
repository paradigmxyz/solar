// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/ReceiveFallback.sol";

contract ReceiveFallbackTest {
    ReceiveFallback target;

    function setUp() public {
        target = new ReceiveFallback();
    }

    function test_receive_with_empty_calldata() public {
        uint256 balanceBefore = target.getBalance();
        uint256 receiveCallsBefore = target.receiveCalls();
        
        (bool success,) = address(target).call{value: 1 ether}("");
        assert(success);
        
        assert(target.receiveCalls() == receiveCallsBefore + 1);
        assert(target.getBalance() == balanceBefore + 1 ether);
        assert(target.totalReceived() == 1 ether);
    }

    function test_fallback_with_calldata() public {
        uint256 balanceBefore = target.getBalance();
        uint256 fallbackCallsBefore = target.fallbackCalls();
        
        (bool success,) = address(target).call{value: 1 ether}("0x1234");
        assert(success);
        
        assert(target.fallbackCalls() == fallbackCallsBefore + 1);
        assert(target.getBalance() == balanceBefore + 1 ether);
        assert(target.totalReceived() == 1 ether);
    }

    function test_receive_multiple_calls() public {
        (bool success1,) = address(target).call{value: 1 ether}("");
        assert(success1);
        
        (bool success2,) = address(target).call{value: 2 ether}("");
        assert(success2);
        
        assert(target.receiveCalls() == 2);
        assert(target.totalReceived() == 3 ether);
        assert(target.getBalance() == 3 ether);
    }

    function test_fallback_multiple_calls() public {
        (bool success1,) = address(target).call{value: 1 ether}(hex"deadbeef");
        assert(success1);
        
        (bool success2,) = address(target).call{value: 2 ether}(hex"cafebabe");
        assert(success2);
        
        assert(target.fallbackCalls() == 2);
        assert(target.totalReceived() == 3 ether);
        assert(target.getBalance() == 3 ether);
    }

    function test_mixed_receive_and_fallback() public {
        (bool success1,) = address(target).call{value: 1 ether}("");
        assert(success1);
        
        (bool success2,) = address(target).call{value: 2 ether}(hex"1234");
        assert(success2);
        
        (bool success3,) = address(target).call{value: 3 ether}("");
        assert(success3);
        
        assert(target.receiveCalls() == 2);
        assert(target.fallbackCalls() == 1);
        assert(target.totalReceived() == 6 ether);
        assert(target.getBalance() == 6 ether);
    }

    function test_receive_zero_value() public {
        (bool success,) = address(target).call{value: 0}("");
        assert(success);
        
        assert(target.receiveCalls() == 1);
        assert(target.totalReceived() == 0);
    }

    function test_fallback_zero_value() public {
        (bool success,) = address(target).call{value: 0}(hex"abcd");
        assert(success);
        
        assert(target.fallbackCalls() == 1);
        assert(target.totalReceived() == 0);
    }
}
