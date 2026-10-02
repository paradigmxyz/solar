//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract AddressCode {
    // CHECK-LABEL: fn @codeLength{{[( ]}}
    // CHECK: [[ADDR:v[0-9]+]] = zext i160 arg0 to i256
    // CHECK: extcodesize [[ADDR]]
    function codeLength(address account) external view returns (uint256) {
        return account.code.length;
    }

    // CHECK-LABEL: fn @codeHash{{[( ]}}
    // CHECK: [[HASH_ADDR:v[0-9]+]] = zext i160 arg0 to i256
    // CHECK: extcodehash [[HASH_ADDR]]
    function codeHash(address account) external view returns (bytes32) {
        return account.codehash;
    }

    // CHECK-LABEL: fn @code{{[( ]}}
    // CHECK: [[ADDR:v[0-9]+]] = zext i160 arg0 to i256
    // CHECK: extcodesize [[ADDR]]
    // CHECK: [[COPY_ADDR:v[0-9]+]] = zext i160 arg0 to i256
    // CHECK: extcodecopy [[COPY_ADDR]]
    function code(address account) external view returns (bytes memory) {
        return account.code;
    }

    // CHECK-LABEL: fn @addressFromBytes20{{[( ]}}
    // CHECK: [[CLEAN:v[0-9]+]] = and arg0, 0xffffffffffffffffffffffffffffffffffffffff000000000000000000000000
    // CHECK: shr 96, [[CLEAN]]
    function addressFromBytes20(bytes20 value) external pure returns (address) {
        return address(value);
    }
    // CHECK-LABEL: fn @callerCodeLength{{[( ]}}
    // CHECK: [[CALLER:v[0-9]+]] = caller
    // CHECK: [[WORD:v[0-9]+]] = zext i160 [[CALLER]] to i256
    // CHECK-NOT: trunc
    // CHECK: extcodesize [[WORD]]
    function callerCodeLength() external view returns (uint256) {
        address account = msg.sender;
        assembly { account := account }
        return account.code.length;
    }

}
