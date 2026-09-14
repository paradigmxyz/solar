contract Transfers {
    uint256 public stored;

    constructor() { stored = 7; }

    function checked(uint256 x) external pure returns (uint256) {
        require(x > 0, "positive");
        return x;
    }

    function named() external pure returns (uint256 x) { x = 3; }

    function size(bytes calldata value) external pure returns (uint256) {
        return value.length;
    }

    function sum(uint256[] calldata values) external pure returns (uint256 total) {
        for (uint256 i; i < values.length; ++i) total += values[i];
    }

    function indirect(bool choose) external pure returns (uint256) {
        function() internal pure returns (uint256) f = choose ? one : two;
        return f();
    }

    function one() internal pure returns (uint256) { return 1; }
    function two() internal pure returns (uint256) { return 2; }
}
