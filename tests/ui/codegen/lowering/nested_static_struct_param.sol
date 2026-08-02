//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

struct Inner {
    uint256 a;
    uint256 b;
}

struct Outer {
    uint256 x;
    Inner inner;
    uint256 y;
}

contract NestedStaticStructParam {
    // A static struct with a nested static struct is fully inlined in the ABI
    // head: `x`, `inner.a`, `inner.b`, `y` occupy four consecutive head words.
    // The typed MIR keeps the nested layout until the ABI phase rebuilds it.
    // CHECK-LABEL: fn @take{{[( ]}}
    // CHECK: abi_params=[tuple<u256, tuple<u256, u256>, u256>]
    // CHECK: memory_object_field_addr memorystruct<3>, arg0, 1
    // CHECK: memory_object_field_addr memorystruct<2>, v1, 1
    function take(Outer calldata o) external pure returns (uint256, uint256) {
        return (o.inner.b, o.y);
    }
}
