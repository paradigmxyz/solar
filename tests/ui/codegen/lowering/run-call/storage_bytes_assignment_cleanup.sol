//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none] run-call: cleanup() => true, true, 32
//@[gas] run-call: cleanup() => true, true, 32
//@[size] run-call: cleanup() => true, true, 32
//@[none] run-call: cleanupShort() => false, true, 1
//@[gas] run-call: cleanupShort() => false, true, 1
//@[size] run-call: cleanupShort() => false, true, 1

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
