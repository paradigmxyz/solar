// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface Vm {
    function roll(uint256) external;
    function warp(uint256) external;
}

// An environment value captured in a local must keep its read-time value even
// when a cheatcode mutates the environment later in the frame: rematerializing
// the read at the use site would observe the mutated value.
contract EnvReadsTest {
    Vm constant vm = Vm(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D);

    function testBlockNumberCapturedBeforeRoll() public {
        uint256 captured = block.number;
        vm.roll(block.number + 41);
        assert(captured + 41 == block.number);
    }

    function testTimestampCapturedBeforeWarp() public {
        uint256 captured = block.timestamp;
        vm.warp(block.timestamp + 1 days);
        assert(captured + 1 days == block.timestamp);
    }
}
