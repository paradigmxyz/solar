//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract CreationCodeChild {
    constructor(uint256 value) {}
}

contract CreationCodeFactory {
    // CHECK-LABEL: fn @creationCode{{[( ]}}
    // CHECK: [[CODE:v[0-9]+]] = alloc
    // CHECK: store [[CODE]], {{v[0-9]+}}
    function creationCode() external pure returns (bytes memory) {
        return type(CreationCodeChild).creationCode;
    }
}
