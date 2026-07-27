//@compile-flags: -Zcodegen -O none -Zdump=mir

contract YulUnsupportedBuiltins {
    function unsupportedBuiltins() public returns (uint256 result) {
        assembly {
            result := extcall(0, 0, 0, 0) //~ ERROR: unsupported Yul builtin `extcall`
            result := extdelegatecall(0, 0, 0) //~ ERROR: unsupported Yul builtin `extdelegatecall`
            result := extstaticcall(0, 0, 0) //~ ERROR: unsupported Yul builtin `extstaticcall`
        }
    }
}
