//@ compile-flags: -Ogas --optimize-runs 10000 -Zdump=mir
//@ filecheck:
//@ run-call: f 0x01020301020101010101, 1, 2, 3 => 5, 2, 1
//@ run-call: f 0x0000000000000000, 1, 2, 3 => 0, 0, 0

// A helper's byte scan carries more than four words and runs to a parameter bound, which its only
// caller passes as a literal. The run before inlining and specialization keeps the counter's
// test, so the loop unrolls completely once the bound is a literal.
// CHECK-LABEL: fn @count
// CHECK-NOT: phi
// CHECK-COUNT-8: mload
// CHECK-NOT: phi
// CHECK: ret

contract ParameterBound {
    function count(bytes memory data, uint256 n, uint256 a, uint256 b, uint256 c)
        internal
        pure
        returns (uint256 x, uint256 y, uint256 z)
    {
        uint256 p;
        assembly {
            p := add(data, 32)
        }
        for (uint256 i = 0; i < n;) {
            uint256 d;
            assembly {
                d := byte(0, mload(add(p, i)))
            }
            unchecked {
                if (d == a) x++;
                if (d == b) y++;
                if (d == c) z++;
                ++i;
            }
        }
    }

    function f(bytes memory data, uint256 a, uint256 b, uint256 c)
        external
        pure
        returns (uint256, uint256, uint256)
    {
        return count(data, 8, a, b, c);
    }
}
