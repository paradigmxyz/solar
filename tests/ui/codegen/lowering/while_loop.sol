//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir

contract WhileLoop {
    function count_down(uint256 n) public pure returns (uint256) {
        uint256 i = n;
        while (i > 0) {
            i = i - 1;
        }
        return i;
    }

    function do_at_least_once(uint256 n) public pure returns (uint256) {
        uint256 i = 0;
        do {
            i = i + 1;
        } while (i < n);
        return i;
    }

    function break_when_found(uint256 n, uint256 target) public pure returns (uint256) {
        uint256 i = 0;
        while (i < n) {
            if (i == target) {
                break;
            }
            i = i + 1;
        }
        return i;
    }
}
