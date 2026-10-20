// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/Events.sol";

interface Vm {
    struct Log {
        bytes32[] topics;
        bytes data;
        address emitter;
    }

    function expectEmit(bool, bool, bool, bool) external;
    function expectRevert(bytes calldata) external;
    function getRecordedLogs() external returns (Log[] memory logs);
    function recordLogs() external;
}

contract EventsTest {
    Vm constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));
    bytes32 constant DEFAULT_TRIPLE_TOPIC =
        0x46700b4d40ac5c35af2c22dda2787a91eb567b06c924a8fb8ae9a05b20c08c21;
    bytes32 constant DEFAULT_DYNAMIC_TOPIC =
        0x290decd9548b62a8d60345a988386fc84ba6bc95484008f6362f93160ef3e563;
    Events public events;

    event SimpleEvent(uint256 value);
    event Transfer(address indexed from, address indexed to, uint256 value);

    function setUp() public {
        events = new Events();
    }

    function test_EmitSignedDynamic() public {
        vm.recordLogs();
        events.emitSignedDynamic();
        Vm.Log[] memory logs = vm.getRecordedLogs();
        require(logs.length == 1);
        require(logs[0].topics.length == 2);
        require(logs[0].topics[1] == keccak256(abi.encode(type(uint256).max)));
    }

    function test_EmitSignedScalar() public {
        vm.recordLogs();
        events.emitSignedScalar();
        Vm.Log[] memory logs = vm.getRecordedLogs();
        require(logs.length == 1);
        require(logs[0].topics.length == 3);
        require(logs[0].topics[1] == bytes32(type(uint256).max));
        require(logs[0].topics[2] == bytes32(type(uint256).max));
        require(logs[0].data.length == 32);
        require(abi.decode(logs[0].data, (int256)) == -1);
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

    function test_EmitDefaultIndexedAggregate() public {
        vm.recordLogs();
        events.emitDefaultIndexedAggregate();
        Vm.Log[] memory logs = vm.getRecordedLogs();
        require(logs.length == 1);
        require(logs[0].topics.length == 2);
        require(logs[0].topics[1] == DEFAULT_TRIPLE_TOPIC);
    }

    function test_EmitDefaultIndexedDynamicAggregate() public {
        vm.recordLogs();
        events.emitDefaultIndexedDynamicAggregate();
        Vm.Log[] memory logs = vm.getRecordedLogs();
        require(logs.length == 1);
        require(logs[0].topics.length == 2);
        require(logs[0].topics[1] == DEFAULT_DYNAMIC_TOPIC);
    }

    function test_EmitDirtyIndexedScalar() public {
        vm.recordLogs();
        events.emitDirtyIndexedScalar();
        Vm.Log[] memory logs = vm.getRecordedLogs();
        require(logs.length == 1);
        require(logs[0].topics.length == 2);
        require(logs[0].topics[1] == keccak256(abi.encode(uint256(1))));
    }

    function test_EmitInvalidIndexedEnum() public {
        vm.expectRevert(abi.encodeWithSignature("Panic(uint256)", 0x21));
        events.emitInvalidIndexedEnum();
    }

    function test_EmitTwoWords() public {
        vm.recordLogs();
        events.emitTwoWords(7, type(uint256).max);
        Vm.Log[] memory logs = vm.getRecordedLogs();
        require(logs.length == 1);
        require(logs[0].topics.length == 1);
        require(logs[0].topics[0] == keccak256("TwoWords(uint256,uint256)"));
        require(logs[0].data.length == 64);
        (uint256 first, uint256 second) = abi.decode(logs[0].data, (uint256, uint256));
        require(first == 7 && second == type(uint256).max);
    }

    function test_EmitTwoWordsIndexed() public {
        vm.recordLogs();
        events.emitTwoWordsIndexed(9, address(0x1234), true);
        Vm.Log[] memory logs = vm.getRecordedLogs();
        require(logs.length == 1);
        require(logs[0].topics.length == 2);
        require(logs[0].topics[0] == keccak256("TwoWordsIndexed(uint256,address,bool)"));
        require(logs[0].topics[1] == bytes32(uint256(9)));
        require(logs[0].data.length == 64);
        (address account, bool flag) = abi.decode(logs[0].data, (address, bool));
        require(account == address(0x1234) && flag);
    }

    function test_EmitTwoWordsAnonymous() public {
        vm.recordLogs();
        events.emitTwoWordsAnonymous(address(0x1234), true);
        Vm.Log[] memory logs = vm.getRecordedLogs();
        require(logs.length == 1);
        require(logs[0].topics.length == 0);
        require(logs[0].data.length == 64);
        (address account, bool flag) = abi.decode(logs[0].data, (address, bool));
        require(account == address(0x1234) && flag);
    }

    function test_EmitTwoWordsSwapped() public {
        vm.recordLogs();
        events.emitTwoWordsSwapped();
        Vm.Log[] memory logs = vm.getRecordedLogs();
        require(logs.length == 1);
        (uint256 first, uint256 second) = abi.decode(logs[0].data, (uint256, uint256));
        require(first == 2 && second == 1);
    }

    function test_EmitThreeWords() public {
        vm.recordLogs();
        events.emitThreeWords(1, type(uint256).max, 3);
        Vm.Log[] memory logs = vm.getRecordedLogs();
        require(logs.length == 1);
        require(logs[0].topics.length == 1);
        require(logs[0].topics[0] == keccak256("ThreeWords(uint256,uint256,uint256)"));
        require(logs[0].data.length == 96);
        (uint256 first, uint256 second, uint256 third) =
            abi.decode(logs[0].data, (uint256, uint256, uint256));
        require(first == 1 && second == type(uint256).max && third == 3);
    }

    function test_EmitDynamicData() public {
        bytes memory data = hex"0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122";
        vm.recordLogs();
        events.emitDynamicData(7, data, 9);
        Vm.Log[] memory logs = vm.getRecordedLogs();
        require(logs.length == 1);
        require(logs[0].topics.length == 2);
        require(logs[0].topics[1] == bytes32(uint256(7)));
        (bytes memory loggedData, string memory text, uint256 count) =
            abi.decode(logs[0].data, (bytes, string, uint256));
        require(keccak256(loggedData) == keccak256(data));
        require(
            keccak256(bytes(text))
                == keccak256("a literal string that is longer than a single word")
        );
        require(count == 9);
    }

    function test_EmitDynamicDataThenAllocate() public {
        bytes memory data = hex"010203";
        bytes memory result = events.emitDynamicDataThenAllocate(data);
        require(keccak256(result) == keccak256(hex"010203aa"));
    }
}
