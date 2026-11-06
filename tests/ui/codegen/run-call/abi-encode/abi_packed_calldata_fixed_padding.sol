//@ codegen-matrix: standard
//@ filecheck:
//@ run-call: packed 0 => 0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
//@ run-call: packed 1 => 0x00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000
//@ run-call: packed 255 => 0x00000000000000000000000000000000000000000000000000000000000000ff0000000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: packed 256 => 0x
//@ run-call: hashed 1 => 0xada5013122d395ba3c54772283fb069b10426056ef8ca54750cb9bb552a59e7d
//@ run-call-fail: hashed 256 => 0x
//@ run-call: signed -1 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff0000000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: signed 128 => 0x
//@ run-call: flags 1 => 0x00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: flags 2 => 0x
//@ run-call: fixedBytes 0xab00000000000000000000000000000000000000000000000000000000000000 => 0xab000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: fixedBytes 1 => 0x
//@ run-call-fail: checkedCopy 1 => 0x

// Packed encoding validates each calldata word, including zero-padded words beyond the input.
// Ordinary calldata-to-memory conversion still requires the full source range.
contract AbiPackedCalldataFixedPadding {
    // CHECK-LABEL: fn @packed(
    // CHECK-NOT: calldatasize
    // CHECK: abi_encode_packed
    function packed(uint256) external pure returns (bytes memory) {
        uint8[2] calldata values;
        assembly { values := 4 }
        return abi.encodePacked(values);
    }

    // CHECK-LABEL: fn @hashed(
    // CHECK-NOT: calldatasize
    // CHECK: abi_encode_packed
    function hashed(uint256) external pure returns (bytes32) {
        uint8[2] calldata values;
        assembly { values := 4 }
        return keccak256(abi.encodePacked(values));
    }

    function signed(int256) external pure returns (bytes memory) {
        int8[2] calldata values;
        assembly { values := 4 }
        return abi.encodePacked(values);
    }

    function flags(uint256) external pure returns (bytes memory) {
        bool[2] calldata values;
        assembly { values := 4 }
        return abi.encodePacked(values);
    }

    function fixedBytes(uint256) external pure returns (bytes memory) {
        bytes1[2] calldata values;
        assembly { values := 4 }
        return abi.encodePacked(values);
    }

    function checkedCopy(uint256) external pure returns (uint8[2] memory) {
        uint8[2] calldata values;
        assembly { values := 4 }
        return values;
    }
}
