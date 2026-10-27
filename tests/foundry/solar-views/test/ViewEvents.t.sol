// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../src/ViewEvents.sol";

interface Vm {
    struct Log {
        bytes32[] topics;
        bytes data;
        address emitter;
    }

    function getRecordedLogs() external returns (Log[] memory logs);
    function recordLogs() external;
}

contract ViewEventsTest {
    Vm constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));
    ViewEvents views;
    CopyEvents copies;

    function setUp() public {
        views = new ViewEvents();
        copies = new CopyEvents();
    }

    function payload(bytes memory b, string memory s, uint256 n) internal pure returns (bytes memory) {
        uint256[] memory words = new uint256[](n);
        for (uint256 i; i < n; ++i) {
            words[i] = uint256(keccak256(abi.encode(b, i)));
        }
        bytes[] memory items = new bytes[](2);
        items[0] = bytes.concat(b, bytes(s));
        items[1] = b;
        uint64[] memory amounts = new uint64[](n + 1);
        for (uint256 i; i <= n; ++i) {
            amounts[i] = uint64(i * 7);
        }
        return abi.encode(b, s, words, items, Order(n, bytes(s), amounts));
    }

    function assertSameLogs(Vm.Log[] memory a, Vm.Log[] memory b) internal pure {
        require(a.length == b.length, "log count");
        for (uint256 i; i < a.length; ++i) {
            require(keccak256(abi.encode(a[i].topics)) == keccak256(abi.encode(b[i].topics)), "topics");
            require(keccak256(a[i].data) == keccak256(b[i].data), "data");
        }
    }

    function check(bytes memory data) internal {
        vm.recordLogs();
        views.emitCalldata(data);
        Vm.Log[] memory viewed = vm.getRecordedLogs();
        copies.emitCalldata(data);
        assertSameLogs(viewed, vm.getRecordedLogs());
        views.emitMemory(data);
        viewed = vm.getRecordedLogs();
        copies.emitMemory(data);
        assertSameLogs(viewed, vm.getRecordedLogs());
    }

    function test_Short() public {
        check(payload(hex"0102", "s", 1));
    }

    function test_Long() public {
        check(payload(bytes("a payload of more than one word, to span two words"), "and a long string too, over 32", 5));
    }

    function test_Empty() public {
        check(payload("", "", 0));
    }

    function testFuzz_Logs(bytes memory b, string memory s, uint8 n) public {
        check(payload(b, s, n % 16));
    }
}
