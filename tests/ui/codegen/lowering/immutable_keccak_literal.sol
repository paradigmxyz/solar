//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract ImmutableKeccakLiteral {
    bytes32 immutable value = keccak256("solar");

    function get() external view returns (bytes32) {
        return value;
    }

    // CHECK-LABEL: fn @get{{[( ]}}
    // CHECK: loadimmutable value
    // CHECK-LABEL: fn @constructor{{[( ]}}
    // CHECK: v1 = keccak256_bytes v0
    // CHECK-NEXT: storeimmutable value, v1
}
