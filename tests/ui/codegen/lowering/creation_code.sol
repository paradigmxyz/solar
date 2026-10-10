//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract CreationCodeChild {
    constructor(uint256 value) {}
}

contract CreationCodeFactory {
    // CHECK-LABEL: fn @creationCode{{[( ]}}
    // CHECK: [[CODE:v[0-9]+]] = alloc memorybytes
    // CHECK: [[CODE_HEAD:v[0-9]+]] = ptrtoint memptr [[CODE]] to i256
    // CHECK: mstore [[CODE_HEAD]], {{v[0-9]+}}
    function creationCode() external pure returns (bytes memory) {
        return type(CreationCodeChild).creationCode;
    }
}
