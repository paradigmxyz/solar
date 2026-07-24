//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir

contract C {
    uint256 public immutable start;
    uint256 public immutable duration;

    constructor(uint256 s) {
        start = s;
        // Constructor-context reads observe the current assigned value: runtime
        // placeholders are only patched in the returned copy of the code.
        duration = start + 1;
    }

    function end() public view returns (uint256) {
        return start + duration;
    }
}
