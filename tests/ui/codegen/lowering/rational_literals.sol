//@ codegen-matrix: standard
//@ run-call: fee() => 5000000000000000
//@ run-call: directFee() => 5000000000000000
//@ run-call: signedTyped() => -6
//@ run-call: tiny() => 1
//@ run-call: seven() => 7
//@ run-call: decimal() => 5
//@ run-call: negative() => -7
//@ run-call: remainder() => 1
//@ run-call: inverse() => 4
//@ run-call: wide() => 57896044618658097711785492504343953926634992332820282019728792003956564819968
//@ run-call: max() => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: typed() => 6
//@ run-call: runtimeDivision 7 => 6
//@ run-call: compare() => true

contract RationalLiterals {
    uint constant SEVEN = 7;
    uint constant TWO = 2;
    int constant NEGATIVE_SEVEN = -7;
    int constant SIGNED_TWO = 2;
    uint constant FEE = (5 / 1000) * 1e18;

    function fee() public pure returns (uint) { return FEE; }
    function directFee() public pure returns (uint) { return (5 / 1000) * 1e18; }
    function signedTyped() public pure returns (int) { return (NEGATIVE_SEVEN / SIGNED_TWO) * SIGNED_TWO; }
    function tiny() public pure returns (uint) { return (1 / (2 ** 300)) * (2 ** 300); }
    function seven() public pure returns (uint) { return (7 / 2) * 2; }
    function decimal() public pure returns (uint) { return 0.5 * 10; }
    function negative() public pure returns (int) { return (-7 / 2) * 2; }
    function remainder() public pure returns (uint) { return ((7 / 2) % (3 / 2)) * 2; }
    function inverse() public pure returns (uint) { return (1 / 2) ** -2; }
    function wide() public pure returns (uint) { return (1 << 300) >> 45; }
    function max() public pure returns (uint) { unchecked { return 2**256 - 1; } }
    function typed() public pure returns (uint) { return (SEVEN / TWO) * TWO; }
    function runtimeDivision(uint x) public pure returns (uint) { return (x / 2) * 2; }
    function compare() public pure returns (bool) { return (7 / 2) * 2 == 7 && (5 / 1000) * 1e18 == 5e15; }
}
