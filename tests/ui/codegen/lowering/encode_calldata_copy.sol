//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: forward 7, 0x0102 => 0x0000000000000000000000000000000000000000000000000000000000000007000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000020102000000000000000000000000000000000000000000000000000000000000
//@ run-call: modified 7, 0x0102 => 0x0000000000000000000000000000000000000000000000000000000000000007000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000020302000000000000000000000000000000000000000000000000000000000000

// Converting `bytes calldata` to `bytes memory` copies it into memory. When the
// copy is only ever encoded, the encoding copies from calldata and the memory
// copy goes: one `calldatacopy` remains, and the only `mcopy` returns the result.
// CHECK-LABEL: fn @forward
// CHECK: calldatacopy
// CHECK-NOT: calldatacopy
// CHECK: mcopy
// CHECK-NOT: mcopy
// CHECK: returndata

// A copy that the function writes keeps its own bytes, which the encoding reads.
// CHECK-LABEL: fn @modified
// CHECK: calldatacopy
// CHECK: mcopy
// CHECK: mcopy
contract EncodeCalldataCopy {
    function forward(uint256 x, bytes calldata data) external pure returns (bytes memory) {
        bytes memory copy = data;
        return abi.encode(x, copy);
    }

    function modified(uint256 x, bytes calldata data) external pure returns (bytes memory) {
        bytes memory copy = data;
        copy[0] = 0x03;
        return abi.encode(x, copy);
    }
}
