// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/Events.sol";

contract EventsTest {
    Events public events;

    event SimpleEvent(uint256 value);
    event Transfer(address indexed from, address indexed to, uint256 value);

    function setUp() public {
        events = new Events();
    }

    function test_EmitSimple() public {
        events.emitSimple(42);
    }

    function test_EmitTransfer() public {
        events.emitTransfer(address(0x1), address(0x2), 100);
    }
}
