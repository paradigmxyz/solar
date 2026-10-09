//@ revisions: gas size
//@[gas] compile-flags: -O gas -Zdump=evm-ir-runtime
//@[size] compile-flags: -O size -Zdump=evm-ir-runtime
//@[gas] filecheck: --check-prefixes=CHECK,GAS
//@[size] filecheck: --check-prefixes=CHECK,SIZE
//@ normalize-stdout-test: "(?s).+" -> ""

// The sum is the only resident operand and dies at `log4`. The planner pushes
// the other five operands in order and moves the sum down with one swap,
// instead of leaving it in place and copying it with a deep `dup`. Size mode
// keeps the `4` it pushed as the first calldata offset for the last topic, a
// one-byte `dup` instead of a second push.
// CHECK-LABEL: @module ResidentLastUse_runtime
// SIZE: push 4
// SIZE-NEXT: dup 1
// SIZE-NEXT: calldataload
// CHECK: add
// GAS-NEXT: push 3
// GAS-NEXT: push 2
// GAS-NEXT: push 1
// GAS-NEXT: push 64
// GAS-NEXT: push 4
// GAS-NEXT: swap 5
// SIZE-NEXT: push 2
// SIZE-NEXT: push 1
// SIZE-NEXT: push 64
// SIZE-NEXT: push 3
// SIZE-NEXT: swap 4
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
// CHECK: iszero
// CHECK-NEXT: iszero
// CHECK-NEXT: calldatasize
// CHECK-NEXT: mul
contract SpilledNullary {
    function all() external pure returns (bytes memory) {
        return abi.encodePacked(msg.data);
    }
}
