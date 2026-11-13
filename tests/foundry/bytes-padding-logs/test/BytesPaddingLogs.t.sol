// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/BytesPaddingLogs.sol";

interface Vm {
    struct Log {
        bytes32[] topics;
        bytes data;
        address emitter;
    }

    function getRecordedLogs() external returns (Log[] memory logs);
    function recordLogs() external;
}

contract BytesPaddingLogsTest {
    Vm constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));
    BytesPaddingLogs target;

    function setUp() public {
        target = new BytesPaddingLogs();
    }

    function test_EmitDecoded5() public {
        _emitDecoded(5);
    }

    function test_EmitDecoded33() public {
        _emitDecoded(33);
    }

    function test_EmitReturned5() public {
        vm.recordLogs();
        target.emitReturned(5);
        _check(5);
    }

    function test_EmitReturned33() public {
        vm.recordLogs();
        target.emitReturned(33);
        _check(33);
    }

    function test_EmitPacked33() public {
        vm.recordLogs();
        target.emitPacked(33);
        _check(33);
    }

    function test_EmitPacked63() public {
        vm.recordLogs();
        target.emitPacked(63);
        _check(63);
    }

    function _emitDecoded(uint256 n) private {
        bytes memory encoded = abi.encode(_expected(n));
        // Dirty the padding of the encoded bytes without changing its length.
        encoded[encoded.length - 1] = 0xff;
        vm.recordLogs();
        target.emitDecoded(encoded);
        _check(n);
    }

    function _check(uint256 n) private {
        bytes memory expected = _expected(n);
        Vm.Log[] memory logs = vm.getRecordedLogs();
        require(logs.length == 2);
        require(logs[0].topics.length == 1);
        require(keccak256(logs[0].data) == keccak256(abi.encode(expected)));
        require(logs[1].topics.length == 2);
        require(logs[1].topics[1] == keccak256(expected));
        require(logs[1].data.length == 0);
    }

    function _expected(uint256 n) private pure returns (bytes memory b) {
        b = new bytes(n);
        for (uint256 i; i < n; ++i) b[i] = "A";
    }
}
