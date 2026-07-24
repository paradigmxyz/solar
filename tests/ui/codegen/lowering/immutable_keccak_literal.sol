//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract ImmutableKeccakLiteral {
    // CHECK-LABEL: fn @constructor
    // CHECK: mstore 0x2000, 0x31e1c5bf9da84811147b2cab01421da1659d9baff618fb99b976b2c0901cba01
    bytes32 immutable value = keccak256("solar");

    // CHECK-LABEL: fn @get
    // CHECK: loadimmutable 0
    function get() external view returns (bytes32) {
        return value;
    }
}
