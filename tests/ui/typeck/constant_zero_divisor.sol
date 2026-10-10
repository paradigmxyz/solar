// ported-from: test/libsolidity/syntaxTests/constants/initialization/addmod_by_zero.sol
// ported-from: test/libsolidity/syntaxTests/constants/initialization/division_by_zero.sol
// ported-from: test/libsolidity/syntaxTests/constants/initialization/modulo_by_zero.sol
// ported-from: test/libsolidity/syntaxTests/constants/initialization/mulmod_by_zero.sol

contract AddMod {
    uint constant a1 = 0;
    uint constant a2 = 1;
    uint constant b1 = addmod(3, 4, 0); //~ ERROR: arithmetic modulo zero
    uint constant b2 = addmod(3, 4, a1); //~ ERROR: arithmetic modulo zero
    uint constant b3 = addmod(3, 4, a2 - 1); //~ ERROR: arithmetic modulo zero
}

contract Division {
    uint constant a1 = 0;
    uint constant a2 = 1;
    uint constant b1 = 7 / a1; //~ ERROR: division by zero
    uint constant b2 = 7 / (a2 - 1); //~ ERROR: division by zero
}

contract Modulo {
    uint constant a1 = 0;
    uint constant a2 = 1;
    uint constant b1 = 3 % a1; //~ ERROR: modulo zero
    uint constant b2 = 3 % (a2 - 1); //~ ERROR: modulo zero
}

contract MulMod {
    uint constant a1 = 0;
    uint constant a2 = 1;
    uint constant b1 = mulmod(3, 4, 0); //~ ERROR: arithmetic modulo zero
    uint constant b2 = mulmod(3, 4, a1); //~ ERROR: arithmetic modulo zero
    uint constant b3 = mulmod(3, 4, a2 - 1); //~ ERROR: arithmetic modulo zero
}
