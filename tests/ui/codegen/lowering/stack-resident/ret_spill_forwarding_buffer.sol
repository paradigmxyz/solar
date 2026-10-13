//@ codegen-matrix: standard
//@ run-call: g 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17 => 195
//@ run-call: g2 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17 => 196

// Seventeen arguments and the return address exceed `DUP` reach across a copy that may
// overwrite low memory. Spilled words, the return address among them, ride the stack across the
// copy and go back to their slots after it; a slot the copy overwrote would make the result
// wrong or the return jump to a calldata word.

contract C {
    function h(uint256 a1, uint256 a2, uint256 a3, uint256 a4, uint256 a5, uint256 a6, uint256 a7, uint256 a8, uint256 a9, uint256 a10, uint256 a11, uint256 a12, uint256 a13, uint256 a14, uint256 a15, uint256 a16, uint256 a17) internal pure returns (uint256 r) {
        assembly {
            calldatacopy(0, 0, calldatasize())
        }
        unchecked {
            r = (a17 * 18) ^ (a16 * 17) ^ (a15 * 16) ^ (a14 * 15) ^ (a13 * 14) ^ (a12 * 13) ^ (a11 * 12) ^ (a10 * 11) ^ (a9 * 10) ^ (a8 * 9) ^ (a7 * 8) ^ (a6 * 7) ^ (a5 * 6) ^ (a4 * 5) ^ (a3 * 4) ^ (a2 * 3) ^ (a1 * 2);
        }
    }

    function g(uint256 a1, uint256 a2, uint256 a3, uint256 a4, uint256 a5, uint256 a6, uint256 a7, uint256 a8, uint256 a9, uint256 a10, uint256 a11, uint256 a12, uint256 a13, uint256 a14, uint256 a15, uint256 a16, uint256 a17) external pure returns (uint256) {
        return h(a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15, a16, a17) + 1;
    }

    function g2(uint256 a1, uint256 a2, uint256 a3, uint256 a4, uint256 a5, uint256 a6, uint256 a7, uint256 a8, uint256 a9, uint256 a10, uint256 a11, uint256 a12, uint256 a13, uint256 a14, uint256 a15, uint256 a16, uint256 a17) external pure returns (uint256) {
        return h(a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14, a15, a16, a17) + 2;
    }
}
