//@ check-pass
//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime

// The external wrapper of a public library function must decode a
// storage-reference parameter as its slot (one calldata word), not
// field-expand it like a memory struct. The body below reads and writes the
// struct's fields through the reference, so a wrapper that mis-decoded the
// parameter would address garbage. The one-word decode also keeps the
// wrapper's calldata-size check consistent with the linked-call encoding
// (`abi_head_size` = 32 for storage references).

library DataTypes {
    struct Reserve {
        uint128 a;
        uint128 b;
        uint256 total;
    }
}

library L {
    function settle(DataTypes.Reserve storage r, uint256 amount) public returns (uint256) {
        r.total += amount;
        return r.total + uint256(r.a) + uint256(r.b);
    }
}
