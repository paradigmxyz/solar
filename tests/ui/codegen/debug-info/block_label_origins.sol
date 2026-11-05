//@ compile-flags: -Osize --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// The `JUMPDEST` of a block takes the origin MIR placed its label under, as solc places the tag
// under the `LocationSetter` of the statement or function: the branches of the `if`, the loop,
// and the internal function. The `for` starts nine bytes after the `if` ends.

// CHECK-LABEL: (scheduled runtime) ===
// CHECK: bb{{[0-9]+}} [entry, invoke=f@{{[0-9]+..[0-9]+}}]
// CHECK: bb{{[0-9]+}} !metadata(span={{[0-9]+}}..[[#IF_END:]]):
// CHECK: bb{{[0-9]+}} [loop] !metadata(span=[[#IF_END + 9]]..{{[0-9]+}}):
// CHECK: bb{{[0-9]+}} [invoke=helper@[[HELPER:[0-9]+..[0-9]+]], ret=0, inputs=2, outputs=1] !metadata(span=[[HELPER]]):
contract C {
    function helper(uint256 x) internal pure returns (uint256) {
        return x * 3 + 1;
    }

    function f(uint256 a) external pure returns (uint256 r) {
        if (a > 5) {
            r = helper(a);
        } else {
            r = a + 2;
        }
        for (uint256 i = 0; i < a; i++) {
            r += helper(i);
        }
    }
}
