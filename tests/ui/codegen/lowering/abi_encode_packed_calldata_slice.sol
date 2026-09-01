//@compile-flags: -Zdump=evm-ir-runtime
//@ filecheck:

// `abi.encodePacked(...)` may include a slice of a `bytes`/`string` calldata
// value, `data[start:end]` (open bounds default to `0`/`data.length`). The
// sliced calldata range is copied into a `[len][data]` memory buffer and packed
// as raw data. The packed bytes (hashed here) are verified equal to solc 0.8.30
// separately, for bounded, open, and prefixed slices.

contract AbiEncodePackedCalldataSlice {
    // CHECK: push 0xd07523
    // CHECK: eq
    // CHECK-NEXT: push [[HASH_BODY:bb[0-9]+]]
    // CHECK: push 0x2a5dea89
    // CHECK: eq
    // CHECK-NEXT: push [[OPEN_BODY:bb[0-9]+]]
    // CHECK: [[HASH_BODY]]:
    // CHECK: calldatacopy
    // CHECK: mcopy
    // CHECK: keccak256
    // CHECK: return
    // CHECK: [[OPEN_BODY]]:
    // CHECK: calldatacopy
    // CHECK: push 112
    // CHECK: mcopy
    // CHECK: keccak256
    // CHECK: return
    function hash(bytes calldata data, uint256 start, uint256 end) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(data[start:end]));
    }

    function open(bytes calldata data, uint256 start) external pure returns (bytes32) {
        return keccak256(abi.encodePacked("p", data[start:], uint256(1)));
    }
}
