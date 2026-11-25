//@ compile-flags: -Ogas --emit=ethdebug-runtime --pretty-json
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// A tail call whose jump survives layout carries `return` and the callee's `invoke` at -Ogas too.
// Mutual recursion keeps `even` and `odd` from being inlined.
// CHECK-LABEL: "ethdebug-runtime"
// CHECK: "identifier": "even"
// CHECK: "jump": true
// CHECK: "offset": [[EVEN:[0-9]+]]
// CHECK: "identifier": "odd"
// CHECK: "mnemonic": "JUMP"
// CHECK-NOT: "mnemonic"
// CHECK: "identifier": "even"
// CHECK: "jump": true
// CHECK: "offset": [[EVEN]]
// CHECK: "return": {}
// CHECK-NEXT: "transform": [
// CHECK-NEXT: "tailcall"
contract C {
    function even(uint256 n) internal pure returns (bool) {
        if (n == 0) return true;
        return odd(n - 1);
    }

    function odd(uint256 n) internal pure returns (bool) {
        if (n == 0) return false;
        return even(n - 1);
    }

    function e(uint256 n) external pure returns (bool) {
        return even(n);
    }

    function e2(uint256 n) external pure returns (bool) {
        return even(n) && odd(n + 1);
    }
}
