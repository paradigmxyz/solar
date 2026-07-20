//@compile-flags: -Zcodegen -Zdump=mir

contract YulCallErrors {
    function unknownCall() public pure returns (uint256 result) {
        assembly {
            result := unknown_yul_call() //~ ERROR: unresolved symbol `unknown_yul_call`
        }
    }
}
