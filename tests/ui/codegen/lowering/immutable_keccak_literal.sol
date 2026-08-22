//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract ImmutableKeccakLiteral {
    bytes32 immutable value = keccak256("solar");

    function get() external view returns (bytes32) {
        return value;
    }

    // CHECK-LABEL: fn @get{{[( ]}}
    // CHECK: loadimmutable value
    // CHECK-LABEL: fn @constructor{{[( ]}}
    // CHECK: storeimmutable value, 0x31e1c5bf9da84811147b2cab01421da1659d9baff618fb99b976b2c0901cba01
}
