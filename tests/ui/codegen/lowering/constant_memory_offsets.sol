//@ compile-flags: -O gas -Zdump=evm-ir-runtime
//@ filecheck: --implicit-check-not=mul

// CHECK-LABEL: @module runtime
// CHECK-LABEL: bb0:
// CHECK: calldatacopy
// CHECK-NEXT: push 64
// CHECK-NEXT: dup 2
// CHECK-NEXT: add
// CHECK-NEXT: push 7
// CHECK-LABEL: bb6:
// CHECK: push 32
// CHECK-NEXT: dup 2
// CHECK-NEXT: add
// CHECK-NEXT: push 64
// CHECK-NEXT: add
// CHECK-NEXT: push 7

contract ConstantMemoryOffsets {
    function fixedArray() public pure returns (bytes32 result) {
        uint256[4] memory values;
        values[2] = 7;
        assembly {
            result := keccak256(values, 128)
        }
    }

    function dynamicArray() public pure returns (bytes32 result) {
        uint256[] memory values = new uint256[](3);
        values[2] = 7;
        assembly {
            result := keccak256(add(values, 32), 96)
        }
    }
}
