//@ compile-flags: -Zcodegen --emit=bin-runtime -Zmir-pipeline=lower-abi,lower-abi-encode,lower-aggregates,lower-slices,lower-dispatch,lower-memory-objects,lower-alloc,lower-evm-shaped

contract StoreImmutableRequiresLowering { //~ ERROR: immutable assignments must be lowered before EVM codegen
    uint256 immutable value;

    constructor(uint256 value_) {
        value = value_;
    }
}
