//@ compile-flags: -Ogas
//@ run-call: first 7, 64 => 72
//@ run-call: second 9, 32 => 42

contract ResidentArgRecursion {
    function first(uint256 value, uint256 depth) external pure returns (uint256) {
        return enter(value, depth);
    }

    function second(uint256 value, uint256 depth) external pure returns (uint256) {
        return enter(value, depth);
    }

    function enter(uint256 value, uint256 depth) internal pure returns (uint256) {
        return value + recurse(depth);
    }

    function recurse(uint256 depth) internal pure returns (uint256) {
        if (depth == 0) return 1;
        return recurse(depth - 1) + 1;
    }
}
