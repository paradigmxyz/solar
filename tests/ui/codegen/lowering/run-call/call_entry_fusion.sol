//@ codegen-matrix: standard
//@ run-call: run 0, 0, 42 => 0, 0, 0, 42
//@ run-call: run 3, 2, 99 => 3, 4, 3, 99
//@ run-call: run 7, 7, 1 => 21, 21, 21, 1
//@ run-call: run 1, 6, 17 => 21, 7, 0, 17

contract CallEntryFusion {
    function run(uint256 a, uint256 b, uint256 retained)
        external pure returns (uint256, uint256, uint256, uint256)
    {
        uint256 first = fold(a, b & 7);
        uint256 second = fold(b, a & 7);
        uint256 duplicated = fold(a & 7, a & 7);
        return (first, second, duplicated, retained);
    }

    function fold(uint256 word, uint256 rounds) internal pure returns (uint256 total) {
        unchecked {
            while (rounds != 0) {
                total += (word ^ rounds);
                --rounds;
            }
        }
    }
}
