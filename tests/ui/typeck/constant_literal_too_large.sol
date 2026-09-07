// A typed operand makes an untyped literal leave the rationals: the literal
// gets its mobile type, and a literal too large for the full-width type of its
// sign has no mobile type at all. The operands must be checked before the
// operation, because folding only retypes the result: `(1 << 256) >> ONE`
// shifts the literal back into range and would otherwise be accepted, where
// solc rejects it as "Literal too large" and the runtime expression yields 0.
// The same check applies to every other binary operator, where the literal
// reaches the operation through its mobile type as well.
contract C {
    uint256 constant ONE = 1;
    uint8 constant S8 = 8;
    uint16 constant U16 = 1;

    uint256 constant SHR = (1 << 256) >> ONE;
    uint256 constant ADD = (1 << 256) + S8;

    function lengths() public pure {
        uint256[(1 << 256) >> ONE] memory shr; //~ ERROR: failed to evaluate constant: literal is too large for the type of the other operand
        uint256[(1 << 256) ** ONE] memory pow; //~ ERROR: failed to evaluate constant: literal is too large for the type of the other operand
        uint256[(0 - (1 << 256)) >> ONE] memory negativeShr; //~ ERROR: failed to evaluate constant: literal is too large for the type of the other operand
        uint256[ONE << (1 << 256)] memory shlAmount; //~ ERROR: failed to evaluate constant: literal is too large for the type of the other operand
        uint256[(1 << 256) + S8] memory add; //~ ERROR: failed to evaluate constant: literal is too large for the type of the other operand
        uint256[(1 << 256) - S8] memory sub; //~ ERROR: failed to evaluate constant: literal is too large for the type of the other operand
        uint256[(1 << 256) / S8] memory div; //~ ERROR: failed to evaluate constant: literal is too large for the type of the other operand
        shr;
        pow;
        negativeShr;
        shlAmount;
        add;
        sub;
        div;
    }

    function initializers() public pure {
        uint256[SHR] memory shr; //~ ERROR: failed to evaluate constant: literal is too large for the type of the other operand
        uint256[ADD] memory add; //~ ERROR: failed to evaluate constant: literal is too large for the type of the other operand
        shr;
        add;
    }

    // Two literals keep full precision, and a literal that has a mobile type
    // still combines with a typed operand even when it does not fit that
    // operand's own type.
    function inRange() public pure {
        uint256[(1 << 256) >> 256] memory wide;
        uint256[300 - S8] memory sub;
        uint256[1 << ONE] memory shl;
        uint256[256 + S8] memory add;
        uint256[65534 + U16] memory u16;
        wide[0] = 1;
        sub[0] = 2;
        shl[0] = 3;
        add[0] = 4;
        u16[0] = 5;
    }
}
