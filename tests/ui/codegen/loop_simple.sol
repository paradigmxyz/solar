//@ignore-host: windows
//@compile-flags: -Zcodegen --emit=mir

contract LoopSimple {
    function sum_to(uint256 n) public pure returns (uint256) {
        uint256 total = 0;
        for (uint256 i = 0; i < n; i++) {
            total = total + i;
        }
        return total;
    }
}
