// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/Signatures.sol";

interface Vm {
    struct Log {
        bytes32[] topics;
        bytes data;
        address emitter;
    }
    function recordLogs() external;
    function getRecordedLogs() external returns (Log[] memory);
}

contract SignaturesTest {
    Vm constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));

    function test_selectors() public {
        assert(new C().selectors());
    }

    function test_errorData() public {
        assert(new C().errorData());
    }

    function test_eventTopic() public {
        C c = new C();
        vm.recordLogs();
        c.fire();
        Vm.Log[] memory logs = vm.getRecordedLogs();
        assert(logs.length == 1);
        assert(logs[0].emitter == address(c));
        assert(logs[0].topics.length == 1);
        assert(logs[0].topics[0] == keccak256("E(address,uint8,(address,uint8,uint256))"));
        assert(keccak256(logs[0].data) == keccak256(abi.encode(
            address(1), uint8(1), L.S(C(address(2)), L.Kind.A, 3)
        )));
    }
}
