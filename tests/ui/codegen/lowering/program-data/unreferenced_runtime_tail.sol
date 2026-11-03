//@ compile-flags: -Ogas -Zdump=evm-ir
//@ filecheck:

// A constructor that always reverts has no `CODECOPY` of its runtime code, so nothing references
// the last entry of creation code, which `pack-data` must drop with the other unreferenced data.
// CHECK-LABEL: unreferenced_runtime_tail.sol:Reverts (creation)
// CHECK: revert
contract Reverts {
    constructor() {
        revert("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef-this string is long enough for a copy-fedcba9876543210fedcba9876543210");
    }

    function f() external {}
}
