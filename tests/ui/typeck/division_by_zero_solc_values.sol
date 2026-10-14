// The division by zero (1211) and modulo zero (4195) checks only apply to values solc's constant
// evaluator computes. It does not follow member accesses, converts the exponent of a typed power
// to the base type, and follows at most 32 nested constants, where it reports error 5210 instead;
// see TYPECK-020.
library L {
    uint constant Z = 0;
}

uint constant ZERO = 0;
uint8 constant Z8 = 0;
uint constant D0 = 0;
uint constant D1 = D0;
uint constant D2 = D1;
uint constant D3 = D2;
uint constant D4 = D3;
uint constant D5 = D4;
uint constant D6 = D5;
uint constant D7 = D6;
uint constant D8 = D7;
uint constant D9 = D8;
uint constant D10 = D9;
uint constant D11 = D10;
uint constant D12 = D11;
uint constant D13 = D12;
uint constant D14 = D13;
uint constant D15 = D14;
uint constant D16 = D15;
uint constant D17 = D16;
uint constant D18 = D17;
uint constant D19 = D18;
uint constant D20 = D19;
uint constant D21 = D20;
uint constant D22 = D21;
uint constant D23 = D22;
uint constant D24 = D23;
uint constant D25 = D24;
uint constant D26 = D25;
uint constant D27 = D26;
uint constant D28 = D27;
uint constant D29 = D28;
uint constant D30 = D29;
uint constant D31 = D30;
uint constant D32 = D31;

contract C {
    function member() public pure returns (uint, uint) {
        return (1 / L.Z, addmod(1, 2, L.Z));
    }

    function power() public pure returns (uint) {
        return 1 / (Z8 ** 256);
    }

    function deep() public pure returns (uint) {
        return 1 / D32;
    }

    function shallow() public pure returns (uint, uint) {
        return (1 / D31, mulmod(1, 2, ZERO)); //~ ERROR: division by zero
        //~^ ERROR: arithmetic modulo zero
    }
}
