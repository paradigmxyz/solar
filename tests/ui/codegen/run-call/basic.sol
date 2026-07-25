//@ run-call: add 2 => 42
//@ run-call: negate(bool) true => false
//@ run-call: pair 41, true => 42, false
//@ run-call: sum(uint256[]) [1, 2, 3] => 6
//@ run-call: increment => 41
//@ run-call: increment => 41
//@ run-call: testInline()
//@ run-call: trimLen(bytes) 0x010203 => 3
//@ run-call: trimLen(bytes) 0x010203040506 => 2
//@ run-call: repeatedSourceJoin(bool,uint256,uint256) true, 7, 9 => 7, 7
//@ run-call: repeatedSourceJoin(bool,uint256,uint256) false, 7, 9 => 9, 7
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

    function trimLen(bytes calldata data) external pure returns (uint256) {
        return trim(data).length;
    }

    function trim(bytes calldata data) internal pure returns (bytes calldata) {
        if (data.length > 4) return data[4:];
        return data;
    }

    function repeatedSourceJoin(
        bool first,
        uint256 a,
        uint256 b
    ) external pure returns (uint256 x, uint256 y) {
        if (first) {
            x = a;
            y = a;
        } else {
            x = b;
            y = a;
        }
    }
}
