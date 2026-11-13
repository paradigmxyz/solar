// The default pipeline on a contract with an internal call, a checked operation, a loop and an
// external call, whose return data size the last pass reads for zero.
contract ValidateCalls {
    uint256 total;

    function sum(uint256 n) external returns (uint256 s) {
        for (uint256 i = 0; i < n; i++) {
            s += twice(i);
        }
        total = s;
    }

    function ping(address target) external returns (bool ok) {
        (ok,) = target.call("");
    }

    function twice(uint256 x) internal pure returns (uint256) {
        return x * 2;
    }
}
