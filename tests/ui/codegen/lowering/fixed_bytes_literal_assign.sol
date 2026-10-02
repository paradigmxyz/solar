//@ codegen-matrix: standard opt
//@ run-call: patch(bool) true => 0x2d
//@ run-call: patch(bool) false => 0x41
//@ run-call: store() => 0x2d
//@[opt] compile-flags: -Ogas -Zdump=mir
//@[opt] filecheck: --check-prefix=MIR

// A literal assigned to a fixed-bytes place is that word. Lowering it as a
// memory literal and reading the first word back leaves the allocation behind:
// it writes the free-memory pointer, so no later pass can remove it. Only the
// buffer being written to is allocated here.
contract FixedBytesLiteralAssign {
    bytes1 slot;

    // MIR-LABEL: fn @patch
    // MIR: [[BASE:v[0-9]+]] = mload 64
    // MIR: [[END:v[0-9]+]] = add [[BASE]], 64
    // MIR: mstore 64, [[END]]
    // MIR-NOT: mstore 64,
    // MIR: mstore8 {{v[0-9]+}}, 45
    function patch(bool safe) external pure returns (bytes1) {
        bytes memory table = "AB";
        if (safe) table[0] = "-";
        return table[0];
    }

    // MIR-LABEL: fn @store
    // MIR-NOT: = alloc
    // MIR: sstore
    function store() external returns (bytes1) {
        slot = "-";
        return slot;
    }
}
