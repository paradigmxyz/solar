//@ run-call: add 2 => 42
//@ run-call: negate(bool) true => false
//@ run-call: pair 41, true => 42, false
//@ run-call: sum(uint256[]) [1, 2, 3] => 6
//@ run-call: increment => 41
//@ run-call: increment => 41
//@ run-call: testInline()
//@ run-call: 0x1003e2d20000000000000000000000000000000000000000000000000000000000000002 => 0x000000000000000000000000000000000000000000000000000000000000002a

contract RunCall {
    uint256 private base;

    constructor() {
        base = 40;
    }

    function add(uint256 value) external view returns (uint256) {
        return base + value;
    }

    function negate(bool value) external pure returns (bool) {
        return !value;
    }

    function pair(uint256 value, bool flag) external pure returns (uint256, bool) {
        return (value + 1, !flag);
    }

    function sum(uint256[] calldata values) external pure returns (uint256 result) {
        for (uint256 i = 0; i < values.length; i++) {
            result += values[i];
        }
    }

    function increment() external returns (uint256) {
        return ++base;
    }

    function testInline() external view {
        assert(base == 40);
    }
}
