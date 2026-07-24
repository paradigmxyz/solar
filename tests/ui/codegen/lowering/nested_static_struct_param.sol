//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck: --check-prefix=NESTED

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
    // The nested struct rebuilds into its own allocation stored as a pointer,
    // and the field after it slots at the correct head word.
    // The nested struct is a separate allocation, and its second field reads at
    // a +32 offset rather than the enclosing struct's base.
    // NESTED-LABEL: fn @take{{[( ]}}
    // NESTED: alloc memorystruct<3>
    // NESTED: alloc raw, exact, uninitialized, infallible, 64
    // NESTED: mstore {{.*}}, arg3
    function take(Outer calldata o) external pure returns (uint256, uint256) {
        return (o.inner.b, o.y);
    }
}
