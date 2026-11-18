//@ compile-flags: -Onone --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ normalize-stdout-test: "(?s).+" -> ""
//@ filecheck:

// A `return` or `leave` that follows a call as a statement of its own does not enclose the call.
// The tail call's argument setup maps to the call, and the callee push and jump that leave the
// function map to that statement, as the jump of a plain `return` does. The spans of the jumps are the offsets of
// `return;` in `g` and of `leave` in `z`. When layout falls the tail call through into its callee,
// no jump is left and the statement has no entry, as a statement that emits no code has none.

// CHECK-LABEL: (scheduled runtime) ===
// CHECK: [invoke=g
// CHECK-NEXT: push 1 !metadata(span=[[G:[0-9]+..[0-9]+]])
// CHECK-NEXT: swap 2 !metadata(span=[[G]])
// CHECK-NEXT: swap 1 !metadata(span=[[G]])
// CHECK-NEXT: jump bb{{[0-9]+}} !metadata(span=1665..1672, exit=return)

// The implicit return that ends a function encloses the call, so the jump maps to the call.
// CHECK: [invoke=i
// CHECK-NEXT: push 2 !metadata(span=[[I:[0-9]+..[0-9]+]])
// CHECK-NEXT: swap 2 !metadata(span=[[I]])
// CHECK-NEXT: swap 1 !metadata(span=[[I]])
// CHECK-NEXT: jump bb{{[0-9]+}} !metadata(span=[[I]], exit=return)

// CHECK: [invoke=z
// CHECK-NEXT: push 1 !metadata(span=[[Z:[0-9]+..[0-9]+]])
// CHECK-NEXT: swap 2 !metadata(span=[[Z]])
// CHECK-NEXT: swap 1 !metadata(span=[[Z]])
// CHECK-NEXT: jump bb{{[0-9]+}} !metadata(span=1947..1952, exit=return)
contract C {
    uint256 public s;

    function f(uint256 a, uint256 b) internal {
        s = a + b;
    }

    function g(uint256 x) internal {
        f(x, 1);
        return;
    }

    function i(uint256 x) internal {
        f(x, 2);
    }

    function j(uint256 x) internal {
        assembly {
            function y(a, b) {
                sstore(0, add(a, b))
            }
            function z(a) {
                y(a, 1)
                leave
            }
            z(x)
        }
    }

    function a(uint256 y) external {
        g(y);
        i(y);
        j(y);
    }
}
