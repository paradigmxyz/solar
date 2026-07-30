//@ run-call: add 2 => 42
//@ run-call: negate(bool) true => false
//@ run-call: pair 41, true => 42, false
//@ run-call: sum(uint256[]) [1, 2, 3] => 6
//@ run-call: increment => 41
//@ run-call: increment => 41
//@ run-call: testInline()
//@ run-call: fullyInitializedNamedStruct => ([0], 0x00)
//@ run-call: reservedSpillFreshness(bool) true => 83
//@ run-call: reservedSpillFreshness(bool) false => 137
//@ run-call: 0x1003e2d20000000000000000000000000000000000000000000000000000000000000002 => 0x000000000000000000000000000000000000000000000000000000000000002a

contract RunCall {
    struct DynamicHolder {
        uint256[] values;
        bytes data;
    }

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

    function fullyInitializedNamedStruct()
        external
        pure
        returns (DynamicHolder memory holder)
    {
        holder.values = new uint256[](1);
        holder.data = new bytes(1);
    }

    function reservedSpillFreshness(bool first) external returns (uint256 out) {
        uint256 seed = base;
        uint256 a = seed;
        uint256 off = seed;
        if (first) {
            (a, off) = pairInternal(seed);
            out = a + off;
        } else {
            base = 99;
            (uint256 b, uint256 c) = pairInternal(off + 7);
            out = b + c + off;
        }
    }

    function pairInternal(uint256 value) internal pure returns (uint256, uint256) {
        return (value + 1, value + 2);
    }
}
