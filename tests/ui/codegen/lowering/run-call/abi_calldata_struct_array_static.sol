//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: f [(1, 2), (3, 4)] => 2, 1, 2, 3, 4
// ported-from: test/libsolidity/semanticTests/abicoder/calldataDecoding/array/calldata_array_struct_static_v2.sol

struct StaticPair {
    uint256 a;
    uint256 b;
}

contract AbiCalldataStructArrayStatic {
    function f(StaticPair[] calldata value)
        external
        pure
        returns (uint256 length, uint256 a, uint256 b, uint256 c, uint256 d)
    {
        length = value.length;
        a = value[0].a;
        b = value[0].b;
        c = value[1].a;
        d = value[1].b;
    }
}
