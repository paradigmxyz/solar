//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call-fail: structConstructor 0 => 0x
//@ run-call-fail: structConstructor 1 => 0x
//@ run-call-fail: internalCall 0 => 0x
//@ run-call-fail: internalCall 1 => 0x
//@ run-call-fail: tuple 0 => 0x
//@ run-call-fail: tuple 1 => 0x

// A calldata slice that assembly moved out of range reverts with empty data when it is copied to
// memory. The copy happens as soon as the argument is evaluated, before `x - 1`, so `x == 0`
// reverts empty here; solc evaluates every argument first and reverts with `Panic(0x11)`. See
// CODEGEN-033 in docs/SOLC_DIVERGENCE.md.
contract C {
    struct S {
        bytes a;
        uint256 b;
    }

    // CHECK-LABEL: fn @structConstructor(
    function structConstructor(uint256 x) external pure returns (uint256) {
        bytes calldata s;
        assembly {
            s.offset := 0xffffff
            s.length := 0
        }
        S memory m = S({a: s, b: x - 1});
        return m.b;
    }

    // CHECK-LABEL: fn @internalCall(
    function internalCall(uint256 x) external pure returns (uint256) {
        bytes calldata s;
        assembly {
            s.offset := 0xffffff
            s.length := 0
        }
        return take(s, x - 1);
    }

    // CHECK-LABEL: fn @tuple(
    function tuple(uint256 x) external pure returns (uint256) {
        bytes calldata s;
        assembly {
            s.offset := 0xffffff
            s.length := 0
        }
        (bytes memory m, uint256 y) = (s, x - 1);
        return y + m.length;
    }

    function take(bytes memory, uint256 y) internal pure returns (uint256) {
        return y;
    }
}
