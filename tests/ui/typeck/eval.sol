uint constant x = (69 + (((420))));

uint constant rec1 = rec1;
uint constant rec2 = rec1;

uint constant bigLiteral = 115792089237316195423570985008687907853269984665640564039457584007913129639935;

uint constant fails = 0 / 0;

contract C {
    uint constant zero = x - x;
    uint public constant zeroPublic = x / x - 1;
    uint[zero] public zeroArray; //~ ERROR: array length must be greater than zero
    uint[zeroPublic + 1] public oneArray;

    uint[bigLiteral] public big;
    uint[bigLiteral + 1] public tooBig1; //~ ERROR: failed to evaluate constant: arithmetic overflow

    uint private stateVar = 69;
    uint public stateVarPublic = 420;

    function early(uint[fails] memory) public {} //~ ERROR: failed to evaluate constant: attempted to divide by zero

    function a(uint[x / 0] memory) public {} //~ ERROR: failed to evaluate constant: attempted to divide by zero
    function a2(uint[x / zeroPublic] memory) public {} //~ ERROR: failed to evaluate constant: attempted to divide by zero
    function b(uint[x] memory) public {}
    function c(uint[x * 2] memory) public {}
    function d(uint[0 - 1] memory) public {} //~ ERROR: failed to evaluate constant: arithmetic overflow
    function d2(uint[zeroPublic - 1] memory) public {} //~ ERROR: failed to evaluate constant: arithmetic overflow
    function e(uint[rec1] memory) public {} //~ ERROR: failed to evaluate constant: recursion limit reached
    function f(uint[rec2] memory) public {} //~ ERROR: failed to evaluate constant: recursion limit reached

    function g(uint[0] memory) public {} //~ ERROR: array length must be greater than zero
    function h(uint[zero] memory) public {} //~ ERROR: array length must be greater than zero
    function h2(uint[zeroPublic] memory) public {} //~ ERROR: array length must be greater than zero

    function i(uint[block.timestamp] memory) public {} //~ ERROR: failed to evaluate constant: unsupported expression
    function j(uint["lol"] memory) public {} //~ ERROR: failed to evaluate constant: unsupported literal
    function k(uint[--x] memory) public {} //~ ERROR: failed to evaluate constant: unsupported unary operation
    function l(uint[stateVar] memory) public {} //~ ERROR: failed to evaluate constant: only constant variables are allowed
    function m(uint[stateVarPublic] memory) public {} //~ ERROR: failed to evaluate constant: only constant variables are allowed
}
