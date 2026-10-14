//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: length => 65537
//@ run-call: last => 0x00
//@ run-call-fail: discardHuge() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041

// A `bytes` constant too large to fold is built from its initializer at every use.
contract C {
    bytes constant B = new bytes(65537);
    bytes constant HUGE = new bytes(2**64);

    // CHECK-LABEL: fn @length(
    // CHECK: [[BYTES:v[0-9]+]] = alloc zeroed
    // CHECK: mstore [[BYTES]], 0x10001
    function length() public pure returns (uint256) {
        B;
        return B.length;
    }

    function last() public pure returns (bytes1) {
        return B[65536];
    }

    // A discarded read still runs the initializer, which reverts.
    function discardHuge() public pure {
        HUGE;
    }
}
