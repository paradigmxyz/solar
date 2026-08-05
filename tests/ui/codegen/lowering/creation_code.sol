//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract CreationCodeChild {
    constructor(uint256 value) {}
}

contract CreationCodeFactory {
    // CHECK-LABEL: fn @creationCode{{[( ]}}
    // CHECK: alloc memorybytes
    // CHECK: set_memory_object_len memorybytes
    function creationCode() external pure returns (bytes memory) {
        return type(CreationCodeChild).creationCode;
    }
}
