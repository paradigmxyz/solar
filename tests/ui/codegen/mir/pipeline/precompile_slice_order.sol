//@ revisions: checked unchecked
//@ compile-flags: -Ogas --emit=bin-runtime
//@ compile-flags: -Zmir-pipeline=lower-checks,lower-arithmetic,lower-abi,lower-abi-encode,lower-structs,lower-frame-slots,lower-mapping-slots,lower-memory-objects,lower-slices,lower-builtins,lower-abi,lower-dispatch,lower-frame-slots,lower-memory-objects,lower-alloc,lower-memory-zero,lower-integers,lower-evm-shaped
//@[checked] compile-flags: -Zvalidate-ir=true
//@[unchecked] compile-flags: -Zvalidate-ir=false
// A reordered conversion pipeline must fail before a headerless slice can become a bytes object.
//~? ERROR: `lower-slices` requires lowered builtin slice operands; run `lower-builtins` first

contract PrecompileSliceOrder {
    function sha(bytes calldata input) external pure returns (bytes32) {
        return sha256(input);
    }

    function ripemd(bytes calldata input) external pure returns (bytes20) {
        return ripemd160(input);
    }
}
