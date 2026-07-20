//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=evm-ir

contract WhileEmptyBody {
    function f(uint256 x) public pure {
        while (x > 0) {}
    }
}
