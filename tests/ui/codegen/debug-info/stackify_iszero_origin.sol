//@ compile-flags: -Ogas --emit=srcmap-runtime -Zdump=evm-ir-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// Stackify peels `eq v, 0` off the condition of `b % 3 == 0 || ...` and inverts the branch, then
// inverts it again so the `then` successor falls through. The `ISZERO` it emits is the peeled `eq`
// and keeps its span, `b % 3 == 0`, while the `PUSH` and `JUMPI` keep the span of the `||`.

// CHECK-LABEL: (runtime) ===
// CHECK: mod !metadata(span=[[#B:]]..{{[0-9]+}})
// CHECK-NEXT: iszero !metadata(span=[[#B]]..[[#B + 10]])
// CHECK-NEXT: push {{bb[0-9]+}} !metadata(span=[[#B]]..[[#B + 24]])
// CHECK-NEXT: jumpi !metadata(span=[[#B]]..[[#B + 24]])
contract StackifyIszeroOrigin {
    function f(uint256 a, uint256 b) public pure returns (uint256) {
        require(b % 3 == 0 || a % 2 == 0);
        return a + b;
    }
}
