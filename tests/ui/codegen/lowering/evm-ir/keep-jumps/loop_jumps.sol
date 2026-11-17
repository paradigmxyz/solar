//@ revisions: none size
//@[none] compile-flags: -Onone -Zdump=mir-final,evm-ir-runtime
//@[size] compile-flags: -Osize -Zdump=mir-final,evm-ir-runtime
//@[size] filecheck: --check-prefix=SIZE
//@ normalize-stdout-test: "(?s).+" -> ""

// An unoptimized build keeps the block of each `break` and `continue` statement and ends it in an
// executed `JUMP`, so that the statement and the branch path into it have an instruction of their
// own. The size build threads the branch straight to the loop exit and the loop latch.

// CHECK-LABEL: fn @f()
// CHECK: eq {{v[0-9]+}}, 3
// CHECK-NEXT: jumpi {{v[0-9]+}}, bb[[CONTINUE:[0-9]+]], bb{{[0-9]+}}
// CHECK: eq {{v[0-9]+}}, 7
// CHECK-NEXT: jumpi {{v[0-9]+}}, bb[[BREAK:[0-9]+]], bb{{[0-9]+}}
// CHECK: bb[[BREAK]]:
// CHECK-NEXT: jump bb
// CHECK: bb[[CONTINUE]]:
// CHECK-NEXT: jump bb
// CHECK-LABEL: (runtime) ===
// CHECK: push bb{{[0-9]+}} !metadata(keep_with_next)
// CHECK-NEXT: jump
// CHECK: push bb{{[0-9]+}} !metadata(keep_with_next)
// CHECK-NEXT: jump

// SIZE-LABEL: fn @f()
// SIZE: eq {{v[0-9]+}}, 3
// SIZE-NEXT: jumpi {{v[0-9]+}}, bb[[LATCH:[0-9]+]], bb{{[0-9]+}}
// SIZE: eq {{v[0-9]+}}, 7
// SIZE-NEXT: jumpi {{v[0-9]+}}, bb[[EXIT:[0-9]+]], bb{{[0-9]+}}
// SIZE: bb[[EXIT]]:
// SIZE-NEXT: mstore 0,
// SIZE: bb[[LATCH]]:
// SIZE-NEXT: {{v[0-9]+}} = phi
// SIZE-LABEL: (runtime) ===
// SIZE-NOT: keep_with_next
contract C {
    function f(uint256 n) external pure returns (uint256 sum) {
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
