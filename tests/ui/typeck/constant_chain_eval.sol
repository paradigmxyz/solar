// Constant evaluation reads each constant once, so a chain of constants that each use the
// previous one twice takes linear time instead of exponential.

contract C {
    uint256 constant Z0 = 0;
    uint256 constant Z1 = Z0 + Z0;
    uint256 constant Z2 = Z1 + Z1;
    uint256 constant Z3 = Z2 + Z2;
    uint256 constant Z4 = Z3 + Z3;
    uint256 constant Z5 = Z4 + Z4;
    uint256 constant Z6 = Z5 + Z5;
    uint256 constant Z7 = Z6 + Z6;
    uint256 constant Z8 = Z7 + Z7;
    uint256 constant Z9 = Z8 + Z8;
    uint256 constant Z10 = Z9 + Z9;
    uint256 constant Z11 = Z10 + Z10;
    uint256 constant Z12 = Z11 + Z11;
    uint256 constant Z13 = Z12 + Z12;
    uint256 constant Z14 = Z13 + Z13;
    uint256 constant Z15 = Z14 + Z14;
    uint256 constant Z16 = Z15 + Z15;
    uint256 constant Z17 = Z16 + Z16;
    uint256 constant Z18 = Z17 + Z17;
    uint256 constant Z19 = Z18 + Z18;
    uint256 constant Z20 = Z19 + Z19;
    uint256 constant Z21 = Z20 + Z20;
    uint256 constant Z22 = Z21 + Z21;
    uint256 constant Z23 = Z22 + Z22;
    uint256 constant Z24 = Z23 + Z23;

    uint256[Z24 + 1] x;

    function add(uint256 a, uint256 b) public pure returns (uint256) {
        return addmod(a, b, Z24); //~ ERROR: arithmetic modulo zero
    }
}
