//@ codegen-matrix: standard runtime
//@[runtime] compile-flags: -Ogas -Zdump=disasm-runtime
//@[runtime] filecheck: --check-prefix=RUNTIME
//@ run-call: f 1, 2, 1267650600228229401496703205376 => 0
//@ run-call: f 1, 0, 1267650600228229401496703205504 => 0
//@ run-call-fail: f 1, 2, 1267650600228229401496703205504

// The first mask reaches the join as a phi input from a block outside the region that a retained
// constant lives in, and the join then reads it twice. The edge pushes the mask where it runs, in
// the predecessor, and the join keeps the pushed word, so the second read is a copy.
// RUNTIME-LABEL: // === {{.*}}:C (runtime) ===
// RUNTIME-COUNT-2: PUSH8 0xffffffffffffffff
// RUNTIME-NOT: PUSH8 0xffffffffffffffff
contract C {
    function f(uint256 a, uint256 b, uint256 c) external pure returns (uint256 r) {
        uint256 v = 0xffffffffffffffff;
        uint256 w = a;
        if ((0xdeadbeefcafebabe1234567 | a) < (0xdeadbeefcafebabe1234567 ^ c)) {
            v = 0xabcdef0123456789abcdef0123456789abcdef01234567;
        } else {
            w = 0xdeadbeefcafebabe1234567 ^ (c - 0xdeadbeefcafebabe1234567);
        }
        if ((b & 0xffffffffffffffff) != 0) {
            if ((v & 0xffffffffffffffff) != 0) {
                for (uint256 i; i < (w & 3) + 1; ++i) {
                    if ((c >> 7) & 1 == 1) {
                        revert();
                    }
                }
            }
        }
    }
}
