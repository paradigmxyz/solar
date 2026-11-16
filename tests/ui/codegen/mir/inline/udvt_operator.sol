//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir-final
//@[dump] filecheck:
//@ run-call: Operator::run 1, 3, 1, 2, 4, 8, 16, 32 => 65
//@ run-call: Unwrapped::run 1, 3, 1, 2, 4, 8, 16, 32 => 65

// The loop keeps more words alive across the comparison than the inline stack budget allows.
// The user-defined `==` is a straight-line helper, whose clone holds one word fewer than its
// call, so it inlines and both contracts compile to the same code.
type Ratio is uint96;

using {eq as ==} for Ratio global;

function eq(Ratio a, Ratio b) pure returns (bool) {
    return Ratio.unwrap(a) == Ratio.unwrap(b);
}

// CHECK-LABEL: @module Operator
// CHECK-NOT: icall
// CHECK: eq v{{[0-9]+}}, v0
contract Operator {
    function run(Ratio r, Ratio limit, uint256 a, uint256 b, uint256 c, uint256 d, uint256 e, uint256 f)
        external
        pure
        returns (uint256 s)
    {
        unchecked {
            uint256 t;
            uint256 u;
            uint256 v;
            uint256 w;
            uint256 x;
            while (true) {
                s += a;
                t += b;
                u += c;
                v += d;
                w += e;
                x += f;
                if (r == limit) break;
                r = Ratio.wrap(Ratio.unwrap(r) + 1);
            }
            return s ^ t ^ u ^ v ^ w ^ x;
        }
    }
}

// CHECK-LABEL: @module Unwrapped
// CHECK-NOT: icall
// CHECK: eq v{{[0-9]+}}, v0
contract Unwrapped {
    function run(Ratio r, Ratio limit, uint256 a, uint256 b, uint256 c, uint256 d, uint256 e, uint256 f)
        external
        pure
        returns (uint256 s)
    {
        unchecked {
            uint256 t;
            uint256 u;
            uint256 v;
            uint256 w;
            uint256 x;
            while (true) {
                s += a;
                t += b;
                u += c;
                v += d;
                w += e;
                x += f;
                if (Ratio.unwrap(r) == Ratio.unwrap(limit)) break;
                r = Ratio.wrap(Ratio.unwrap(r) + 1);
            }
            return s ^ t ^ u ^ v ^ w ^ x;
        }
    }
}
