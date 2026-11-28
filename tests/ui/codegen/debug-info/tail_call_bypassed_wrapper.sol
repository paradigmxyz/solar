//@ compile-flags: -Ogas --emit=ethdebug-runtime --pretty-json
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// `w` only tail calls `big`, and `big` calls `w` back, so neither is inlined. CFG simplification
// bypasses `w`: the callers jump to `big`, and `w`'s return does not move onto those jumps, which
// call a function and do not leave their own. No jump is a tail call.
// CHECK-LABEL: "ethdebug-runtime"
// CHECK-NOT: "tailcall"
contract C {
    function big(uint256 x) internal pure returns (uint256) {
        if (x < 2) return 1;
        uint256 r = x;
        for (uint256 i = 0; i < x % 5; i++) r = r * 31 + i;
        return r % 1000003 + w(x - 2);
    }

    function w(uint256 x) internal pure returns (uint256) {
        return big(x);
    }

    function a(uint256 x) external pure returns (uint256) {
        return w(x) + 1;
    }

    function b(uint256 x) external pure returns (uint256) {
        return w(x + 1) ^ 5;
    }

    function c(uint256 x) external pure returns (uint256) {
        return w(x + 2) * 3;
    }
}
