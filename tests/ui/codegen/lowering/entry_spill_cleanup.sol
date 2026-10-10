//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@[ir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: toHexStringNoPrefix 0x => ""
//@ run-call: toHexStringNoPrefix 0x00abff => "00abff"
//@ run-call: prefixed 0x => "0x"
//@ run-call: prefixed 0x00abff => "0x00abff"

// The shared helper keeps its free-memory pointer, result, and loop end on the
// stack across its loop: the loop test copies the end and the cursor without a load.
// Reduced from Solady v0.1.26 LibString.toHexStringNoPrefix (MIT).
//
// CHECK: [[RETURN:bb[0-9]+]] [continuation]:
// CHECK-NEXT: push {{bb[0-9]+}}
// CHECK-NEXT: jump [[HELPER:bb[0-9]+]]
// CHECK-NEXT: [[HELPER]]:
// CHECK: jump [[HEADER:bb[0-9]+]]
// CHECK: [[HEADER]] [loop]:
// CHECK-NEXT: dup 5
// CHECK-NEXT: dup 3
// CHECK-NEXT: sub
// CHECK-NEXT: push {{bb[0-9]+}}
// CHECK-NEXT: jumpi
contract EntrySpillCleanup {
    function prefixed(bytes memory raw) external pure returns (string memory result) {
        result = toHexStringNoPrefix(raw);
        assembly {
            let n := add(mload(result), 2)
            mstore(result, 0x3078)
            result := sub(result, 2)
            mstore(result, n)
        }
    }

    function toHexStringNoPrefix(bytes memory raw) public pure returns (string memory result) {
        assembly {
            let n := mload(raw)
            result := add(mload(0x40), 2) // Skip 2 bytes for the optional prefix.
            mstore(result, add(n, n)) // Store the length of the output.

            mstore(0x0f, 0x30313233343536373839616263646566) // Store the "0123456789abcdef" lookup.
            let o := add(result, 0x20)
            let end := add(raw, n)
            for {} iszero(eq(raw, end)) {} {
                raw := add(raw, 1)
                mstore8(add(o, 1), mload(and(mload(raw), 15)))
                mstore8(o, mload(and(shr(4, mload(raw)), 15)))
                o := add(o, 2)
            }
            mstore(o, 0) // Zeroize the slot after the string.
            mstore(0x40, add(o, 0x20)) // Allocate memory.
        }
    }
}
