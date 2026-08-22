//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none] run-call: add 2 => 42
//@[gas] run-call: add 2 => 42
//@[size] run-call: add 2 => 42
//@[none] run-call: negate(bool) true => false
//@[gas] run-call: negate(bool) true => false
//@[size] run-call: negate(bool) true => false
//@[none] run-call: pair 41, true => 42, false
//@[gas] run-call: pair 41, true => 42, false
//@[size] run-call: pair 41, true => 42, false
//@[none] run-call: sum(uint256[]) [1, 2, 3] => 6
//@[gas] run-call: sum(uint256[]) [1, 2, 3] => 6
//@[size] run-call: sum(uint256[]) [1, 2, 3] => 6
//@[none] run-call: 0x0194db8e0000000000000000000000000000000000000000000000000000000000000000 => 0x0000000000000000000000000000000000000000000000000000000000000000
//@[gas] run-call: 0x0194db8e0000000000000000000000000000000000000000000000000000000000000000 => 0x0000000000000000000000000000000000000000000000000000000000000000
//@[size] run-call: 0x0194db8e0000000000000000000000000000000000000000000000000000000000000000 => 0x0000000000000000000000000000000000000000000000000000000000000000
//@[none] run-call-fail: 0x0194db8e0000000000000000000000000000000000000000000000000000000000000020
//@[gas] run-call-fail: 0x0194db8e0000000000000000000000000000000000000000000000000000000000000020
//@[size] run-call-fail: 0x0194db8e0000000000000000000000000000000000000000000000000000000000000020
//@[none] run-call-fail: 0x0194db8e0000000000000000000000000000000000000000000000000000000000000020ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@[gas] run-call-fail: 0x0194db8e0000000000000000000000000000000000000000000000000000000000000020ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@[size] run-call-fail: 0x0194db8e0000000000000000000000000000000000000000000000000000000000000020ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@[none] run-call: increment => 41
//@[gas] run-call: increment => 41
//@[size] run-call: increment => 41
//@[none] run-call: testInline()
//@[gas] run-call: testInline()
//@[size] run-call: testInline()
//@[none] run-call: fullyInitializedNamedStruct => ([0], 0x00)
//@[gas] run-call: fullyInitializedNamedStruct => ([0], 0x00)
//@[size] run-call: fullyInitializedNamedStruct => ([0], 0x00)
//@[none] run-call: reservedSpillFreshness(bool) true => 83
//@[gas] run-call: reservedSpillFreshness(bool) true => 83
//@[size] run-call: reservedSpillFreshness(bool) true => 83
//@[none] run-call: reservedSpillFreshness(bool) false => 137
//@[gas] run-call: reservedSpillFreshness(bool) false => 137
//@[size] run-call: reservedSpillFreshness(bool) false => 137
//@[none] run-call: 0x1003e2d20000000000000000000000000000000000000000000000000000000000000002 => 0x000000000000000000000000000000000000000000000000000000000000002a
//@[gas] run-call: 0x1003e2d20000000000000000000000000000000000000000000000000000000000000002 => 0x000000000000000000000000000000000000000000000000000000000000002a
//@[size] run-call: 0x1003e2d20000000000000000000000000000000000000000000000000000000000000002 => 0x000000000000000000000000000000000000000000000000000000000000002a

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
