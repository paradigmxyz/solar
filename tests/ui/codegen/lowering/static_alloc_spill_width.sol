//@ compile-flags: -O size -Zdump=disasm-runtime
//@ filecheck:
//@ run-call: f => 5, 6, 0

// The values the copy loop needs stay on the stack, so no spill slot competes
// with `other` for a short static address: it takes 0x80 with a `PUSH1`.
contract C {
    uint256[3][] private triples;

    function f() external returns (uint256, uint256, uint256) {
        uint256[2] memory pair = [uint256(5), 6];
        uint256[2] memory other = [uint256(9), 9];
        triples.push(pair);
        // A scratch write that may alias `other` keeps its stores from being forwarded or
        // dropped as unread, so it still needs a static address.
        assembly ("memory-safe") {
            calldatacopy(0, 0, 4)
        }
        return (triples[0][0], triples[0][1], triples[0][2] + other[0] + other[1] - 18);
    }
}

// CHECK: PUSH1 0x06
// CHECK: PUSH1 0x80
// CHECK-NEXT: PUSH1 0x09
// CHECK-NEXT: DUP2
// CHECK-NEXT: MSTORE
