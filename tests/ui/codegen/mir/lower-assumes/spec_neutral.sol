//@ revisions: gas size
//@[gas] compile-flags: -Ogas --emit=bin-runtime
//@[size] compile-flags: -Osize --emit=bin-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// Specifications never change the code the compiler emits: without
// `-Zcheck-analyses` or `-Zprove-analyses` no clause is lowered, so the same
// contract with and without its tags compiles to the same bytecode.

// CHECK: "{{[^"]*}}:WithSpecs":{"bin-runtime":"[[CODE:[0-9a-f]+]]"}
// CHECK-SAME: "{{[^"]*}}:WithoutSpecs":{"bin-runtime":"[[CODE]]"}
contract WithSpecs {
    /// @custom:requires n <= 32 && a != 0
    /// @custom:ensures s >= a || n == 0
    function sum(uint8 a, uint256 n) external pure returns (uint256 s) {
        for (uint256 i = 0; i < n; i++) {
            s += a;
        }
    }

    /// @custom:ensures r >= x && (r == x || r == y)
    function max(int256 x, int256 y) external pure returns (int256 r) {
        if (x > y) return x;
        return y;
    }
}

contract WithoutSpecs {
    function sum(uint8 a, uint256 n) external pure returns (uint256 s) {
        for (uint256 i = 0; i < n; i++) {
            s += a;
        }
    }

    function max(int256 x, int256 y) external pure returns (int256 r) {
        if (x > y) return x;
        return y;
    }
}
