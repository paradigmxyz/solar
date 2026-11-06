//@ codegen-matrix: standard
//@ run-call: read [(7, 0x0102)] => 7, 0x0102
//@ run-call: 0x171e22c600000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000001ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff5c => 0x000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: 0x171e22c6000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040

// A negative element offset accepted by solc's signed tail bound may wrap the
// struct pointer past the end of calldata. Decoding stays lazy: the whole head
// is not range-checked eagerly, so the selected fields read zero-filled
// calldata and resolve to zero and an empty `bytes`, exactly like solc.
pragma abicoder v2;

contract AbiCalldataLazyStructWrapped {
    struct S {
        uint256 a;
        bytes b;
    }

    function read(S[] calldata items) external pure returns (uint256, bytes memory) {
        return (items[0].a, items[0].b);
    }
}
