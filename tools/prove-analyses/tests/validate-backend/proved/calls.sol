// Every block of an internal call, a loop and a storage write lowers faithfully.
contract ValidateBackendCalls {
    uint256 total;

    function run(uint256 a, uint256 n) external returns (uint256 r) {
        r = step(a, n);
        for (uint256 i; i < n; ++i) {
            r ^= i + a;
        }
        total = r;
    }

    function step(uint256 a, uint256 b) internal pure returns (uint256) {
        return (a ^ b) + 1;
    }
}
