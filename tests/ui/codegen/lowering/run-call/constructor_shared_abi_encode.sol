//@ codegen-matrix: standard
//@ run-call: run => 2

contract DynamicSink {
    uint256 public calls;

    function record(address, string calldata) external {
        calls++;
    }
}

contract DynamicCaller {
    constructor(DynamicSink sink) {
        recordFirst(sink);
        recordSecond(sink);
    }

    function recordFirst(DynamicSink sink) internal {
        sink.record(address(1), "first");
    }

    function recordSecond(DynamicSink sink) internal {
        sink.record(address(2), "second");
    }
}

contract ConstructorSharedAbiEncode {
    function run() external returns (uint256) {
        DynamicSink sink = new DynamicSink();
        new DynamicCaller(sink);
        return sink.calls();
    }
}
