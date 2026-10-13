//@ codegen-matrix: standard
//@ run-call: swap 3, 1, 2 => 2, 1
//@ run-call: swap 4, 1, 2 => 1, 2
//@ run-call: rotate 1, 1, 2, 3 => 2, 3, 1
//@ run-call: rotate 2, 1, 2, 3 => 3, 1, 2
//@ run-call: swapSpilled 3, 1, 2 => 0x48
//@ run-call: rotateSpilled 2, 1, 2, 3 => 0x33d

// Loop-carried values that swap or rotate form cycles of phi copies. Each copy
// must read its source before another copy in the cycle overwrites it.
contract PhiCopyCycles {
    function swap(uint256 n, uint256 a, uint256 b) external pure returns (uint256, uint256) {
        for (uint256 i = 0; i < n; i++) {
            (a, b) = (b, a);
        }
        return (a, b);
    }

    function rotate(uint256 n, uint256 a, uint256 b, uint256 c)
        external
        pure
        returns (uint256, uint256, uint256)
    {
        for (uint256 i = 0; i < n; i++) {
            (a, b, c) = (b, c, a);
        }
        return (a, b, c);
    }

    function swapSpilled(uint256 n, uint256 a, uint256 b) external pure returns (uint256) {
        unchecked {
            uint256 x0 = a + 1;
            uint256 x1 = b + 2;
            uint256 x2 = a * 3;
            uint256 x3 = b * 5;
            uint256 x4 = a ^ 7;
            uint256 x5 = b ^ 11;
            uint256 x6 = a + b;
            uint256 x7 = a * b;
            for (uint256 i = 0; i < n; i++) {
                (a, b) = (b, a);
            }
            return a * 16 + b + x0 + x1 + x2 + x3 + x4 + x5 + x6 + x7;
        }
    }

    function rotateSpilled(uint256 n, uint256 a, uint256 b, uint256 c)
        external
        pure
        returns (uint256)
    {
        unchecked {
            uint256 x0 = a + 1;
            uint256 x1 = b + 2;
            uint256 x2 = c * 3;
            uint256 x3 = a * 5;
            uint256 x4 = b ^ 7;
            uint256 x5 = c ^ 11;
            uint256 x6 = a + c;
            uint256 x7 = b * c;
            for (uint256 i = 0; i < n; i++) {
                (a, b, c) = (b, c, a);
            }
            return a * 256 + b * 16 + c + x0 + x1 + x2 + x3 + x4 + x5 + x6 + x7;
        }
    }
}
