//@ revisions: none gas
//@[none] compile-flags: -Onone
//@[gas] compile-flags: -Ogas
//@ compile-flags: --emit=srcmap-runtime -Zevm-ir-pipeline=none -Zdump=evm-ir-runtime-scheduled
//@[none] filecheck: --check-prefix=NONE
//@[gas] filecheck: --check-prefix=GAS
//@ normalize-stdout-test: "(?s).+" -> ""

// At `-O none` the assignment `x = s` copies the value that stays live with a `DUP` of its own,
// which takes the origin of the assignment. The edge out of the `if` arm only moves the copy, so
// its swaps and its jump have none.
//
// Optimized, an assignment leaves no code of its own: the copy of a value that stays live, on the
// edge out of the `if` arm, runs once per run of the assignment `x = s`, so the `DUP` that makes
// the copy takes its origin. It is the only assignment of `s` in the arm, so the scheduler's
// record names it.
contract C {
    uint256 stored;

    // NONE-LABEL: ; function copyOnArm(
    // NONE: dup 1 !metadata(span=[[#COPY:]]..
    // NONE-SAME: [[#COPY+5]])
    // NONE: sstore
    // NONE-NEXT: swap 1{{$}}
    // NONE-NEXT: jump {{bb[0-9]+}}{{$}}
    // GAS-LABEL: ; function copyOnArm(
    // GAS: sstore
    // GAS-NEXT: dup 1 !metadata(span=[[#COPY:]]..
    // GAS-SAME: [[#COPY+5]])
    // GAS-NEXT: jump
    function copyOnArm(bool c, uint256 a, uint256 b) external returns (uint256) {
        uint256 s = a * b;
        uint256 x = 1;
        if (c) {
            x = s;
            stored = 2;
        }
        return x + s;
    }

    // At `-O none` two assignments store `s` and each copy takes the origin of its own
    // assignment. Optimized, the copy stands for neither, so it keeps the origin of the jump,
    // which leaves the arm of an `if` and has none.
    // NONE-LABEL: ; function sameValueTwice(
    // NONE: dup 1 !metadata(span=[[#X:]]..
    // NONE-SAME: [[#X+5]])
    // NONE-NEXT: dup 2 !metadata(span=[[#W:]]..
    // NONE-SAME: [[#W+5]])
    // NONE: sstore
    // NONE-NEXT: swap 2{{$}}
    // NONE-NEXT: swap 1{{$}}
    // NONE-NEXT: jump {{bb[0-9]+}}{{$}}
    // GAS-LABEL: ; function sameValueTwice(
    // GAS: sstore
    // GAS-NEXT: dup 1{{$}}
    // GAS-NEXT: dup 1{{$}}
    // GAS-NEXT: jump {{bb[0-9]+}}{{$}}
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
