//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: cleanup() => 3, 0

contract StorageArrayAssignmentCleanup {
    uint256[] private values;

    function cleanup() external returns (uint256 first, uint256 stale) {
        uint256[] memory initial = new uint256[](2);
        initial[0] = 1;
        initial[1] = 2;
        values = initial;

        uint256[] memory replacement = new uint256[](1);
        replacement[0] = 3;
        values = replacement;
        assembly {
            mstore(0, 0)
            let base := keccak256(0, 0x20)
            stale := sload(add(base, 1))
        }
        return (values[0], stale);
    }
}
