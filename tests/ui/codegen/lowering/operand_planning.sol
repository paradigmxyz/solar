//@ revisions: gas size
//@[gas] compile-flags: -O gas -Zdump=evm-ir-runtime
//@[size] compile-flags: -O size -Zdump=evm-ir-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// The sum is the only computed operand and dies at `log4`. The planner pushes
// the other five operands in order and computes the sum last, on top, so it
// needs neither a swap nor a deep `dup`.
// CHECK-LABEL: @module ResidentLastUse_runtime
// CHECK: push 4
// CHECK-NEXT: push 3
// CHECK-NEXT: push 2
// CHECK-NEXT: push 1
// CHECK-NEXT: push 64
// CHECK-NEXT: push 36
// CHECK-NEXT: calldataload
// CHECK-NEXT: dup 6
// CHECK-NEXT: calldataload
// CHECK-NEXT: add
// CHECK-NEXT: log4
contract ResidentLastUse {
    function emitSum(uint256 x, uint256 y) external {
        assembly {
            log4(add(x, y), 0x40, 1, 2, 3, 4)
        }
    }
}

// `calldatasize` costs less to read again than to keep on the stack or reload
// from a spill slot, so the planner reads it again at each use.
// CHECK-LABEL: @module SpilledNullary_runtime
// CHECK: push 63
// CHECK-NEXT: calldatasize
// CHECK-NEXT: add
contract SpilledNullary {
    function all() external pure returns (bytes memory) {
        return abi.encodePacked(msg.data);
    }
}
