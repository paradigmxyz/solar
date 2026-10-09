//@ revisions: gas size
//@[gas] compile-flags: -O gas -Zdump=evm-ir-runtime
//@[size] compile-flags: -O size -Zdump=evm-ir-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// The sum is the only resident operand and dies at `log4`. The planner pushes
// the other five operands in order and moves the sum down with one swap,
// instead of leaving it in place and copying it with a deep `dup`.
// CHECK-LABEL: @module ResidentLastUse_runtime
// CHECK: add
// CHECK-NEXT: push 3
// CHECK-NEXT: push 2
// CHECK-NEXT: push 1
// CHECK-NEXT: push 64
// CHECK-NEXT: push 4
// CHECK-NEXT: swap 5
// CHECK-NEXT: log4
contract ResidentLastUse {
    function emitSum(uint256 x, uint256 y) external {
        assembly {
            log4(add(x, y), 0x40, 1, 2, 3, 4)
        }
    }
}

// `calldatasize` costs less to read again than to reload from a spill slot, so
// the planner reads it again instead of loading the spilled copy.
// CHECK-LABEL: @module SpilledNullary_runtime
// CHECK: push 63
// CHECK-NEXT: calldatasize
// CHECK-NEXT: add
contract SpilledNullary {
    function all() external pure returns (bytes memory) {
        return abi.encodePacked(msg.data);
    }
}
