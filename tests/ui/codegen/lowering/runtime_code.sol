//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract RuntimeCodeTarget {
    function value() external pure returns (uint256) {
        return 7;
    }
}

contract RuntimeCode {
    // CHECK-LABEL: fn @runtime{{[( ]}}
    // CHECK: alloc memorybytes
    // CHECK: set_memory_object_len memorybytes
    function runtime() external pure returns (uint256) {
        return type(RuntimeCodeTarget).runtimeCode.length;
    }
}
