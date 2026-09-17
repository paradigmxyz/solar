//@ codegen-matrix: standard
//@ compile-flags: -Zmir-pipeline=lower-abi,lower-abi-encode,lower-aggregates,lower-slices,lower-dispatch,lower-memory-objects,lower-alloc,lower-evm-shaped

//~? ERROR: immutable assignment instruction `storeimmutable` survives the `lowered` phase boundary
contract StoreImmutableRequiresLowering {
    uint256 immutable value;

    constructor(uint256 value_) {
        value = value_;
    }
}
