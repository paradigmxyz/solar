//@ compile-flags: -Onone --emit=srcmap-runtime -Zevm-ir-pipeline=none -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// The copy of a value that stays live, on the edge out of the `if` arm, runs once per run of the
// assignment `x = s`, so the `DUP` that makes the copy takes its origin. It is the only assignment
// of `s` in the arm, so the scheduler's record names it.
contract C {
    uint256 stored;

    // CHECK-LABEL: ; function copyOnArm(
    // CHECK: sstore
    // CHECK-NEXT: dup 1 !metadata(span=[[#COPY:]]..
    // CHECK-SAME: [[#COPY+5]])
    // CHECK-NEXT: jump
    function copyOnArm(bool c, uint256 a, uint256 b) external returns (uint256) {
        uint256 s = a * b;
        uint256 x = 1;
        if (c) {
            x = s;
            stored = 2;
        }
        return x + s;
    }

    // Two assignments stored `s`: the copy stands for neither, so it keeps the origin of the jump.
    // CHECK-LABEL: ; function sameValueTwice(
    // CHECK: sstore
    // CHECK-NEXT: dup 1 !metadata(span=[[#JUMP:]]..{{[0-9]+}})
    // CHECK-NEXT: dup 1 !metadata(span=[[#JUMP]]..{{[0-9]+}})
    // CHECK-NEXT: jump {{bb[0-9]+}} !metadata(span=[[#JUMP]]..{{[0-9]+}})
    function sameValueTwice(bool c, uint256 a, uint256 b) external returns (uint256) {
        uint256 s = a * b;
        uint256 x = 1;
        uint256 w = 2;
        if (c) {
            x = s;
            w = s;
            stored = 3;
        }
        return x + w + s;
    }
}
