//@ check-pass
//@compile-flags: -Zcodegen -Zdump=mir

// Pins the checked exponentiation shapes ported from solc's `checked_exp_*`
// Yul helpers:
// - unsigned: trivial bases (0, 1) and base 2 short-circuit; bases below
//   11/307 with exponents below 78/32 use native `EXP` (exact, no wrap);
//   everything else runs the squaring loop with one `base * base` overflow
//   check per iteration and a checked final multiply.
// - signed: exponent 0/1 and bases 0/1/-1 short-circuit; the first squaring
//   is pulled out (only iteration with a possibly negative base); positive
//   bases reuse the small-base native `EXP` path with a range check; the
//   final multiply is bounds-checked per sign.
// - constant full-width bases compile to a single exponent bound check plus
//   native `EXP` (`SHL` for base 2).
// - unchecked exponentiation stays native `EXP` (masked for sub-word types).
contract CheckedPowShapes {
    function upow(uint256 a, uint256 b) public pure returns (uint256) {
        return a ** b;
    }

    function spow(int256 a, uint256 b) public pure returns (int256) {
        return a ** b;
    }

    function upow8(uint8 a, uint8 b) public pure returns (uint8) {
        return a ** b;
    }

    function spow8(int8 a, uint8 b) public pure returns (int8) {
        return a ** b;
    }

    function const2(uint256 b) public pure returns (uint256) {
        return 2 ** b;
    }

    function const10(uint256 b) public pure returns (uint256) {
        return 10 ** b;
    }

    function const_neg2(uint256 b) public pure returns (int256) {
        return (-2) ** b;
    }

    function unchecked_pow8(uint8 a, uint8 b) public pure returns (uint8) {
        unchecked {
            return a ** b;
        }
    }
}
