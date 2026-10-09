// ported-from: test/libsolidity/syntaxTests/literalOperations/division_by_zero.sol
// ported-from: test/libsolidity/syntaxTests/literalOperations/division_by_zero_complex.sol
// ported-from: test/libsolidity/syntaxTests/literalOperations/division_by_zero_complex_compound.sol
// ported-from: test/libsolidity/syntaxTests/literalOperations/division_by_zero_compound.sol
// ported-from: test/libsolidity/syntaxTests/literalOperations/division_by_zero_nonliteral.sol
// ported-from: test/libsolidity/syntaxTests/literalOperations/mod_zero.sol
// ported-from: test/libsolidity/syntaxTests/literalOperations/mod_zero_complex.sol
// ported-from: test/libsolidity/syntaxTests/literalOperations/mod_zero_complex_compound.sol
// ported-from: test/libsolidity/syntaxTests/literalOperations/mod_zero_compound.sol
// ported-from: test/libsolidity/syntaxTests/literalOperations/mod_zero_nonliteral.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/540_array_length_invalid_expression_division_by_zero.sol

contract DivisionByZero {
    uint constant a = 1 / 0; //~ ERROR: failed to evaluate constant: attempted to divide by zero
}

contract DivisionByZeroComplex {
    uint constant a = 1 / ((1+3)-4); //~ ERROR: failed to evaluate constant: attempted to divide by zero
}

contract DivisionByZeroComplexCompound {
    uint a;
    constructor() { a /= (((2)*2)%4); }
}

contract DivisionByZeroCompound {
    uint a = 5;
    constructor() { a /= uint(0); }
}

contract DivisionByZeroNonliteral {
    constructor() { uint a; a / 0; }
}

contract ModZero {
    uint constant b3 = 1 % 0; //~ ERROR: failed to evaluate constant: attempted to divide by zero
}

contract ModZeroComplex {
    uint constant b3 = 1 % (-4+((2)*2)); //~ ERROR: failed to evaluate constant: attempted to divide by zero
}

contract ModZeroComplexCompound {
    uint a = 5;
    constructor() { a %= uint(((2)*2)%4); }
}

contract ModZeroCompound {
    uint a;
    constructor() { a = 5; a %= 0; }
}

contract ModZeroNonliteral {
    constructor() { uint a; a % 0; }
}

contract ArrayLength {
    uint[3/0] ids; //~ ERROR: failed to evaluate constant: attempted to divide by zero
}

// Fractions cannot be divided by zero either.
contract Fractions {
    uint constant c = 0.5 / 0; //~ ERROR: failed to evaluate constant: attempted to divide by zero
    uint constant d = 0.5 % (0.5 - 0.5); //~ ERROR: failed to evaluate constant: attempted to divide by zero
    uint[c] lengths; //~ ERROR: failed to evaluate constant: attempted to divide by zero
    function f() public pure returns (uint) {
        return (1 / 0) + 1; //~ ERROR: failed to evaluate constant: attempted to divide by zero
    }
}
