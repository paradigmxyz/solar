//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: shorten [1, 2, 3, 4], 2 => [1, 2]
//@ run-call: viaAlias [7, 8, 9], 1 => 1
//@ run-call: viaAlias [1, 2, 3], 5 => 5

// The same store of the length word, bare. On valid input it agrees. Asked to
// shorten a three-element array to five it does not fail: the array now
// claims two elements it never had, in whatever memory follows it.
// CHECK-LABEL: fn @shorten
// CHECK: mstore {{v[0-9]+}}, arg1
contract Unsafe {
    function shorten(uint256[] memory a, uint256 n) public pure returns (uint256[] memory) {
        assembly ("memory-safe") {
            mstore(a, n)
        }
        return a;
    }

    function viaAlias(uint256[] memory a, uint256 n) public pure returns (uint256) {
        uint256[] memory b = a;
        assembly ("memory-safe") {
            mstore(b, n)
        }
        return a.length;
    }
}
