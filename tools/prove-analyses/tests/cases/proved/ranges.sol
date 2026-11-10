// Check elimination's loop and branch ranges hold for every input.
contract Ranges {
    function sum(uint256 n) external pure returns (uint256 total) {
        if (n > 100) n = 100;
        for (uint256 i = 0; i < n; i++) {
            total += i;
        }
    }

    function next(uint256 x) external pure returns (uint256) {
        if (x < 10) {
            return x + 1;
        }
        return 0;
    }

    function index(uint256[4] calldata values, uint256 i) external pure returns (uint256) {
        return values[i % 4];
    }
}
