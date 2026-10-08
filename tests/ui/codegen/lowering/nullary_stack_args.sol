//@ compile-flags: -O gas -Zdump=evm-ir-runtime -Zlegacy-stack-lowering
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// A cheap environment read passed to an internal call is re-emitted right
// below the jump as a stack argument. Storing it to the callee's frame first
// would add a memory write at every call site.
contract NullaryStackArgs {
    uint256 internal total;

    // CHECK-LABEL: (runtime) ===
    // CHECK: callvalue
    // CHECK-NEXT: jump bb{{[0-9]+}}
    // CHECK: callvalue
    // CHECK-NEXT: jump bb{{[0-9]+}}
    // CHECK: callvalue
    // CHECK-NEXT: jump bb{{[0-9]+}}
    function f(uint256 x) external payable returns (uint256) {
        uint256 a = record(msg.value, x);
        uint256 b = record(msg.value, a);
        return record(msg.value, b);
    }

    function record(uint256 value, uint256 x) internal returns (uint256) {
        total += value;
        for (uint256 i = 0; i < x; i++) {
            total += i * value;
        }
        return total + x;
    }
}
