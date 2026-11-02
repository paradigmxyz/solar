//@ codegen-matrix: standard
//@ run-call: f 0 => 0
//@ run-call: f 1 => 1
//@ run-call: f 2 => 3
//@ run-call: f 3 => 5
//@ run-call: f 10 => 19

contract RecursiveManyReturns {
    // A recursive call keeps the return address and the spilled words below its operands, and
    // the 18 results land above them. All but two are dead, so the call drops them to restore
    // the saved words.
    function g(uint256 n) internal pure returns (uint256 r0, uint256 r1, uint256 r2, uint256 r3, uint256 r4, uint256 r5, uint256 r6, uint256 r7, uint256 r8, uint256 r9, uint256 r10, uint256 r11, uint256 r12, uint256 r13, uint256 r14, uint256 r15, uint256 r16, uint256 r17) {
        r1 = n;
        if (n != 0) {
            (, uint256 a1, , , , , , , , , , , , , , , , ) = g(n - 1);
            r0 = a1 + n;
        }
    }

    function f(uint256 n) external pure returns (uint256 r) {
        (r, , , , , , , , , , , , , , , , , ) = g(n);
    }
}
