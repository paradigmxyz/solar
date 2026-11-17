//@ revisions: none size
//@[none] compile-flags: -Onone -Zdump=disasm-runtime
//@[size] compile-flags: -Osize -Zdump=disasm-runtime
//@[size] filecheck: --check-prefix=SIZE
//@ normalize-stdout-test: "(?s).+" -> ""

// A function that only forwards to another keeps its entry in an unoptimized build: callers enter
// it at a `JUMPDEST` and it jumps on, as the `JUMPDEST` of every function does in solc. The size
// build sends the callers to the callee.

// CHECK-LABEL: (runtime) ===
// CHECK: ; bb{{[0-9]+}}
// CHECK-NEXT: JUMPDEST
// CHECK-NEXT: PUSH{{[0-9]}} {{0x[0-9a-f]+}} ; bb{{[0-9]+}}
// CHECK-NEXT: JUMP

// SIZE-LABEL: (runtime) ===
// SIZE-NOT: JUMPDEST{{[[:space:]]+}}PUSH{{[0-9]}} {{0x[0-9a-f]+}} ; bb{{[0-9]+}}{{[[:space:]]+}}JUMP{{[[:space:]]}}
library Impl {
    function indexOf(uint256 a, uint256 b) internal pure returns (uint256) {
        return a + b;
    }
}

library Wrap {
    function indexOf(uint256 a, uint256 b) internal pure returns (uint256) {
        return Impl.indexOf(a, b);
    }
}

contract C {
    function f(uint256 x, uint256 y) external pure returns (uint256) {
        return Wrap.indexOf(x, y);
    }
}
