//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir

contract YulLowLevelBuiltins {
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
