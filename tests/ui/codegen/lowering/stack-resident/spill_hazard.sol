//@ codegen-matrix: standard
//@ run-call: run 0x80, 5 => 105925
//@ run-call: run 0x80, 12345678901234567890 => 122135617410562660762100
//@ run-call: run 0x200, 7 => 126035

// Twenty words live at once spill to low memory. The copy may overwrite that
// area, so the words still read after it stay on the stack.
contract SpillHazard {
    function run(uint256 dst, uint256 x) external pure returns (uint256 r) {
        unchecked {
            uint256 a0 = x * 3 + 1;
            uint256 a1 = x * 4 + 8;
            uint256 a2 = x * 5 + 15;
            uint256 a3 = x * 6 + 22;
            uint256 a4 = x * 7 + 29;
            uint256 a5 = x * 8 + 36;
            uint256 a6 = x * 9 + 43;
            uint256 a7 = x * 10 + 50;
            uint256 a8 = x * 11 + 57;
            uint256 a9 = x * 12 + 64;
            uint256 a10 = x * 13 + 71;
            uint256 a11 = x * 14 + 78;
            uint256 a12 = x * 15 + 85;
            uint256 a13 = x * 16 + 92;
            uint256 a14 = x * 17 + 99;
            uint256 a15 = x * 18 + 106;
            uint256 a16 = x * 19 + 113;
            uint256 a17 = x * 20 + 120;
            uint256 a18 = x * 21 + 127;
            uint256 a19 = x * 22 + 134;
            uint256 s = a0 * 1 + a1 * 2 + a2 * 3 + a3 * 4 + a4 * 5 + a5 * 6 + a6 * 7 + a7 * 8 + a8 * 9 + a9 * 10 + a10 * 11 + a11 * 12 + a12 * 13 + a13 * 14 + a14 * 15 + a15 * 16 + a16 * 17 + a17 * 18 + a18 * 19 + a19 * 20;
            uint256 t = a19 ^ a18 ^ a17 ^ a16 ^ a15 ^ a14 ^ a13 ^ a12 ^ a11 ^ a10 ^ a9 ^ a8 ^ a7 ^ a6 ^ a5 ^ a4 ^ a3 ^ a2 ^ a1 ^ a0;
            require(s != t);
            assembly {
                calldatacopy(dst, 0, calldatasize())
            }
            r = s * 3 + t + x;
        }
    }
}
