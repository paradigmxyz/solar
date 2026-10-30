//@ compile-flags: -O gas -Zdump=evm-ir-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// The hex loop of Solady's `LibString.toHexStringNoPrefix`. Its header lays the carried words out
// with the input pointer on top, so the output pointer steps by two before the last `mstore8` and
// swaps into its old slot: the body takes five `dup`s and `swap`s, not six.
// CHECK: push 0x30313233343536373839616263646566
// CHECK: bb{{[0-9]+}} [loop]:
// CHECK-NEXT: push 1
// CHECK-NEXT: add
// CHECK-NEXT: dup 1
// CHECK-NEXT: mload
// CHECK-NEXT: push 15
// CHECK-NEXT: and
// CHECK-NEXT: mload
// CHECK-NEXT: dup 4
// CHECK-NEXT: push 1
// CHECK-NEXT: add
// CHECK-NEXT: mstore8
// CHECK-NEXT: dup 1
// CHECK-NEXT: mload
// CHECK-NEXT: push 4
// CHECK-NEXT: shr
// CHECK-NEXT: push 15
// CHECK-NEXT: and
// CHECK-NEXT: mload
// CHECK-NEXT: dup 4
// CHECK-NEXT: push 2
// CHECK-NEXT: add
// CHECK-NEXT: swap 4
// CHECK-NEXT: mstore8
// CHECK-NEXT: dup 5
// CHECK-NEXT: dup 2
// CHECK-NEXT: sub
// CHECK-NEXT: push bb{{[0-9]+}}
// CHECK-NEXT: jumpi
contract HexLoopHeaderLayout {
    function hexNoPrefix(bytes memory raw) external pure returns (string memory) {
        return toHexStringNoPrefix(raw);
    }

    function toHexStringNoPrefix(bytes memory raw) internal pure returns (string memory result) {
        assembly {
            let n := mload(raw)
            result := add(mload(0x40), 2)
            mstore(result, add(n, n))
            mstore(0x0f, 0x30313233343536373839616263646566)
            let o := add(result, 0x20)
            let end := add(raw, n)
            for {} iszero(eq(raw, end)) {} {
                raw := add(raw, 1)
                mstore8(add(o, 1), mload(and(mload(raw), 15)))
                mstore8(o, mload(and(shr(4, mload(raw)), 15)))
                o := add(o, 2)
            }
            mstore(o, 0)
            mstore(0x40, add(o, 0x20))
        }
    }
}
