// A shift or exponentiation whose left operand is a literal and whose right
// operand is typed is performed in the literal's mobile type, which for these
// operators is `uint256` for a non-negative literal and `int256` for a
// negative one. Evaluating them at full precision instead would keep a 257-bit
// intermediate and fold `(1 << SHIFT) >> SHIFT` to `1`, where the EVM shifts
// the bit out. solc reports the out-of-range intermediates as "Arithmetic
// error when computing constant value".
contract C {
    uint16 constant S255 = 255;
    uint16 constant S256 = 256;
    uint8 constant S8 = 8;

    function tooWide() public pure {
        uint256[1 << S256] memory a; //~ ERROR: failed to evaluate constant: arithmetic overflow
        uint256[(1 << S256) >> S256] memory b; //~ ERROR: failed to evaluate constant: arithmetic overflow
        uint256[2 ** S256] memory c; //~ ERROR: failed to evaluate constant: arithmetic overflow
        a;
        b;
        c;
    }

    function inRange() public pure {
        uint256[(1 << S8) >> S8] memory a;
        uint256[3 ** S8] memory b;
        uint256[(1 << S255) >> S255] memory c;
        a[0] = 1;
        b[0] = 2;
        c[0] = 3;
    }
}
