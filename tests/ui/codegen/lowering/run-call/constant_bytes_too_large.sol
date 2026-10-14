//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: length => 65537
//@ run-call: last => 0x00

// A `bytes` constant too large to fold is built from its initializer at every use.
contract C {
    bytes constant B = new bytes(65537);

    // CHECK-LABEL: fn @length(
    // CHECK: alloc memorybytes, exact, zeroed
    // CHECK: set_memory_object_len memorybytes, {{v[0-9]+}}, 0x10001
    function length() public pure returns (uint256) {
        B;
        return B.length;
    }

    function last() public pure returns (bytes1) {
        return B[65536];
    }
}
