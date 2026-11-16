//@ revisions: none size
//@[none] compile-flags: -Onone -Zdump=evm-ir-runtime
//@[size] compile-flags: -Osize -Zdump=evm-ir-runtime
//@[size] filecheck: --check-prefix=SIZE
//@ normalize-stdout-test: "(?s).+" -> ""

// An unoptimized build keeps the entry of a function that its only caller falls into, so the
// function keeps a label and a `JUMPDEST`. The size build merges it into the caller.

// CHECK-LABEL: (runtime) ===
// CHECK: [ret=0, inputs=2, outputs=1]:{{.*}}function h(

// SIZE-LABEL: (runtime) ===
// SIZE-NOT: [ret=0, inputs=2, outputs=1]
contract C {
    uint256 a;

    function g(uint256 x) external returns (uint256) {
        return h(x);
    }

    function h(uint256 x) internal returns (uint256) {
        a = x;
        return a + x;
    }
}
