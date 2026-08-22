//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: f [(1, 2), (3, 4)] => 2, 1, 2, 3, 4
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
