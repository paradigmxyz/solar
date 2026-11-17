//@ compile-flags: -Onone --emit=srcmap-runtime -Zevm-ir-pipeline=none -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// Leaving an arm of an `if` and starting the code after it implement no source construct, so the
// jump out of each arm and the join's label have no origin. A debugger then stops once on the
// `if`, at its condition, rather than again after the arm ran. The arm's own entry label keeps the
// `if`, and the label of a loop condition, which desugars to `if (cond) body else break`, keeps
// the loop.
contract C {
    uint256 stored;

    // CHECK-LABEL: ; function arms(
    // CHECK: push 3 !metadata(span=
    // CHECK-NEXT: jump [[JOIN:bb[0-9]+]]{{$}}
    // CHECK: push 2 !metadata(span=
    // CHECK-NEXT: jump [[JOIN]]{{$}}
    // CHECK-NEXT: [[JOIN]]:
    // CHECK-NEXT: push 0 !metadata(span=
    function arms(bool c) external returns (uint256 r) {
        if (c) {
            r = 2;
            stored = 1;
        } else {
            r = 3;
            stored = 4;
        }
        return r;
    }

    // An arm that only assigns a local leaves no code of its own: the edge from the branch to the
    // join holds the push of the assignment, which has its origin, and the jump, which has none.
    // CHECK-LABEL: ; function phiArm(
    // CHECK: swap 2 !metadata(span=
    // CHECK-NEXT: pop !metadata(span=
    // CHECK-NEXT: push 1 !metadata(span={{[0-9]+\.\.[0-9]+}})
    // CHECK-NEXT: jump {{bb[0-9]+}}{{$}}
    function phiArm(uint256 n) external pure returns (uint256) {
        uint256 state = 0;
        for (uint256 i = 0; i < n; i++) {
            if (state == 0) {
                if (i % 3 == 0) {
                    state = 1;
                }
            }
        }
        return state;
    }

    // The jump of a `break` is a source construct, so the edge that replaces it keeps its origin.
    // CHECK-LABEL: ; function breakArm(
    // CHECK: pop !metadata(span=
    // CHECK-NEXT: push 1 !metadata(span=
    // CHECK-NEXT: swap 1 !metadata(span=[[#BREAK:]]..{{[0-9]+}})
    // CHECK-NEXT: jump {{bb[0-9]+}} !metadata(span=[[#BREAK]]..{{[0-9]+}})
    function breakArm(uint256 n, uint256 m) external pure returns (uint256 r) {
        for (uint256 i = 0; i < n; i++) {
            if (i == m) {
                r = 1;
                break;
            }
            r = r + i;
        }
    }

    // CHECK-LABEL: ; function loops(
    // CHECK: bb{{[0-9]+}} [loop] !metadata(span=[[#LOOP:]]..{{[0-9]+}}):
    // CHECK: bb{{[0-9]+}} !metadata(span=[[#LOOP]]..{{[0-9]+}}):
    // CHECK: bb{{[0-9]+}} [loop] !metadata(span=[[#LOOP]]..{{[0-9]+}}):
    function loops(uint256 n) external {
        for (uint256 i = 0; i < n; i++) {
            stored += i;
        }
    }
}
