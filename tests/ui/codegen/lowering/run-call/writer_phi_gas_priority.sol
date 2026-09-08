//@ codegen-matrix: standard physical
//@[physical] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[physical] filecheck:
//@ run-call: copy [[([1, 2], [3, 7]), ([11, 13], [17, 19])]] => 35

// Reduced from storage_nested_struct_calldata.sol. Nested calldata decoding
// keeps a loop counter resident across a writer whose original bank has eleven
// homes. Retiring one home saves gas even when the local bitmap is smaller.
contract WriterPhiGasPriority {
    struct Entry {
        uint8[] values;
        uint8[2] pair;
    }

    Entry[][] private entries;

    // The first protected inner-array allocation keeps its twelve-home bitmap.
    // The loop counter stays on the physical stack; its loop test duplicates it
    // instead of reading a restored home after the later bytes-only veto.
    // CHECK-LABEL: @module WriterPhiGasPriority_runtime
    // CHECK: push 0x5ddf
    // CHECK: swap 5
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: swap 4
    // CHECK-NEXT: mstore
    // CHECK-NEXT: mstore
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 0
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: push [[CURSOR_HOME:[0-9]+]]
    // CHECK-NEXT: mstore
    // CHECK-NEXT: jump [[LOOP:bb[0-9]+]]
    // CHECK-NEXT: [[LOOP]]:
    // CHECK-NEXT: push [[LENGTH_HOME:[0-9]+]]
    // CHECK-NEXT: mload
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: lt
    // CHECK-NEXT: jumpi {{bb[0-9]+}}, {{bb[0-9]+}}
    function copy(Entry[][] calldata input) external returns (uint256) {
        entries = input;
        return entries[0][1].values[1] + entries[0][0].pair[0] + entries[0][1].pair[1];
    }
}
