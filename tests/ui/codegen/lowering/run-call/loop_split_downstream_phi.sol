//@ codegen-matrix: standard
//@ run-call: nestedExit 2, 5, 3 => 12
//@ run-call: nestedExit 2, 5, 0 => 0
//@ run-call: nestedExit 2, 3, 10 => 6

contract LoopSplitDownstreamPhi {
    function nestedExit(uint256 rounds, uint256 bound, uint256 stop) external pure returns (uint256 total) {
        unchecked {
            for (uint256 i; i < rounds; ++i) {
                for (uint256 j; j < bound; ++j) {
                    if (j >= stop) break;
                    if (j + 1 < bound) total += j + 1;
                }
            }
        }
    }
}
