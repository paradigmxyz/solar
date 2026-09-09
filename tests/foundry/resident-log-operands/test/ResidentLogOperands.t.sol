// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/ResidentLogOperands.sol";

interface Vm {
    struct Log {
        bytes32[] topics;
        bytes data;
        address emitter;
    }
    function recordLogs() external;
    function getRecordedLogs() external returns (Log[] memory);
}

contract ResidentLogOperandsTest {
    Vm constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));
    ResidentLogOperands target;

    function setUp() public {
        target = new ResidentLogOperands();
    }

    function checkLog3(Vm.Log[] memory logs, bytes32 signature, address first, address second, uint256 amount)
        internal view
    {
        require(logs.length == 1, "log count");
        require(logs[0].emitter == address(target), "emitter");
        require(logs[0].topics.length == 3, "topic count");
        require(logs[0].topics[0] == signature, "signature");
        require(logs[0].topics[1] == bytes32(uint256(uint160(first))), "first topic");
        require(logs[0].topics[2] == bytes32(uint256(uint160(second))), "second topic");
        require(logs[0].data.length == 32, "payload length");
        require(keccak256(logs[0].data) == keccak256(abi.encode(amount)), "payload");
    }

    function testDistinctTopicsAndAllowance() public {
        address owner = address(0x1111);
        address spender = address(0x2222);
        vm.recordLogs();
        target.approve(owner, spender, type(uint256).max);
        checkLog3(vm.getRecordedLogs(), keccak256("Approval(address,address,uint256)"), owner, spender, type(uint256).max);
        require(target.allowance(owner, spender) == type(uint256).max, "allowance");
        require(target.allowance(spender, owner) == 0, "reversed mapping");
    }

    function testLiveTopicThenStorage() public {
        address token = address(0x1234);
        vm.recordLogs();
        target.setFee(token, 123);
        Vm.Log[] memory logs = vm.getRecordedLogs();
        require(logs.length == 1 && logs[0].topics.length == 2, "log shape");
        require(logs[0].emitter == address(target), "emitter");
        require(logs[0].topics[0] == keccak256("Fee(address,uint256)"), "signature");
        require(logs[0].topics[1] == bytes32(uint256(uint160(token))), "live topic");
        require(logs[0].data.length == 32 && keccak256(logs[0].data) == keccak256(abi.encode(uint256(123))), "payload");
        require(target.fees(token) == 123, "later store");
        require(target.fees(address(0)) == 0, "other key");
    }

    function testRepeatedLiveTopicZeroPayload() public {
        address actor = address(type(uint160).max);
        vm.recordLogs();
        target.repeated(actor, 0);
        checkLog3(vm.getRecordedLogs(), keccak256("Repeated(address,address,uint256)"), actor, actor, 0);
        require(target.saved() == type(uint160).max, "retained actor");
    }

    function testUnrelatedRetainedValue() public {
        vm.recordLogs();
        uint256 result = target.retain(address(0), address(0x3333), 7, type(uint256).max);
        checkLog3(vm.getRecordedLogs(), keccak256("Approval(address,address,uint256)"), address(0), address(0x3333), 7);
        require(result == type(uint256).max && target.saved() == result, "retained value");
    }

    function testRepeatedZeroLiteralAndLaterStorage() public {
        address actor = address(0x5555);
        vm.recordLogs();
        target.repeatedZero(actor, type(uint256).max);
        checkLog3(vm.getRecordedLogs(), bytes32(0), actor, actor, type(uint256).max);
        require(target.saved() == uint256(uint160(actor)), "later actor store");
    }
}
