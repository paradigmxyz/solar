//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: f(uint256[],bytes) [255], 0x313233343536 => 0xeb2efe930fbfab5fd98a6fca96de04949a4b3f87b839ff47f531d0cadcd833a2
//@ run-call: g(uint256[],bytes) [65535], 0x3132333435363738 => 0x1153b57430060732616b43977897ab432cb8e19a3ccc7a17813c41318f42c0d0
// ported-from: test/libsolidity/semanticTests/abicoder/abi_encode_memory_dynamic_array_and_calldata_bytes_v1.sol

pragma abicoder v1;

contract AbiEncodeMemoryCalldataV1 {
    function f(uint256[] memory values, bytes calldata data) public pure returns (bytes32) {
        return keccak256(abi.encode(values, data));
    }

    function g(uint256[] memory values, bytes calldata data) external pure returns (bytes32) {
        return f(values, data);
    }
}
