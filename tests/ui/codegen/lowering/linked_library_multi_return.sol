//@ codegen-matrix: standard
//@compile-flags: --libraries Lib=0x1111111111111111111111111111111111111111
//@filecheck:

library Lib {
    function pair() public pure returns (uint256, uint256) {
        return (4, 5);
    }

    function dynamicPair(uint256 value) public pure returns (uint256, bytes memory) {
        return (value, abi.encode(value + 1));
    }
}

// CHECK-LABEL: @module C
contract C {
    using Lib for uint256;
    // Linked-library calls return through DELEGATECALL. Lowering decodes the
    // returned words before tuple extraction.
    // CHECK-LABEL: fn @pair{{[( ]}}
    // CHECK: delegatecall
    // CHECK: returndatasize
    // CHECK: abi_decode [u256, u256]
    // CHECK-NOT: frame_store
    // CHECK: mload
    // CHECK: ret
    function pair() external pure returns (uint256, uint256) {
        (uint256 first, uint256 second) = Lib.pair();
        assembly { mstore(0x40, 0x80) }
        return (first, second);
    }

    // CHECK-LABEL: fn @attached
    // CHECK: delegatecall
    // CHECK: abi_decode [u256, bytes]
    // CHECK-NOT: frame_store
    // CHECK: ret
    function attached(uint256 value) external pure returns (uint256, uint256) {
        (uint256 first, bytes memory data) = value.dynamicPair();
        uint256 second = abi.decode(data, (uint256));
        assembly { mstore(0x40, 0x80) }
        return (first, second);
    }
}
