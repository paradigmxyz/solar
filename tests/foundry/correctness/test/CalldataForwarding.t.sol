// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface BytesSink {
    function consume(bytes calldata data) external;
}

contract RecordingBytesSink is BytesSink {
    bytes32 public hash;
    uint256 public length;

    function consume(bytes calldata data) external {
        hash = keccak256(data);
        length = data.length;
    }
}

contract CalldataForwarder {
    function forward(bytes calldata data, BytesSink sink) external {
        sink.consume(data);
    }
}

interface NestedSink {
    function consume(bytes[] calldata data) external;
}

struct NestedItem {
    uint256 id;
    bytes payload;
}

interface NestedStructSink {
    function consume(NestedItem[] calldata data) external;
}

contract RawNestedSink {
    bytes32 public hash;
    uint256 public length;

    fallback() external {
        assembly {
            calldatacopy(0, 0, calldatasize())
            sstore(hash.slot, keccak256(0, calldatasize()))
            sstore(length.slot, calldatasize())
        }
    }
}

contract NestedCalldataForwarder {
    function forward(bytes[] calldata data, NestedSink sink) external {
        sink.consume(data);
    }

    function forwardStructs(NestedItem[] calldata data, NestedStructSink sink) external {
        sink.consume(data);
    }
}

contract CalldataForwardingTest {
    function testForwardCalldataBytes() public {
        RecordingBytesSink sink = new RecordingBytesSink();
        CalldataForwarder forwarder = new CalldataForwarder();
        bytes memory data = hex"112233445566778899aabbccddeeff";

        forwarder.forward(data, sink);

        assert(sink.hash() == keccak256(data));
        assert(sink.length() == data.length);
    }

    function testForwardNestedCalldataBytes() public {
        RawNestedSink sink = new RawNestedSink();
        NestedCalldataForwarder forwarder = new NestedCalldataForwarder();
        bytes[] memory data = new bytes[](3);
        data[0] = hex"112233";
        data[1] = hex"";
        data[2] = hex"aabbccddeeff";

        NestedSink(address(sink)).consume(data);
        bytes32 directHash = sink.hash();
        uint256 directLength = sink.length();

        forwarder.forward(data, NestedSink(address(sink)));

        assert(sink.hash() == directHash);
        assert(sink.length() == directLength);
    }

    function testForwardNestedCalldataStructs() public {
        RawNestedSink sink = new RawNestedSink();
        NestedCalldataForwarder forwarder = new NestedCalldataForwarder();
        NestedItem[] memory data = new NestedItem[](2);
        data[0] = NestedItem({id: 7, payload: hex"112233"});
        data[1] = NestedItem({id: 42, payload: hex"aabbccddeeff"});

        NestedStructSink(address(sink)).consume(data);
        bytes32 directHash = sink.hash();
        uint256 directLength = sink.length();

        forwarder.forwardStructs(data, NestedStructSink(address(sink)));

        assert(sink.hash() == directHash);
        assert(sink.length() == directLength);
    }
}
