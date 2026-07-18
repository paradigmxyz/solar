//@compile-flags: -Zcodegen --emit=mir

contract YulUnsupportedBuiltins {
    function unsupportedBuiltin() public pure returns (uint256 result) {
        assembly {
            result := clz(1) //~ ERROR: unsupported Yul builtin `clz`
        }
    }
}
