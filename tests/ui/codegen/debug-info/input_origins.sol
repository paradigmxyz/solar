//@ compile-flags: -Zdump=mir -Zmir-pipeline=none -Zdebugify=check
//@ normalize-stderr-test: "debugify: [^\n]*\n" -> ""
//@ normalize-stdout-test: "(?s).+" -> ""
//@ filecheck:

// Default return materialization and modifier-argument conversions originate at their variable
// and modifier declarations. The packed hash fast path must keep the hash call and return spans.
contract InputOrigins {
    modifier useMemory(uint256[] memory values) {
        values[0] = 7;
        _;
    }

    // CHECK-LABEL: fn @withModifier(
    // CHECK: [[COPY:v[0-9]+]] = alloc
    // CHECK: calldatacopy
    function withModifier(uint256[] calldata values) external pure useMemory(values) returns (uint256) {
        return values[0];
    }

    modifier skip(bool enter) {
        if (enter) _;
    }

    // CHECK-LABEL: fn @defaults(
    // CHECK: [[DEFAULT:v[0-9]+]] = alloc 160
    // CHECK: memory_zero [[DEFAULT]], 160
    function defaults(bool enter) external pure skip(enter) returns (uint256[5] memory result) {
        result[2] = 3;
    }

    // CHECK-LABEL: fn @packed(
    // CHECK: [[HASH:v[0-9]+]] = keccak256_packed
    // CHECK: ret [[HASH]]
    function packed(uint256 a, uint256 b) external pure returns (bytes32) {
        bytes memory encoded = abi.encodePacked(a, b);
        return keccak256(encoded);
    }
}
