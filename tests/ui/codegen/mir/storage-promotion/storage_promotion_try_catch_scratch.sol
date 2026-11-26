//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// The compiler's copy of revert data to scratch memory in an internal `try` function names a range of
// unknown length at address zero. It does not stop promotion of a loop in another function.

// CHECK-LABEL: fn @bump
// CHECK-NOT: sload
// CHECK: mload
interface Callee {
    function ping() external;
}

contract StoragePromotionTryCatchScratch {
    uint256 public x;

    function bump(uint256 n) external returns (uint256) {
        x = 1;
        for (uint256 i; i < n; ++i) x += i;
        return x;
    }

    function guarded(Callee c) external returns (bytes memory) {
        return _guarded(c);
    }

    function _guarded(Callee c) internal returns (bytes memory r) {
        try c.ping() {} catch (bytes memory reason) {
            r = reason;
        }
    }
}
