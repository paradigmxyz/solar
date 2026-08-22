//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: cleanup() => true, true, 32
//@ run-call: cleanupShort() => false, true, 1

contract StorageBytesAssignmentCleanup {
    bytes private data;

    function cleanup() external returns (bool firstNonzero, bool staleZero, uint256 length) {
        data = hex"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        data = hex"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        assembly {
            mstore(0, 0)
            let base := keccak256(0, 0x20)
            firstNonzero := iszero(iszero(sload(base)))
            staleZero := iszero(sload(add(base, 1)))
        }
        length = data.length;
    }

    function cleanupShort() external returns (bool firstNonzero, bool staleZero, uint256 length) {
        data = hex"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        data = hex"bb";
        assembly {
            mstore(0, 0)
            let base := keccak256(0, 0x20)
            firstNonzero := iszero(iszero(sload(base)))
            staleZero := iszero(sload(add(base, 1)))
        }
        length = data.length;
    }
}
