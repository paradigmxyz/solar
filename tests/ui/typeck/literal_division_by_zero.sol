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

contract DivisionByZero {
    uint constant a = 1 / 0; //~ ERROR: cannot apply builtin operator `/`
}

contract DivisionByZeroComplex {
    uint constant a = 1 / ((1+3)-4); //~ ERROR: cannot apply builtin operator `/`
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
    uint constant b3 = 1 % 0; //~ ERROR: cannot apply builtin operator `%`
}

contract ModZeroComplex {
    uint constant b3 = 1 % (-4+((2)*2)); //~ ERROR: cannot apply builtin operator `%`
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
