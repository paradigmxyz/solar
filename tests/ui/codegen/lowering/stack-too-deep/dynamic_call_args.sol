//@ revisions: gas size
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@ run-call: run 1 => 190

// Recursive internal functions use dynamic frames. The anonymous frame base adds a physical stack
// word before arguments are stored, so the deepest argument needs a spill route before that push.
contract DynamicCallArgs {
    function run(uint256 x) external pure returns (uint256) {
        return recurse(
            1,
            x,
            x + 1,
            x + 2,
            x + 3,
            x + 4,
            x + 5,
            x + 6,
            x + 7,
            x + 8,
            x + 9,
            x + 10,
            x + 11,
            x + 12,
            x + 13,
            x + 14,
            x + 15,
            x + 16,
            x + 17,
            x + 18
        );
    }

    function recurse(
        uint256 depth,
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
        uint256 a17,
        uint256 a18
    ) internal pure returns (uint256) {
        if (depth != 0) {
            return recurse(
                depth - 1,
                a0,
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
                a16,
                a17,
                a18
            );
        }
        return a0 + a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8 + a9 + a10 + a11 + a12 + a13
            + a14 + a15 + a16 + a17 + a18;
    }
}
