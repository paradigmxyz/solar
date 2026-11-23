//@ compile-flags: -Onone --emit=srcmap-runtime -Zevm-ir-pipeline=none -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// At `-O none` the assignment `x = s` copies the value that stays live with a `DUP` of its own,
// which takes the origin of the assignment. The edge out of the `if` arm only moves the copy, so
// its swaps and its jump have none.
contract C {
    uint256 stored;

    // CHECK-LABEL: ; function copyOnArm(
    // CHECK: dup 1 !metadata(span=[[#COPY:]]..
    // CHECK-SAME: [[#COPY+5]])
    // CHECK: sstore
    // CHECK-NEXT: swap 1{{$}}
    // CHECK-NEXT: jump {{bb[0-9]+}}{{$}}
    function copyOnArm(bool c, uint256 a, uint256 b) external returns (uint256) {
        uint256 s = a * b;
        uint256 x = 1;
        if (c) {
            x = s;
            stored = 2;
        }
        return x + s;
    }

    // Two assignments store `s`: each copy takes the origin of its own assignment.
    // CHECK-LABEL: ; function sameValueTwice(
    // CHECK: dup 1 !metadata(span=[[#X:]]..
    // CHECK-SAME: [[#X+5]])
    // CHECK-NEXT: dup 2 !metadata(span=[[#W:]]..
    // CHECK-SAME: [[#W+5]])
    // CHECK: sstore
    // CHECK-NEXT: swap 2{{$}}
    // CHECK-NEXT: swap 1{{$}}
    // CHECK-NEXT: jump {{bb[0-9]+}}{{$}}
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
