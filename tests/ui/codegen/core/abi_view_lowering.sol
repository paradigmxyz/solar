//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:

contract Test {
    struct Pair {
        uint256 a;
        bytes b;
    }

    // A view of calldata reads calldata. Hashing it copies the bytes past the
    // free memory pointer without allocating, so the pointer is never written.
    // CHECK-LABEL: fn @viewed(
    // CHECK-NOT: mstore 64
    // CHECK: calldatacopy
    // CHECK-NEXT: keccak256
    // CHECK-NOT: mstore 64
    // CHECK: returndata
    function viewed(bytes calldata data) external pure returns (uint256 id, bytes32 hash) {
        /// @custom:solar-view
        (uint256 n, bytes memory payload) = abi.decode(data, (uint256, bytes));
        id = n;
        hash = keccak256(payload);
    }

    // Without the tag, the decode copies the calldata to memory first.
    // CHECK-LABEL: fn @copied(
    // CHECK: mstore 64
    // CHECK: calldatacopy
    // CHECK: icall @decode_aggregate
    function copied(bytes calldata data) external pure returns (uint256 id, bytes32 hash) {
        (uint256 n, bytes memory payload) = abi.decode(data, (uint256, bytes));
        id = n;
        hash = keccak256(payload);
    }

    // A view of memory hashes the decoded bytes where they are.
    // CHECK-LABEL: fn @viewedMemory(
    // CHECK-NOT: mstore 64
    // CHECK-NOT: icall @decode_aggregate
    // CHECK: keccak256
    // CHECK: returndata
    function viewedMemory(bytes memory data) public pure returns (uint256 id, bytes32 hash) {
        /// @custom:solar-view
        (uint256 n, bytes memory payload) = abi.decode(data, (uint256, bytes));
        id = n;
        hash = keccak256(payload);
    }

    // Arrays and structs are views too, read where the decode validated them:
    // nothing is allocated or copied.
    // CHECK-LABEL: fn @nestedViewed(
    // CHECK-NOT: mstore 64
    // CHECK-NOT: icall @decode_aggregate
    // CHECK-NOT: calldatacopy
    // CHECK: returndata
    function nestedViewed(bytes calldata data) external pure returns (uint256 total) {
        /// @custom:solar-view
        (uint256[][] memory rows, Pair memory pair) = abi.decode(data, (uint256[][], Pair));
        for (uint256 i; i < rows.length; ++i) {
            total += rows[i].length;
        }
        total += pair.a + pair.b.length;
    }

    // CHECK-LABEL: fn @copiedMemory(
    // CHECK: icall @decode_aggregate
    function copiedMemory(bytes memory data) public pure returns (uint256 id, bytes32 hash) {
        (uint256 n, bytes memory payload) = abi.decode(data, (uint256, bytes));
        id = n;
        hash = keccak256(payload);
    }
}
