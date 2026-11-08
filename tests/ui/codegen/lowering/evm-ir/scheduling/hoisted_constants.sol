//@ codegen-matrix: standard
//@[gas] compile-flags: -Zdump=evm-ir-runtime-scheduled
//@[gas] filecheck:
//@ run-call: store 3, 5 => 7
//@ run-call: store 4, 4 => 7
//@ run-call: clear 3, 5 => 0

// The store takes its constant below the address that it computes from two calldata words. The
// planner pushes the constant before the computation, instead of swapping it down after it, and
// the push keeps the origin of the store. A zero stays where the planner pushes it.

contract HoistedConstants {
    // CHECK-LABEL: ; function store(uint256 a, uint256 b)
    // CHECK: push 7 !metadata(span=[[STORE:[0-9]+..[0-9]+]])
    // CHECK-NEXT: push 36
    // CHECK-NEXT: calldataload
    // CHECK: mul
    // CHECK-NEXT: add
    // CHECK-NEXT: mstore !metadata(span=[[STORE]])
    function store(uint256 a, uint256 b) public pure returns (uint256 r) {
        assembly {
            mstore(add(mul(a, b), b), 7)
            r := mload(20)
        }
    }

    // CHECK-LABEL: ; function clear(uint256 a, uint256 b)
    // CHECK: mul
    // CHECK-NEXT: add
    // CHECK-NEXT: push 0
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: mstore
    function clear(uint256 a, uint256 b) public pure returns (uint256 r) {
        assembly {
            mstore(add(mul(a, b), b), 0)
            r := mload(20)
        }
    }
}
