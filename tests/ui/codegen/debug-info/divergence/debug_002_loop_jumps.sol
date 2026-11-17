//@ compile-flags: -Onone --emit=srcmap-runtime -Zdump=disasm-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// DEBUG-002: a `break` or `continue` has no instruction of its own. The branch of the `if` that
// guards it jumps to the loop exit or to the loop's increment, so no jump is left for the
// statement.

// Offsets are bytes from `contract C`. An entry that changes the location writes its start, so a
// location with no instruction never appears.

// CHECK: "srcmap-runtime":"[[#C:]]:
// `i == 3`
// CHECK: {{[;"]}}[[#C+134]]{{[:;]}}
// `continue;`
// CHECK-NOT: {{[;"]}}[[#C+160]]{{[:;]}}
// `i == 7`
// CHECK: {{[;"]}}[[#C+200]]{{[:;]}}
// `break;`
// CHECK-NOT: {{[;"]}}[[#C+226]]{{[:;]}}
// `sum += i`
// CHECK: {{[;"]}}[[#C+259]]{{[:;]}}
contract C {
    function loop(uint256 n) external pure returns (uint256 sum) {
        for (uint256 i; i < n; i++) {
            if (i == 3) {
                continue;
            }
            if (i == 7) {
                break;
            }
            sum += i;
        }
    }
}
