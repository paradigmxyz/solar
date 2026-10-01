//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:

contract Test {
    /// @custom:solar-view data
    function checksum(bytes memory data) internal pure returns (bytes32, uint256) {
        uint256 total;
        for (uint256 i; i < data.length; ++i) {
            total += uint8(data[i]);
        }
        return (keccak256(data), total);
    }

    // A view of calldata is passed as it is, to the copy of `checksum` that
    // takes its parameter in calldata and reads it there: nothing is allocated
    // or copied, and the hash reads a copy made past the free memory pointer.
    // CHECK-LABEL: fn @ofCalldata(
    // CHECK-NOT: mstore 64
    // CHECK: icall @[[CLONE:checksum\.[0-9]+]],
    // CHECK: fn @[[CLONE]](
    // CHECK-NOT: mstore 64
    // CHECK: calldatacopy
    // CHECK-NOT: mstore 64
    // CHECK: calldataload
    function ofCalldata(bytes calldata data) external pure returns (bytes32, uint256) {
        /// @custom:solar-view
        (bytes memory v) = abi.decode(data, (bytes));
        return checksum(v);
    }

    function ofMemory(bytes memory data) public pure returns (bytes32, uint256) {
        return checksum(data);
    }
}
