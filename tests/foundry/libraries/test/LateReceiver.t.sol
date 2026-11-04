// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/LateReceiver.sol";

contract LateReceiverTest {
    LateReceiver target;

    function setUp() public {
        target = new LateReceiver();
    }

    function test_attachedReceiverReadsBeforeArguments() public {
        require(target.attached(), "attached");
    }

    function test_attachedReceiverReadsBeforeArgumentsInTry() public {
        require(target.attachedTry(), "try");
    }

    function test_attachedTextReceiverReadsBeforeArguments() public {
        require(target.attachedText(), "text");
    }

    function test_attachedTextReceiverReadsBeforeArgumentsInTry() public {
        require(target.attachedTextTry(), "text try");
    }
}
