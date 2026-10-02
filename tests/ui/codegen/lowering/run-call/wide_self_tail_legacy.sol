//@ revisions: osakaGas osakaSize amsterdamGas
//@[osakaGas] compile-flags: -O gas --evm-version osaka
//@[osakaSize] compile-flags: -O size --evm-version osaka
//@[amsterdamGas] compile-flags: -O gas --evm-version amsterdam
//@ run-call: run 1 => 171

contract WideSelfTailLegacy {
    function recurse(
        uint256 a0,
        uint256 a1,
        uint256 a2,
        uint256 a3,
        uint256 a4,
        uint256 a5,
        uint256 a6,
        uint256 a7,
        uint256 a8,
        uint256 a9,
        uint256 a10,
        uint256 a11,
        uint256 a12,
        uint256 a13,
        uint256 a14,
        uint256 a15,
        uint256 a16,
        uint256 a17
    ) internal pure {
        if (a0 == 0) return;
        recurse(
            a0 - 1,
            a17,
            a1,
            a2,
            a3,
            a4,
            a5,
            a6,
            a7,
            a8,
            a9,
            a10,
            a11,
            a12,
            a13,
            a14,
            a15,
            a16
        );
    }

    function run(uint256 rounds) external pure returns (uint256) {
        recurse(rounds, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
        return 171;
    }
}
