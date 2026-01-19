// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/Events.sol";

interface Vm {
    function expectEmit(bool, bool, bool, bool) external;
}

contract EventsTest {
    Vm constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));
    Events public events;

    event SimpleEvent(uint256 value);
    event Transfer(address indexed from, address indexed to, uint256 value);

    function setUp() public {
        events = new Events();
    }

    function test_EmitSimple() public {
        vm.expectEmit(false, false, false, true);
        emit SimpleEvent(42);
        events.emitSimple(42);
    }

    function test_EmitTransfer() public {
        vm.expectEmit(true, true, false, true);
        emit Transfer(address(0x1), address(0x2), 100);
        events.emitTransfer(address(0x1), address(0x2), 100);
    }
}
