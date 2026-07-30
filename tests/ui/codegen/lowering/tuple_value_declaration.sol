//@ run-call: pair(int256) -7 => 999999999999999993, -7
//@ run-call: triple(uint256) 11 => 11, 12, 13
//@ run-call: arrays(uint256,uint256) 17, 29 => 17, 29
//@ run-call: discarded() => 13, 3
//@ run-call: parenthesizedCall() => 7

contract TupleValueDeclaration {
    uint256 private count;

    function pair(int256 x) external pure returns (int256, int256) {
        (int256 wad, int256 p) = (int256(1e18), x);
        return (wad + p, p);
    }

    function triple(uint256 x) external pure returns (uint256, uint256, uint256) {
        (uint256 n, uint256 o, uint256 e) = (x, x + 1, x + 2);
        return (n, o, e);
    }

    function arrays(uint256 x, uint256 y) external pure returns (uint256, uint256) {
        uint256[] memory keys = new uint256[](1);
        uint256[] memory values = new uint256[](1);
        keys[0] = x;
        values[0] = y;

        (uint256[] memory originalKeys, uint256[] memory originalValues) = (keys, values);
        return (originalKeys[0], originalValues[0]);
    }

    function discarded() external returns (uint256, uint256) {
        (uint256 first,, uint256 third) = (bump(1), bump(2), bump(3));
        return (first * 10 + third, count);
    }

    function bump(uint256 value) internal returns (uint256) {
        count++;
        return value;
    }

    function parenthesizedCall() external pure returns (uint256) {
        uint256 first;
        (first,) = (pairValues());
        return first;
    }

    function pairValues() internal pure returns (uint256, uint256) {
        return (7, 9);
    }
}
