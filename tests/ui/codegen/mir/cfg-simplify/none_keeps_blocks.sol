//@ compile-flags: -Onone -Zdump=mir-final
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// At `-O none` cfg-simplify follows clang's `-O0`: it shares no equal blocks, folds no branch
// whose arms look alike, and merges or forwards no block, so each `return` keeps its own block.
contract C {
    // CHECK-LABEL: fn @f(
    // CHECK: eq
    // CHECK-NEXT: jumpi {{.*}}, [[THEN:bb[0-9]+]], [[ELSE:bb[0-9]+]]
    // CHECK: [[ELSE]]:
    // CHECK-NEXT: jump [[JOIN:bb[0-9]+]]
    // CHECK: [[JOIN]]:
    // CHECK: returndata
    // CHECK: [[THEN]]:
    // CHECK: returndata
    function f(uint256 x) external pure returns (uint256) {
        if (x == 1) {
            return 7;
        }
        return 7;
    }
}
