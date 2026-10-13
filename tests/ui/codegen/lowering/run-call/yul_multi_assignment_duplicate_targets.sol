//@ run-call: plain => 1
//@ run-call: member [8, 7, 6] => 1, 8

// A Yul multi-assignment may repeat a target, and the leftmost value wins.
// solc rejects repeated targets with error 9005; see TYPECK-020 in
// docs/SOLC_DIVERGENCE.md.

contract YulMultiAssignmentDuplicateTargets {
    function plain() external pure returns (uint256 x) {
        assembly {
            function pair() -> a, b {
                a := 1
                b := 2
            }
            x, x := pair()
        }
    }

    function member(uint256[] calldata a) external pure returns (uint256, uint256) {
        assembly {
            function pair() -> x, y {
                x := 1
                y := 2
            }
            a.length, a.length := pair()
        }
        return (a.length, a[0]);
    }
}
