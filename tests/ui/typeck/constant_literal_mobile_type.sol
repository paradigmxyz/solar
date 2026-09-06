// A literal paired with a typed operand leaves the rationals through its
// mobile type, which is the narrowest integer type of its sign holding it, and
// the operation is performed in the common type of that and the typed operand.
// `65535 + ONE` with a `uint8` `ONE` is therefore `uint16` arithmetic that
// overflows, where the full-width mobile type folded it to 65536; solc reports
// "Arithmetic error when computing constant value" for it.
//
// Shifts and exponentiation with a literal left operand keep their own rule:
// they are performed in `uint256`, or `int256` for a negative literal, rather
// than in the literal's mobile type.
contract C {
    uint8 constant ONE = 8;
    uint8 constant WIDE = 250;
    uint16 constant U16 = 1;
    int8 constant I8 = -1;
    int16 constant J16 = 32767;

    function mobileType() public pure {
        uint256[255 + ONE] memory addUint8; //~ ERROR: failed to evaluate constant: arithmetic overflow
        uint256[65535 + ONE] memory addUint16; //~ ERROR: failed to evaluate constant: arithmetic overflow
        uint256[200 * ONE] memory mulUint8; //~ ERROR: failed to evaluate constant: arithmetic overflow
        uint256[65535 + U16] memory addTyped; //~ ERROR: failed to evaluate constant: arithmetic overflow
        uint256[100 + J16] memory addSigned; //~ ERROR: failed to evaluate constant: arithmetic overflow
        uint256[-32768 * I8] memory mulInt16; //~ ERROR: failed to evaluate constant: arithmetic overflow
        addUint8;
        addUint16;
        mulUint8;
        addTyped;
        addSigned;
        mulInt16;
    }

    // A shift or exponentiation with a literal left operand is performed in a
    // word, so it still overflows when the result does not fit one.
    function wordOperations() public pure {
        uint256[200 << WIDE] memory shl; //~ ERROR: failed to evaluate constant: arithmetic overflow
        uint256[3 ** WIDE] memory pow; //~ ERROR: failed to evaluate constant: arithmetic overflow
        shl;
        pow;
    }

    function inRange() public pure {
        uint256[256 + ONE] memory add;
        uint256[65534 + U16] memory addTyped;
        uint256[-127 * I8] memory mulInt8;
        uint256[2 ** ONE] memory pow;
        uint256[1 << ONE] memory shl;
        uint256[200 << ONE] memory shlWide;
        add[0] = 1;
        addTyped[0] = 2;
        mulInt8[0] = 3;
        pow[0] = 4;
        shl[0] = 5;
        shlWide[0] = 6;
    }
}
