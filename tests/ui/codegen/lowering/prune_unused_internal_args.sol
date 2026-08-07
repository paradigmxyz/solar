//@ run-call: run() => 11

contract PruneUnusedInternalArgs {
    uint256 public count;

    function run() external returns (uint256) {
        return helper(5, bump()) + count;
    }

    function helper(uint256 value, uint256) internal pure returns (uint256) {
        return value * 2;
    }

    function bump() internal returns (uint256) {
        return ++count;
    }
}
