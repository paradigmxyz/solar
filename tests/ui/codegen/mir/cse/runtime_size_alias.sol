//@ codegen-matrix: standard
//@ run-call: clobber 3 => 7, 42
//@ run-call: clobber 0 => 7, 42

// Inline assembly can enlarge an array's length past its allocation, after which a checked index
// reaches the next object: `a[n + 1]` is `b[0]` here. An indexed store into an array of runtime
// size therefore keeps no disjointness from other objects, and the read of `b[0]` after it loads
// the stored word instead of forwarding the 7 stored before.
contract RuntimeSizeAlias {
    function clobber(uint256 n) external pure returns (uint256 before, uint256 afterStore) {
        uint256[] memory a = new uint256[](n);
        uint256[] memory b = new uint256[](1);
        b[0] = 7;
        before = b[0];
        assembly {
            mstore(a, add(n, 2))
        }
        a[n + 1] = 42;
        afterStore = b[0];
    }
}
