//@ codegen-matrix: standard
//@ run-call: fib 0 => 780
//@ run-call: fib 1 => 5518
//@ run-call: fib 2 => 156712
//@ run-call: fib 7 => 10594469350

// Eighteen words cross the loop backedge, so some header phis spill. Each `a` takes the old
// `b` while `b` takes a new value. The backedge must read every phi input before it stores
// any spilled phi result; reading a spilled input after its slot was overwritten passes the
// new `b` to `a`.
contract C {
    function fib(uint256 n) external pure returns (uint256 r) {
        unchecked {
            uint256 a1 = 1;
            uint256 b1 = 8;
            uint256 a2 = 2;
            uint256 b2 = 15;
            uint256 a3 = 3;
            uint256 b3 = 22;
            uint256 a4 = 4;
            uint256 b4 = 29;
            uint256 a5 = 5;
            uint256 b5 = 36;
            uint256 a6 = 6;
            uint256 b6 = 43;
            uint256 a7 = 7;
            uint256 b7 = 50;
            uint256 a8 = 8;
            uint256 b8 = 57;
            uint256 a9 = 9;
            uint256 b9 = 64;
            for (uint256 i = 0; i < n; i++) {
                (a1, b1) = (b1, a1 + b1 * 2);
                (a2, b2) = (b2, a2 + b2 * 3);
                (a3, b3) = (b3, a3 + b3 * 4);
                (a4, b4) = (b4, a4 + b4 * 5);
                (a5, b5) = (b5, a5 + b5 * 6);
                (a6, b6) = (b6, a6 + b6 * 7);
                (a7, b7) = (b7, a7 + b7 * 8);
                (a8, b8) = (b8, a8 + b8 * 9);
                (a9, b9) = (b9, a9 + b9 * 10);
            }
            r = (a1 * 4) ^ (b1 * 14) ^ (a2 * 5) ^ (b2 * 15) ^ (a3 * 6) ^ (b3 * 16) ^ (a4 * 7) ^ (b4 * 17) ^ (a5 * 8) ^ (b5 * 18) ^ (a6 * 9) ^ (b6 * 19) ^ (a7 * 10) ^ (b7 * 20) ^ (a8 * 11) ^ (b8 * 21) ^ (a9 * 12) ^ (b9 * 22);
        }
    }
}
