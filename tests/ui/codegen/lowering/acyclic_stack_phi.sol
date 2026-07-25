//@ compile-flags: -Zcodegen -Zdump=evm-ir-runtime
//@ filecheck:

contract AcyclicStackPhi {
    // CHECK-LABEL: @module runtime
    // CHECK: push 0xbcc0f6fa
    // CHECK-NEXT: eq
    // CHECK-NEXT: push [[ENTRY:bb[0-9]+]]
    // CHECK: [[ENTRY]]:
    // CHECK: jump [[MERGE:bb[0-9]+]]
    // CHECK-NEXT: [[MERGE]]:
    // CHECK-NEXT: pop
    // CHECK-NEXT: dup1
    function trim(bytes calldata data) external pure returns (bytes calldata) {
        if (data.length > 4) return data[4:];
        return data;
    }
}
