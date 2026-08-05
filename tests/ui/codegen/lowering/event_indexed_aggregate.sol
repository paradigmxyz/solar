//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract EventIndexedAggregate {
    event IndexedArray(uint256[2] indexed values);

    // CHECK-LABEL: fn @emitArray{{[( ]}}
    // CHECK: abi_encode [array<2, word>]
    // CHECK: log2
    function emitArray(uint256[2] memory values) external {
        emit IndexedArray(values);
    }
}
