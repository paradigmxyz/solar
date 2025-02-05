uint constant x = (69 + (((420))));

uint constant rec1 = rec1;
uint constant rec2 = rec1;

uint constant bigLiteral = 115792089237316195423570985008687907853269984665640564039457584007913129639935;
uint constant tooBigLiteral = 115792089237316195423570985008687907853269984665640564039457584007913129639936;

contract C {
    uint constant zero = x - x;
    uint public constant zeroPublic = x / x - 1;
    uint[zero] public zeroArray; //~ ERROR: array length must be greater than zero
    uint[zeroPublic + 1] public oneArray;

    uint[bigLiteral] public big;
    uint[bigLiteral + 1] public tooBig1; //~ ERROR: evaluation of constant value failed
    uint[tooBigLiteral] public tooBig2; //~ ERROR: evaluation of constant value failed

    uint private stateVar = 69;
    uint public stateVarPublic = 420;

    function a(uint[x / 0] memory) public {} //~ ERROR: evaluation of constant value failed
    function b(uint[x] memory) public {}
    function c(uint[x * 2] memory) public {}
    function d(uint[0 - 1] memory) public {} //~ ERROR: evaluation of constant value failed
    function e(uint[rec1] memory) public {} //~ ERROR: evaluation of constant value failed
    function f(uint[rec2] memory) public {} //~ ERROR: evaluation of constant value failed

    function g(uint[0] memory) public {} //~ ERROR: array length must be greater than zero
    function h(uint[zero] memory) public {} //~ ERROR: array length must be greater than zero
    function h2(uint[zeroPublic] memory) public {} //~ ERROR: array length must be greater than zero

    function i(uint[block.timestamp] memory) public {} //~ ERROR: evaluation of constant value failed
    function j(uint["lol"] memory) public {} //~ ERROR: evaluation of constant value failed
    function k(uint[--x] memory) public {} //~ ERROR: evaluation of constant value failed
    function l(uint[stateVar] memory) public {} //~ ERROR: evaluation of constant value failed
    function m(uint[stateVarPublic] memory) public {} //~ ERROR: evaluation of constant value failed
}
