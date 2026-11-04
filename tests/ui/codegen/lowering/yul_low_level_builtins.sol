//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract YulLowLevelBuiltins {
    // CHECK-LABEL: fn @yulStringLiteral{{[( ]}}
    // CHECK: mstore 0, 0x736f6c6172000000000000000000000000000000000000000000000000000000
    // CHECK: {{v[0-9]+}} = mload 0
    function yulStringLiteral() public pure returns (uint256 result) {
        assembly {
            mstore(0, "solar")
            result := mload(0)
        }
    }

    // CHECK-LABEL: fn @leadingZeros{{[( ]}}
    // CHECK: {{v[0-9]+}} = clz arg0
    function leadingZeros(uint256 value) public pure returns (uint256 result) {
        assembly {
            result := clz(value)
        }
    }

    // CHECK-LABEL: fn @callCode{{[( ]}}
    // CHECK: [[TARGET:v[0-9]+]] = zext i160 arg0 to i256
    // CHECK: {{v[0-9]+}} = callcode {{v[0-9]+}}, [[TARGET]], 0, 0, 0, 0, 0
    function callCode(address target) public returns (uint256 success) {
        assembly {
            success := callcode(gas(), target, 0, 0, 0, 0, 0)
        }
    }

    // CHECK-LABEL: fn @safeCall{{[( ]}}
    // CHECK: [[FMP:v[0-9]+]] = mload 64
    // CHECK: [[TOKEN:v[0-9]+]] = zext i160 arg0 to i256
    // CHECK: {{v[0-9]+}} = call {{v[0-9]+}}, [[TOKEN]], 0, 0, 4, 0, 32
    // CHECK: [[CODE_TOKEN:v[0-9]+]] = zext i160 arg0 to i256
    // CHECK: extcodesize [[CODE_TOKEN]]
    // CHECK: mstore 64, [[FMP]]
    // CHECK: returndatacopy [[FMP]], 0,
    // CHECK: revert [[FMP]],
    function safeCall(address token, bytes4 selector) public returns (bool success) {
        assembly {
            let fmp := mload(0x40)
            mstore(0x00, selector)
            success := call(gas(), token, 0, 0x00, 0x04, 0x00, 0x20)
            if iszero(and(success, eq(mload(0x00), 1))) {
                returndatacopy(fmp, 0x00, returndatasize())
                revert(fmp, returndatasize())
            }
            success := and(success, gt(extcodesize(token), 0))
            mstore(0x40, fmp)
        }
    }
}
