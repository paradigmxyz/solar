//@ compile-flags: -O gas --emit=bin

// Seventeen arguments and the return address exceed `DUP` reach, and the arguments cannot
// spill because they live across a copy that may overwrite low memory. The return address
// must not move to a spill slot there either: the copy overwrites it and the return jumps to
// a calldata word. `-O size` behaves the same; `-O none` finds a stack-only plan.

//~? ERROR: codegen cannot preserve values across a low-memory forwarding buffer in `h`
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
