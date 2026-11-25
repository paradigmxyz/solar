//@ compile-flags: -Onone --emit=ethdebug-runtime --pretty-json
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// A tail call leaves the caller and enters the callee with one `JUMP`, so that `JUMP` carries the
// `return` and the callee's `invoke` together with `transform: ["tailcall"]`, as the ethdebug
// format marks a tail call. The `invoke` target is the callee's entry `JUMPDEST`, which keeps its
// own `invoke`. A call that has work left after it is no tail call.
// CHECK-LABEL: "ethdebug-runtime"

// A call whose result is still used is a plain call, and so is recursion that multiplies it.
// CHECK: "identifier": "fact"
// CHECK-NOT: "tailcall"

// An inline-assembly function that calls another one in tail position.
// CHECK: "identifier": "viaTwice"
// CHECK: "mnemonic": "JUMP"
// CHECK-NOT: "mnemonic"
// CHECK: "identifier": "twice"
// CHECK: "jump": true
// CHECK: "offset": [[TWICE:[0-9]+]]
// CHECK: "return": {}
// CHECK-NEXT: "transform": [
// CHECK-NEXT: "tailcall"
// CHECK: "offset": [[TWICE]],
// CHECK: "mnemonic": "JUMPDEST"
// CHECK-NOT: "mnemonic"
// CHECK: "identifier": "twice"
// CHECK: "return": {}
// CHECK-NOT: "tailcall"

// A Solidity function that calls another one in tail position.
// CHECK: "identifier": "tailOuter"
// CHECK: "mnemonic": "JUMP"
// CHECK-NOT: "mnemonic"
// CHECK: "identifier": "leaf"
// CHECK: "jump": true
// CHECK: "offset": [[LEAF:[0-9]+]]
// CHECK: "return": {}
// CHECK-NEXT: "transform": [
// CHECK-NEXT: "tailcall"
// CHECK: "offset": [[LEAF]],
// CHECK: "mnemonic": "JUMPDEST"
// CHECK-NOT: "mnemonic"
// CHECK: "identifier": "leaf"
// CHECK: "return": {}
// CHECK-NOT: "tailcall"

// A call in the middle of an expression.
// CHECK: "identifier": "nested"
// CHECK-NOT: "tailcall"

// A function that calls itself in tail position jumps back to its own entry.
// CHECK: "identifier": "countdown"
// CHECK: "jump": true
// CHECK: "offset": [[COUNTDOWN:[0-9]+]]
// CHECK: "mnemonic": "JUMP"
// CHECK-NOT: "mnemonic"
// CHECK: "identifier": "countdown"
// CHECK: "jump": true
// CHECK: "offset": [[COUNTDOWN]]
// CHECK: "return": {}
// CHECK-NEXT: "transform": [
// CHECK-NEXT: "tailcall"
contract Calls {
    function leaf(uint256 x) internal pure returns (uint256) {
        return x * 3;
    }

    function nested(uint256 x) internal pure returns (uint256) {
        return leaf(x) + 1;
    }

    function fact(uint256 n) internal pure returns (uint256) {
        if (n == 0) return 1;
        return n * fact(n - 1);
    }

    function tailOuter(uint256 x) internal pure returns (uint256) {
        return leaf(x + 1);
    }

    function countdown(uint256 n, uint256 acc) internal pure returns (uint256) {
        if (n == 0) return acc;
        return countdown(n - 1, acc + n);
    }

    function runNested(uint256 a) external pure returns (uint256) {
        return nested(a);
    }

    function runFact(uint256 a) external pure returns (uint256) {
        return fact(a);
    }

    function runTail(uint256 a) external pure returns (uint256) {
        return tailOuter(a);
    }

    function runCountdown(uint256 a) external pure returns (uint256) {
        return countdown(a, 0);
    }

    function runAsm(uint256 a) external pure returns (uint256 r) {
        assembly {
            function twice(x) -> y {
                y := add(x, x)
            }
            function viaTwice(x) -> y {
                y := twice(x)
            }
            r := viaTwice(a)
        }
    }
}
