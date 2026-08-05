//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract AddressCode {
    // CHECK-LABEL: fn @codeLength{{[( ]}}
    // CHECK: extcodesize arg0
    function codeLength(address account) external view returns (uint256) {
        return account.code.length;
    }

    // CHECK-LABEL: fn @codeHash{{[( ]}}
    // CHECK: extcodehash arg0
    function codeHash(address account) external view returns (bytes32) {
        return account.codehash;
    }

    // CHECK-LABEL: fn @code{{[( ]}}
    // CHECK: extcodesize arg0
    // CHECK: extcodecopy arg0
    function code(address account) external view returns (bytes memory) {
        return account.code;
    }

    // CHECK-LABEL: fn @addressFromBytes20{{[( ]}}
    // CHECK: shr 96, arg0
    function addressFromBytes20(bytes20 value) external pure returns (address) {
        return address(value);
    }
}
