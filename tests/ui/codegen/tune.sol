//@ revisions: none interpreter
//@[none] compile-flags: -O gas -Ztune=none -Zdump=evm-ir
//@[none] filecheck: --check-prefixes=CHECK,NONE
//@[interpreter] compile-flags: -O gas -Ztune=interpreter -Zdump=evm-ir
//@[interpreter] filecheck: --check-prefixes=CHECK,INTERPRETER
//@ normalize-stdout-test: "(?s).+" -> ""
//@ run-call: run => 2

contract DynamicSink {
    uint256 public calls;

    function record(address, string calldata) external {
        calls++;
    }
}

// Sharing a helper costs a call and a return at each site, which an interpreter dispatches as
// extra instructions, so under `-Ztune=interpreter` the constructor keeps no call and return.
// CHECK-LABEL: @module DynamicCaller_deployment
// NONE: jump bb{{[0-9]+$}}
// NONE: jump{{$}}
// INTERPRETER-NOT: jump bb{{[0-9]+$}}
// INTERPRETER-NOT: jump{{$}}
// CHECK-LABEL: @module ConstructorSharedAbiEncode_deployment
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
