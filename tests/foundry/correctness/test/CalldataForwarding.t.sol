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

contract CalldataForwardingTest {
    function testForwardCalldataBytes() public {
        RecordingBytesSink sink = new RecordingBytesSink();
        CalldataForwarder forwarder = new CalldataForwarder();
        bytes memory data = hex"112233445566778899aabbccddeeff";

        forwarder.forward(data, sink);

        assert(sink.hash() == keccak256(data));
        assert(sink.length() == data.length);
    }
}
