//@ compile-flags: -Onone --emit=ethdebug-runtime --pretty-json -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// An internal call's `invoke` marks the callee's entry `JUMPDEST`, where the ethdebug format puts
// it, and its `return` marks the `JUMP` that leaves. The first call enters `g` by falling into it,
// the second by a jump, and the one entry event covers both.
// CHECK-LABEL: "ethdebug-runtime"
// CHECK: "identifier": "f"
// CHECK: "mnemonic": "JUMPDEST"
// CHECK-NOT: "mnemonic"
// CHECK: "invoke": {
// CHECK-NEXT: "identifier": "g"
// CHECK: "jump": true
// CHECK-NEXT: "target": {
// CHECK-NEXT: "pointer": {
// CHECK-NEXT: "location": "code"
// CHECK: "mnemonic": "JUMP"
// CHECK-NOT: "mnemonic"
// CHECK: "return": {}
contract C {
    function g(uint256 x) internal pure returns (uint256) {
        return x * 3;
    }

    function f(uint256 a) external pure returns (uint256) {
        return g(a) + g(a + 1);
    }
}
