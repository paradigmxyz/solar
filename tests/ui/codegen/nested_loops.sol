//@ignore-host: windows
//@compile-flags: -Zcodegen --emit=mir

contract NestedLoops {
    function sum_grid(uint256 n, uint256 m) public pure returns (uint256) {
        uint256 total = 0;
        for (uint256 i = 0; i < n; i++) {
            for (uint256 j = 0; j < m; j++) {
                total = total + i * j;
            }
        }
        return total;
    }

    function find_first(uint256 n, uint256 target) public pure returns (uint256) {
        for (uint256 i = 0; i < n; i++) {
            for (uint256 j = 0; j < n; j++) {
                if (i + j == target) {
                    return i;
                }
            }
        }
        return n;
    }
}
