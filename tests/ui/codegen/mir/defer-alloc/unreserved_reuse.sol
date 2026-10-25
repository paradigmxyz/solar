//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: stage 0x => 0x0102030405
//@ run-call: stage 0x01010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101010101  => 0x0102030405
//@ run-call: zeroed 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0
//@ run-call: zeroed 0x => 0

// The delegatecall input buffer is dead once the call returns, so it stays unreserved at the
// free-memory pointer and the copied returndata lands on the same memory.
// CHECK-LABEL: fn @stage{{.*}}
// CHECK: = mload 64
// CHECK-NOT: mstore 64
// CHECK: delegatecall

// Both the input buffer and the returndata copy are unreserved. The checked, zeroed object that
// follows keeps its bump and clears its memory explicitly.
// CHECK-LABEL: fn @zeroed{{.*}}
// CHECK: = mload 64
// CHECK-NOT: mstore 64
// CHECK: staticcall
// CHECK-NOT: mstore 64
// CHECK: returndatacopy
// CHECK: mstore 64
// CHECK: calldatacopy {{v[0-9]+}}, {{v[0-9]+}}, 128

contract UnreservedReuse {
    address private immutable impl = address(new Impl());

    // The second object is shorter than the first buffer. The encoding must pad it with zeros
    // instead of the dead first buffer's bytes.
    function stage(bytes calldata input) external returns (bytes memory) {
        (bool success, bytes memory output) = impl.delegatecall(input);
        require(success);
        return output;
    }

    // A zeroed allocation over the dirty memory of the dead buffers must still read zero.
    function zeroed(bytes calldata input) external view returns (uint256 dirty) {
        (bool success, bytes memory echoed) = address(4).staticcall(input);
        require(success && echoed.length == input.length);
        bytes memory fresh = new bytes(96);
        assembly ("memory-safe") {
            dirty := or(or(mload(add(fresh, 32)), mload(add(fresh, 64))), mload(add(fresh, 96)))
        }
    }
}

contract Impl {
    fallback(bytes calldata) external returns (bytes memory) {
        return hex"0102030405";
    }
}
