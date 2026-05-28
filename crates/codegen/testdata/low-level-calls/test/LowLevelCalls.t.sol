// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/LowLevelCalls.sol";

contract LowLevelCallsTest {
    Target target;
    LowLevelCalls caller;

    function setUp() public {
        target = new Target();
        caller = new LowLevelCalls();
    }

    function test_call_setValue() public {
        bool success = caller.callTarget(address(target), 42);
        assert(success);
        assert(target.value() == 42);
    }

    function test_staticcall_getValue() public {
        target.setValue(123);
        uint256 result = caller.staticCallTarget(address(target));
        assert(result == 123);
    }

    function test_delegatecall_setValue() public {
        bool success = caller.delegateCallTarget(address(target), 999);
        assert(success);
        assert(caller.value() == 999);
        assert(target.value() == 0);
    }

    function test_call_add() public {
        (bool success, bytes memory data) = address(target).call(
            abi.encodeWithSignature("add(uint256,uint256)", 10, 20)
        );
        assert(success);
        uint256 result = abi.decode(data, (uint256));
        assert(result == 30);
    }

    function test_staticcall_add() public view {
        (bool success, bytes memory data) = address(target).staticcall(
            abi.encodeWithSignature("add(uint256,uint256)", 5, 7)
        );
        assert(success);
        uint256 result = abi.decode(data, (uint256));
        assert(result == 12);
    }

    function test_call_nonexistent_function() public {
        (bool success, ) = address(target).call(
            abi.encodeWithSignature("nonexistent()")
        );
        assert(!success);
    }

    function test_staticcall_state_changing_reverts() public {
        (bool success, ) = address(target).staticcall(
            abi.encodeWithSignature("setValue(uint256)", 42)
        );
        assert(!success);
    }
}
