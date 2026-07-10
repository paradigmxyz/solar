//@ignore-host: windows
//@compile-flags: -Zcodegen --emit=bin

contract WhileEmptyBody {
    function f(uint256 x) public pure {
        while (x > 0) {}
    }
}
