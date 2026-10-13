//@ compile-flags: -Zdump=evm-ir-runtime
//@ filecheck: --implicit-check-not=mload
//@ run-call: trimLen 0x0102030405 => 1
//@ run-call: trimLen 0x01020304 => 4
//@ run-call: trimLen 0x => 0

// `trim` returns one of two slices. Each path leaves its slice on the stack and
// jumps straight back to the caller, so no join and no memory carry it.
contract AcyclicStackPhi {
    // CHECK-LABEL: @module AcyclicStackPhi_runtime
    // CHECK: push 4{{[[:space:]]+}}dup 4{{[[:space:]]+}}gt
    // CHECK-NEXT: push [[SLICE:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: jump
    // CHECK: [continuation]:
    // CHECK: return
    // CHECK: [[SLICE]]:
    // CHECK: jump
    function trimLen(bytes calldata data) external pure returns (uint256) {
        return trim(data).length;
    }

    function trim(bytes calldata data) internal pure returns (bytes calldata) {
        if (data.length > 4) return data[4:];
        return data;
    }
}
