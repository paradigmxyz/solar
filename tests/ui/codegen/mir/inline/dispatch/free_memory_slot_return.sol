//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=mir
//@[ir] filecheck:
//@ run-call: Unwritten::raw => 0x0000000000000000000000000000000000000000000000000000000000000080
//@ run-call: Unwritten::sum 17, 23 => 40
//@ run-call: Written::raw 9 => 9
//@ run-call: Written::sum 17, 23 => 40
//@ run-call: Conditional::raw 0 => 0x0000000000000000000000000000000000000000000000000000000000000080
//@ run-call: Conditional::raw 1 => 9
//@ run-call: Conditional::sum 17, 23 => 40

// The dispatcher never initializes the free-memory pointer. A route that returns the slot keeps its
// wrapper, which does, unless its own block overwrites the slot first.
// CHECK-LABEL: @module Unwritten
// CHECK: tail_call @raw
contract Unwritten {
    function raw() external pure returns (uint256) {
        assembly { return(0x40, 32) }
    }

    function sum(uint256 x, uint256 y) external pure returns (uint256) {
        return x + y;
    }
}

// CHECK-LABEL: @module Written
// CHECK-NOT: tail_call
contract Written {
    function raw(uint256 x) external pure returns (uint256) {
        assembly {
            mstore(0x40, x)
            return(0x40, 32)
        }
    }

    function sum(uint256 x, uint256 y) external pure returns (uint256) {
        return x + y;
    }
}

// The store lies in another block than the return.
// CHECK-LABEL: @module Conditional
// CHECK: tail_call @raw
contract Conditional {
    function raw(uint256 x) external pure returns (uint256) {
        assembly {
            if x { mstore(0x40, 9) }
            return(0x40, 32)
        }
    }

    function sum(uint256 x, uint256 y) external pure returns (uint256) {
        return x + y;
    }
}
